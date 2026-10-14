/*
 * Copyright (c) Qualcomm Technologies, Inc. and/or its subsidiaries.
 * SPDX-License-Identifier: BSD-3-Clause-Clear
 */

//! minivm: a small Hexagon Virtual Machine monitor.
//!
//! It runs one guest that speaks the Hexagon Virtual Machine interface (the
//! `trap1` calls of the HVM specification and the extensions the Linux
//! port uses), with one virtual CPU per hardware thread. That is what a
//! Hexagon Linux kernel needs.
//!
//! The machine starts minivm as firmware, with a device tree pointer in
//! r1:r0, and has already loaded the guest. [`entry.S`](../entry.S) gets
//! each hardware thread to `minivm_main` or `minivm_secondary_main`; both
//! return into `minivm_context_restore_rte`, which resumes whatever context
//! SGP0 points at. From then on the monitor only runs in the exception
//! handlers in `traps`.

#![no_std]
#![no_main]
#![feature(asm_experimental_arch)]

mod arch;
mod cache;
mod console;
mod discovery;
mod hwint;
mod mon;
mod panic;
mod semihosting;
mod traps;

use minivm_types::arch::ArchVersion;
use minivm_types::asid::TranslationType;
use minivm_types::config::OffsetConfig;
use minivm_types::regs::boot_defaults::{THREAD_CCR, THREAD_USR};

use core::sync::atomic::{AtomicU32, Ordering};

use mon::{tlb, va, VcpuInit, MAX_HT};

core::arch::global_asm!(include_str!("entry.S"));
core::arch::global_asm!(include_str!("context.S"));

const MINIVM_LINK_ADDR: u32 = 0xff00_0000;

/// Where a guest is expected when there is no device tree to say.
const DEFAULT_GUEST_ADDR: u32 = 0xa000_0000;

const SIZE_4K: u32 = 0;
const SIZE_4M: u32 = 5;
const PERM_R: u32 = 0x2;
const PERM_RW: u32 = 0x6;
const CACHE_DEVICE: u32 = 0x4;
const CACHE_MEMORY: u32 = 0x7;

const SYSCFG_GIE: u32 = 1 << 4;
const SYSCFG_PCYCLEEN: u32 = 1 << 6;
const SYSCFG_PM: u32 = 1 << 9;
const SYSCFG_BQ: u32 = 1 << 13;
const SYSCFG_DMT: u32 = 1 << 15;

/// The monitor cannot continue. Say why and take the machine down.
pub fn fatal(what: &[u8], ssr: u32, elr: u32) -> ! {
    console::puts(b"minivm: FATAL: ");
    console::puts(what);
    console::puts(b"\n");
    console::field(b"  hthread ", arch::htid() as u32);
    console::field(b"  ssr     ", ssr);
    console::field(b"  elr     ", elr);
    console::field(b"  badva   ", arch::badva());
    die();
}

/// Stop every hardware thread, with a failing exit status.
pub fn die() -> ! {
    mon::abandon(1);
}

/// What the device tree says that the monitor cares about.
struct BootInfo {
    /// Physical address of the device tree, to hand to the guest.
    dtb: u32,
    /// Physical address of a PL011 to print on.
    uart: Option<u32>,
    /// Where the guest image is loaded and entered: `/chosen/hvm,guest-entry`
    /// if present, otherwise the start of the first `/memory` node.
    guest_entry: Option<u32>,
    /// A `/memory` range the guest would think it owns that holds part of
    /// the monitor, with nothing in `/reserved-memory` or the memory
    /// reservation block to keep it off.
    monitor_exposed: Option<(u64, u64)>,
    caches: cache::DtCaches,
}

/// A property holding one address or size, of one or two cells.
fn prop_u64(node: &fdt::node::FdtNode, name: &str) -> Option<u64> {
    let v = node.property(name)?.value;
    match v.len() {
        4 => Some(u32::from_be_bytes(v.try_into().ok()?) as u64),
        8 => Some(u64::from_be_bytes(v.try_into().ok()?)),
        _ => None,
    }
}

