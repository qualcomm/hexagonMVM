/*
 * Copyright (c) Qualcomm Technologies, Inc. and/or its subsidiaries.
 * SPDX-License-Identifier: BSD-3-Clause-Clear
 */

//! minivm: a small Hexagon Virtual Machine monitor.
//!
//! It runs one guest that speaks the Hexagon Virtual Machine interface (the
//! `trap1` calls of the HVM specification and the extensions the Linux
//! port uses), on one virtual CPU.
//!
//! The machine starts minivm as firmware and has already loaded the guest.
//! [`entry.S`](../entry.S) gets the boot thread to `minivm_main`, which
//! returns into `minivm_context_restore_rte`, which resumes whatever context
//! SGP0 points at. From then on the monitor only runs in the exception
//! handlers in `traps`.

#![no_std]
#![no_main]
#![feature(asm_experimental_arch)]

mod arch;
mod console;
mod discovery;
mod mon;
mod panic;
mod semihosting;
mod traps;

use minivm_types::arch::ArchVersion;
use minivm_types::asid::TranslationType;
use minivm_types::config::OffsetConfig;
use minivm_types::regs::boot_defaults::{THREAD_CCR, THREAD_USR};

use mon::VcpuInit;

core::arch::global_asm!(include_str!("entry.S"));
core::arch::global_asm!(include_str!("context.S"));

const MINIVM_LINK_ADDR: u32 = 0xff00_0000;

/// Where the guest is loaded and entered.
const GUEST_ADDR: u32 = 0xa000_0000;

const SIZE_4M: u32 = 5;
const CACHE_MEMORY: u32 = 0x7;

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

/// Boot thread entry, called from entry.S with the MMU on.
///
/// `phys_base` is where the monitor image was loaded.
#[no_mangle]
pub extern "C" fn minivm_main(_fdt_phys: u64, phys_base: u32) {
    extern "C" {
        static __image_end: u8;
    }
    let image_size = core::ptr::addr_of!(__image_end) as usize as u32 - MINIVM_LINK_ADDR;
    let Some(phys_end) = phys_base.checked_add(image_size) else {
        fatal(b"monitor physical range wraps", phys_base, image_size);
    };
    let monitor = (phys_base as u64, phys_end as u64);

    let plat = discovery::discover();

    console::puts(b"minivm: Hexagon VM (Rust)\n");
    console::field(b"  loaded at       ", phys_base);
    console::field(b"  core revision   ", plat.rev);
    console::field(b"  subsystem base  ", plat.ss_base);

    if plat.arch != Some(ArchVersion::V68) {
        fatal(
            b"unsupported core: this image targets the V68 family",
            plat.rev,
            0,
        );
    }

    {
        let mut m = mon::lock();
        m.plat = plat;
        m.phys_base = phys_base;
        m.phys_end = phys_end;
        if plat.jtlb_entries <= mon::FIRST_GUEST_TLB {
            fatal(b"TLB size not reported by the platform", 0, plat.jtlb_entries);
        }
        m.tlb_size = plat.jtlb_entries;
    }

    let entry = GUEST_ADDR;
    if (monitor.0..monitor.1).contains(&(entry as u64)) {
        fatal(b"guest entry is inside minivm", 0, entry);
    }
    if arch::phys_read32(entry as u64) == 0 {
        fatal(b"no guest image at the load address", 0, entry);
    }
    console::field(b"  guest entry     ", entry);

    let mut m = mon::lock();
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
    m.create_vcpu(
        0,
        VcpuInit {
            pc: entry,
            sp: 0,
            arg: 0,
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
