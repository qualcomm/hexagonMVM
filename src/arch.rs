/*
 * Copyright (c) Qualcomm Technologies, Inc. and/or its subsidiaries.
 * SPDX-License-Identifier: BSD-3-Clause-Clear
 */

//! Thin wrappers over the privileged instructions and registers the monitor
//! uses.

use core::arch::asm;

/// Hardware thread number of the caller.
#[inline(always)]
pub fn htid() -> usize {
    let v: u32;
    unsafe { asm!("{v} = htid", v = out(reg) v, options(nomem, nostack)) };
    v as usize
}

/// Faulting address of the exception being handled.
#[inline(always)]
pub fn badva() -> u32 {
    let v: u32;
    unsafe { asm!("{v} = badva", v = out(reg) v, options(nomem, nostack)) };
    v
}

#[inline(always)]
pub fn syscfg() -> u32 {
    let v: u32;
    unsafe { asm!("{v} = syscfg", v = out(reg) v, options(nomem, nostack)) };
    v
}

/// Point this thread's exception entry at the context to save into, which
/// is also the context `minivm_context_restore_rte` resumes.
///
/// # Safety
/// `ctx` must be a live `ThreadContext`.
#[inline(always)]
pub unsafe fn set_sgp0(ctx: u32) {
    asm!("sgp0 = {v}", v = in(reg) ctx, options(nomem, nostack));
}

extern "C" {
    /// Take the calling hardware thread off line. When the last one goes,
    /// QEMU exits with `code`.
    pub fn minivm_stop_thread(code: u32) -> !;
    fn minivm_tlb_insert(entry: u64, index: u32);
}

/// Write a hardware TLB entry.
///
/// # Safety
/// The caller must hold the monitor lock and must not create an entry that
/// overlaps another one.
#[inline(always)]
pub unsafe fn tlbw(entry: u64, index: u32) {
    minivm_tlb_insert(entry, index);
}

/// Probe the TLB for `(asid << 20) | vpn`. Bit 31 of the result is set when
/// nothing matches; otherwise it is the index of the matching entry.
#[inline(always)]
pub fn tlbp(key: u32) -> u32 {
    let v: u32;
    unsafe { asm!("{v} = tlbp({k})", v = out(reg) v, k = in(reg) key, options(nomem, nostack)) };
    v
}

/// Drop every non-global TLB entry belonging to `asid`.
///
/// # Safety
/// The caller must hold the monitor lock.
#[inline(always)]
pub unsafe fn tlbinvasid(asid: u32) {
    asm!("tlbinvasid({a})", "isync", a = in(reg) asid << 20, options(nomem, nostack));
}

/// Read a word of physical memory without going through the TLB.
#[inline(always)]
pub fn phys_read32(pa: u64) -> u32 {
    let upper = (pa >> 11) as u32;
    let lower = (pa & 0x7fc) as u32;
    let v: u32;
    unsafe {
        asm!(
            "{v} = memw_phys({l},{u})",
            v = out(reg) v, l = in(reg) lower, u = in(reg) upper,
            options(nostack)
        );
    }
    v
}
