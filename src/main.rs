/*
 * Copyright (c) Qualcomm Technologies, Inc. and/or its subsidiaries.
 * SPDX-License-Identifier: BSD-3-Clause-Clear
 */

//! minivm: a small Hexagon Virtual Machine monitor.
//!
//! It runs one guest that speaks the Hexagon Virtual Machine interface (the
//! `trap1` calls of the HVM specification and the extensions the Linux
//! port uses), with one virtual CPU per hardware thread.
//!
//! The machine starts minivm as firmware and has already loaded the guest.
//! [`entry.S`](../entry.S) gets each hardware thread to `minivm_main` or
//! `minivm_secondary_main`; both return into `minivm_context_restore_rte`,
//! which resumes whatever context SGP0 points at. From then on the monitor
//! only runs in the exception handlers in `traps`.

#![no_std]
#![no_main]
#![feature(asm_experimental_arch)]

mod arch;
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

/// Where the guest is loaded and entered.
const GUEST_ADDR: u32 = 0xa000_0000;

const SIZE_4M: u32 = 5;
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
