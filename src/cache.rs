/*
 * Copyright (c) Qualcomm Technologies, Inc. and/or its subsidiaries.
 * SPDX-License-Identifier: BSD-3-Clause-Clear
 */

//! Cache configuration and maintenance.
//!
//! entry.S invalidates the caches and turns the L1 caches on before the MMU
//! comes on, then [`init`] sizes and enables the L2, and [`vmcache`] carries
//! out the guest's `vmcache` requests.
//!
//! What can be discovered is: the L2 size comes from the config table, for
//! v66 and later. What cannot be is the L1 data cache's geometry, which the
//! by-index maintenance operations need; it comes from the standard device
//! tree cache properties of `/cpus/cpu@0`, and where those are missing,
//! from a default geometry that suits v5 and later.

use crate::arch;
use crate::console;
use crate::discovery::Platform;

const SYSCFG_L2CFG_SHIFT: u32 = 16;
const SYSCFG_L2CFG: u32 = 0x7 << SYSCFG_L2CFG_SHIFT;
const SYSCFG_L2NWA: u32 = 1 << 21;
const SYSCFG_L2NRA: u32 = 1 << 22;
const SYSCFG_L2WB: u32 = 1 << 23;
/// L2CFG value that means "all of the L2 is cache" for the largest tag
/// arrays.
const L2CFG_MAX: u32 = 6;
/// L2 cache size per L2CFG step: L2CFG n gives `L2_TAG_CHUNK << n` bytes.
const L2_TAG_CHUNK: u32 = 0x8000;

/// What the device tree says about the caches.
#[derive(Clone, Copy, Default)]
pub struct DtCaches {
    /// `d-cache-size`, `d-cache-line-size` and `d-cache-sets` of
    /// `/cpus/cpu@0`.
    pub d_size: Option<u32>,
    pub d_line: Option<u32>,
    pub d_sets: Option<u32>,
    /// `cache-size` of the cache `/cpus/cpu@0` names as `next-level-cache`.
    pub l2_size: Option<u32>,
}

/// L1 data cache geometry as the by-index operations see it: the index of
/// set `s`, way `w` is `s * stride + w`.
#[derive(Clone, Copy)]
struct L1Geometry {
    stride: u32,
    sets: u32,
    ways: u32,
}

/// The L1 data cache of v5 and later: 32K, 8 ways, sets indexed from
/// bit 5. On a smaller or longer-lined cache this visits some sets more
/// than once, which is harmless.
const DEFAULT_DCACHE: L1Geometry = L1Geometry {
    stride: 32,
    sets: 32 * 1024 / 32 / 8,
    ways: 8,
};

/// Set once by `init`, before any other thread runs.
static mut DCACHE: L1Geometry = DEFAULT_DCACHE;

/// Choose the L1 data cache geometry. The device tree describes the cache
/// exactly; without it, use the default, grown to the size the config table
/// reports if that is larger.
fn dcache_geometry(plat: &Platform, dt: &DtCaches) -> L1Geometry {
    if let (Some(size), Some(line), Some(sets)) = (dt.d_size, dt.d_line, dt.d_sets) {
        if let Some(denom) = line.checked_mul(sets) {
            if line.is_power_of_two()
                && sets != 0
                && denom != 0
                && size % denom == 0
                && size / denom <= line
            {
                return L1Geometry {
                    stride: line,
                    sets,
                    ways: size / denom,
                };
            }
        }
    }
    let size = dt.d_size.unwrap_or(plat.l1d_size_kb * 1024).max(32 * 1024);
    L1Geometry {
        sets: size / DEFAULT_DCACHE.stride / DEFAULT_DCACHE.ways,
        ..DEFAULT_DCACHE
    }
}