fn prop_u32(node: &fdt::node::FdtNode, name: &str) -> Option<u32> {
    prop_u64(node, name).and_then(|v| u32::try_from(v).ok())
}

/// The ranges of a node's `reg`, as (start, end).
fn reg_ranges<'b>(node: fdt::node::FdtNode<'b, '_>) -> impl Iterator<Item = (u64, u64)> + 'b {
    node.reg().into_iter().flatten().filter_map(|r| {
        let start = r.starting_address as usize as u64;
        start
            .checked_add(r.size.unwrap_or(0) as u64)
            .map(|end| (start, end))
    })
}

/// A `/memory` range overlapping `[lo, hi)`, unless something reserves
/// all of `[lo, hi)`.
fn find_exposed(fdt: &fdt::Fdt, lo: u64, hi: u64) -> Option<(u64, u64)> {
    let covers = |(s, e): (u64, u64)| s <= lo && hi <= e;
    let reserved_node = fdt
        .find_node("/reserved-memory")
        .into_iter()
        .flat_map(|n| n.children())
        .flat_map(reg_ranges)
        .any(covers);
    let reserved_block = fdt.memory_reservations().any(|r| {
        let s = r.address() as usize as u64;
        s.checked_add(r.size() as u64)
            .is_some_and(|e| covers((s, e)))
    });
    if reserved_node || reserved_block {
        return None;
    }
    fdt.all_nodes()
        .filter(|n| n.name == "memory" || n.name.starts_with("memory@"))
        .flat_map(reg_ranges)
        .find(|&(s, e)| s < hi && lo < e)
}

/// Look at the device tree the machine passed, if it passed one.
/// `monitor` is the physical range the monitor occupies.
///
/// The blob is mapped only for the duration of the call.
fn read_fdt(fdt_phys: u64, monitor: (u64, u64)) -> BootInfo {
    const WINDOW: u32 = 0x40_0000;
    let mut info = BootInfo {
        dtb: 0,
        uart: None,
        guest_entry: None,
        monitor_exposed: None,
        caches: cache::DtCaches::default(),
    };

    if fdt_phys == 0 || fdt_phys > u32::MAX as u64 || fdt_phys & 3 != 0 {
        return info;
    }
    // 0xd00dfeed, stored big-endian.
    if arch::phys_read32(fdt_phys) != 0xedfe_0dd0 {
        return info;
    }
    let pa = fdt_phys as u32;
    let size = u32::from_be(arch::phys_read32(fdt_phys + 4));
    let base = pa & !(WINDOW - 1);
    let offset = pa - base;
    let Some(end) = offset.checked_add(size) else {
        return info;
    };
    let Some(second_base) = base.checked_add(WINDOW) else {
        return info;
    };
    if size < 40 || end > 2 * WINDOW {
        return info;
    }
    info.dtb = pa;

    unsafe {
        arch::tlbw(
            arch::tlb_entry(va::FDT, base, SIZE_4M, PERM_R, CACHE_MEMORY),
            tlb::FDT0,
        );
        arch::tlbw(
            arch::tlb_entry(va::FDT + WINDOW, second_base, SIZE_4M, PERM_R, CACHE_MEMORY),
            tlb::FDT1,
        );
    }
    let blob =
        unsafe { core::slice::from_raw_parts((va::FDT + offset) as *const u8, size as usize) };
    if let Ok(fdt) = fdt::Fdt::new(blob) {
        let first_reg = |node: fdt::node::FdtNode| {
            node.reg()
                .and_then(|mut r| r.next())
                .and_then(|r| u32::try_from(r.starting_address as u64).ok())
        };
        info.uart = fdt.find_compatible(&["arm,pl011"]).and_then(first_reg);
        info.guest_entry = fdt
            .find_node("/chosen")
            .and_then(|c| prop_u32(&c, "hvm,guest-entry"))
            .or_else(|| fdt.find_node("/memory").and_then(first_reg));
        info.monitor_exposed = find_exposed(&fdt, monitor.0, monitor.1);
        if let Some(cpu) = fdt.find_node("/cpus/cpu@0") {
            info.caches.d_size = prop_u32(&cpu, "d-cache-size");
            info.caches.d_line = prop_u32(&cpu, "d-cache-line-size");
            info.caches.d_sets = prop_u32(&cpu, "d-cache-sets");
            info.caches.l2_size = prop_u32(&cpu, "next-level-cache")
                .and_then(|ph| fdt.find_phandle(ph))
                .and_then(|l2| prop_u32(&l2, "cache-size"));
        }
    }
    unsafe {
        arch::tlbw(0, tlb::FDT0);
        arch::tlbw(0, tlb::FDT1);
    }
    info
}