/// The L2CFG value for the cache size the platform reports, if it reports
/// one.
fn l2cfg(plat: &Platform, dt: &DtCaches) -> Option<u32> {
    // The config table gives the tag array size.
    match plat.l2tag_size {
        0 => {}
        1536 => return Some(L2CFG_MAX),
        tags if tags.is_power_of_two() && tags >= 32 => {
            return Some((tags.trailing_zeros() - 5).min(L2CFG_MAX))
        }
        _ => {}
    }
    // Otherwise the device tree's L2 size: L2CFG n is L2_TAG_CHUNK << n bytes.
    let size = dt.l2_size?;
    if size < L2_TAG_CHUNK || !size.is_power_of_two() {
        return None;
    }
    Some((size / L2_TAG_CHUNK).trailing_zeros().min(L2CFG_MAX))
}

/// Clean and invalidate the whole L2, and wait for the hardware to finish.
fn l2_cleaninv() {
    unsafe { arch::l2gcleaninv() };
    while arch::syscfg() & arch::SYSCFG_L2GCA != 0 {
        core::hint::spin_loop();
    }
    arch::syncht();
}

/// Size the L2 and make it write-back. Must run while no other hardware
/// thread runs.
///
/// # Safety
/// Changes the cache configuration of the whole core.
unsafe fn configure_l2(cfg: u32) {
    let mut syscfg = arch::syscfg();
    let alloc = syscfg & (SYSCFG_L2NWA | SYSCFG_L2NRA);

    // Write-through and no allocation while everything is cleaned out...
    syscfg &= !(SYSCFG_L2WB | SYSCFG_L2NWA | SYSCFG_L2NRA);
    arch::set_syscfg(syscfg);
    arch::syncht();
    l2_cleaninv();

    // ...then through size zero to the new size.
    syscfg &= !SYSCFG_L2CFG;
    arch::set_syscfg(syscfg);
    arch::l2kill();

    syscfg |= (cfg << SYSCFG_L2CFG_SHIFT) | SYSCFG_L2WB | alloc;
    arch::syncht();
    arch::set_syscfg(syscfg);
    arch::syncht();
}

/// Bring the caches to their running configuration. entry.S has already
/// turned the L1 caches on.
///
/// # Safety
/// Must run on the boot thread before any other thread is started.
pub unsafe fn init(plat: &Platform, dt: &DtCaches) {
    let g = dcache_geometry(plat, dt);
    DCACHE = g;
    console::field(b"  L1D sets/ways   ", (g.sets << 16) | g.ways);

    match l2cfg(plat, dt) {
        Some(cfg) => {
            configure_l2(cfg);
            console::field(b"  L2CFG           ", cfg);
        }
        None => console::puts(b"  L2 size unknown: left as found\n"),
    }
}

/// Run `op` on every line of the L1 data cache, by index.
fn dcache_by_index(op: unsafe fn(u32)) {
    let g = unsafe { DCACHE };
    for way in 0..g.ways {
        for set in 0..g.sets {
            unsafe { op(set * g.stride + way) };
        }
    }
    arch::syncht();
}

/// `vmcache`. Operations name an address range, but this works on
/// whole caches: the range is a guest virtual address that the monitor
/// could only touch through the guest's own translations. Returns -1 for an
/// unknown operation.
pub fn vmcache(op: u32) -> i32 {
    const ICKILL: u32 = 0;
    const DCKILL: u32 = 1;
    const L2KILL: u32 = 2;
    const DCCLEANINVA: u32 = 3;
    const ICINVA: u32 = 4;
    const IDSYNC: u32 = 5;

    match op {
        ICKILL | ICINVA => arch::ickill(),
        // Invalidating without cleaning would throw away data the guest
        // does not own as well as its own, so a kill is a clean too.
        DCKILL | DCCLEANINVA => dcache_by_index(arch::dccleaninvidx),
        L2KILL => l2_cleaninv(),
        // New instructions in memory: data out, stale instructions gone.
        IDSYNC => {
            dcache_by_index(arch::dccleanidx);
            arch::ickill();
        }
        _ => return -1,
    }
    0
}