/// Boot thread entry, called from entry.S with the MMU on.
///
/// `fdt_phys` is what the machine passed in r1:r0 and `phys_base` is where
/// the monitor image was loaded.
#[no_mangle]
pub extern "C" fn minivm_main(fdt_phys: u64, phys_base: u32) {
    extern "C" {
        static __image_end: u8;
    }
    let image_size = core::ptr::addr_of!(__image_end) as usize as u32 - MINIVM_LINK_ADDR;
    let Some(phys_end) = phys_base.checked_add(image_size) else {
        fatal(b"monitor physical range wraps", phys_base, image_size);
    };
    let monitor = (phys_base as u64, phys_end as u64);

    let plat = discovery::discover();
    let boot = read_fdt(fdt_phys, monitor);

    if let Some(uart) = boot.uart {
        unsafe {
            arch::tlbw(
                arch::tlb_entry(va::UART, uart & !0xfff, SIZE_4K, PERM_RW, CACHE_DEVICE),
                tlb::UART,
            );
        }
        console::use_uart(va::UART + (uart & 0xfff));
    }
    console::puts(b"minivm: Hexagon VM (Rust)\n");
    console::field(b"  loaded at       ", phys_base);
    console::field(b"  core revision   ", plat.rev);
    console::field(b"  subsystem base  ", plat.ss_base);

    if plat.ss_base == 0 {
        fatal(b"no config table: cannot find the L2VIC and QTIMER", 0, 0);
    }
    if plat.arch != Some(ArchVersion::V68) {
        fatal(
            b"unsupported core: this image targets the V68 family",
            plat.rev,
            0,
        );
    }
    if let Some((start, end)) = boot.monitor_exposed {
        // The guest would take minivm's memory for its own. A
        // /reserved-memory node covering the monitor keeps it off.
        console::field(b"  monitor size    ", image_size);
        console::field(b"  memory node     ", start as u32);
        console::field(b"  memory node end ", end as u32);
        fatal(
            b"guest memory holds minivm; reserve it in /reserved-memory",
            0,
            0,
        );
    }
    unsafe { cache::init(&plat, &boot.caches) };
    // The L2VIC and QTIMER sit a few pages apart in the subsystem block;
    // one 4M page covers both.
    let dev_pa = plat.ss_base & !0x3f_ffff;
    let dev_end = plat.qtimer_base + 0x2000;
    if dev_end - dev_pa > 0x40_0000 {
        fatal(b"subsystem block straddles a 4M boundary", plat.ss_base, 0);
    }
    unsafe {
        arch::tlbw(
            arch::tlb_entry(va::DEVICES, dev_pa, SIZE_4M, PERM_RW, CACHE_DEVICE),
            tlb::DEVICES,
        );
        hwint::init(
            va::DEVICES + (plat.l2vic_base - dev_pa),
            va::DEVICES + (plat.qtimer_base - dev_pa),
        );
        hwint::init_thread(0);
        // Interrupts on, pcycle counter and PMU running, and the
        // translation and bus settings Linux expects.
        arch::set_syscfg(
            (arch::syscfg() | SYSCFG_GIE | SYSCFG_PCYCLEEN | SYSCFG_PM | SYSCFG_DMT) & !SYSCFG_BQ,
        );
    }

    {
        let mut m = mon::lock();
        m.plat = plat;
        m.phys_base = phys_base;
        m.phys_end = phys_end;
        m.devices_base = dev_pa;
        if plat.jtlb_entries <= mon::FIRST_GUEST_TLB {
            fatal(b"TLB size not reported by the platform", 0, plat.jtlb_entries);
        }
        m.tlb_size = plat.jtlb_entries;
        m.cpus[0].started = true;
    }

    let hthreads = start_secondaries(plat.thread_mask);
    console::field(b"  hardware threads ", hthreads);

    let entry = boot.guest_entry.unwrap_or(DEFAULT_GUEST_ADDR);
    if (monitor.0..monitor.1).contains(&(entry as u64)) {
        fatal(b"guest entry is inside minivm", 0, entry);
    }
    if arch::phys_read32(entry as u64) == 0 {
        fatal(b"no guest image at the load address", 0, entry);
    }
    console::field(b"  guest entry     ", entry);
    console::field(b"  device tree     ", boot.dtb);

    let mut m = mon::lock();
    m.hthreads = hthreads;
    // The guest starts at its load address under an identity map of all of
    // guest-physical memory, which is what Linux expects of its firmware:
    // head.S works out PHYS_OFFSET from where it finds itself running.
    let identity = OffsetConfig::new(SIZE_4M as u8, CACHE_MEMORY as u8, false, 0xF, 0);
    let Some(asid) = m.asids.inc(
        identity.0,
        TranslationType::Offset,
        false,
        0,
        mon::VM_IDX,
        |_| {},
    ) else {
        fatal(b"no ASID for the guest", 0, 0);
    };
    // Linux takes the device tree pointer in r0.
    m.create_vcpu(
        0,
        VcpuInit {
            pc: entry,
            sp: 0,
            arg: boot.dtb,
            asid,
            usr: THREAD_USR,
            ccr: THREAD_CCR,
            gevb: 0,
        },
    );
    console::puts(b"minivm: starting guest\n");
    console::end_early();
    m.leave(0);
}

/// Hardware threads that have reached `minivm_secondary_main`, for the boot
/// thread to watch without taking the monitor lock: a thread polling the
/// lock in a loop can keep the others from ever getting it.
static THREADS_UP: AtomicU32 = AtomicU32::new(1);

/// How long to wait for the other hardware threads to come up.
const THREAD_START_TIMEOUT_TICKS: u64 = hwint::TIMER_HZ / 10;

/// Start the other hardware threads and wait for them to reach the idle
/// loop. Returns the mask of threads that are running, this one included.
fn start_secondaries(thread_mask: u32) -> u32 {
    let want = thread_mask & ((1 << MAX_HT) - 1) & !1;
    if want == 0 {
        return 1;
    }
    unsafe { arch::start_threads(want) };
    // MODECTL's enable bits show which of them actually exist.
    let present = arch::modectl() & want;
    let deadline = hwint::read_count() + THREAD_START_TIMEOUT_TICKS;
    while THREADS_UP.load(Ordering::Acquire) & present != present && hwint::read_count() < deadline
    {
        core::hint::spin_loop();
    }
    THREADS_UP.load(Ordering::Acquire)
}

/// Entry for every hardware thread but the boot thread: go idle until a
/// virtual CPU is started here.
#[no_mangle]
pub extern "C" fn minivm_secondary_main() {
    let cpu = arch::htid();
    unsafe { hwint::init_thread(cpu) };
    let mut m = mon::lock();
    m.cpus[cpu].started = true;
    THREADS_UP.fetch_or(1 << cpu, Ordering::Release);
    m.leave(cpu);
}
