/*
 * Copyright (c) Qualcomm Technologies, Inc. and/or its subsidiaries.
 * SPDX-License-Identifier: BSD-3-Clause-Clear
 */

//! Thin wrappers over the privileged instructions and registers the monitor
//! uses.

use core::arch::asm;
use minivm_types::arch::ArchVersion;

/// Snapshot of the core interrupt pending and auto-disable masks.
#[derive(Clone, Copy)]
pub struct InterruptState {
    pub pending: u32,
    pub auto_disable: u32,
}

struct InterruptLayout {
    pending_mask: u32,
    auto_disable_mask: u32,
    auto_disable_shift: u32,
}

// Before V81 both masks are 16 bits wide, packed in IPENDAD.
const PACKED_IPENDAD: InterruptLayout = InterruptLayout {
    pending_mask: 0xffff,
    auto_disable_mask: 0xffff_0000,
    auto_disable_shift: 16,
};

// From V81 the native IPEND and IAD registers are 32 bits wide. IPENDAD stays
// as a compatibility view of their low sixteen bits, in the same positions as
// the packed layout, which is what is read here.
const SPLIT_IPEND_IAD: InterruptLayout = InterruptLayout {
    pending_mask: 0xffff,
    auto_disable_mask: 0xffff_0000,
    auto_disable_shift: 16,
};

/// Read the interrupt state using the layout of `arch`.
///
/// Before V81 both masks are packed in IPENDAD. From V81 they are separate
/// IPEND and IAD registers, and IPENDAD supplies the low 16 bits of each.
/// minivm only uses core interrupts 0 and 2, which are in that common range.
/// A core whose revision is not known is read as IPENDAD.
#[inline(always)]
pub fn interrupt_state(arch: Option<ArchVersion>) -> InterruptState {
    let ipendad: u32;
    unsafe { asm!("{v} = s20", v = out(reg) ipendad, options(nomem, nostack)) };
    let layout = if arch.is_some_and(ArchVersion::has_split_ipend_iad) {
        SPLIT_IPEND_IAD
    } else {
        PACKED_IPENDAD
    };
    InterruptState {
        pending: ipendad & layout.pending_mask,
        auto_disable: (ipendad & layout.auto_disable_mask) >> layout.auto_disable_shift,
    }
}

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

/// Hardware thread mode control: enable bits low, wait bits high.
#[inline(always)]
pub fn modectl() -> u32 {
    let v: u32;
    unsafe { asm!("{v} = modectl", v = out(reg) v, options(nomem, nostack)) };
    v
}

/// # Safety
/// Changes global core configuration.
#[inline(always)]
pub unsafe fn set_syscfg(v: u32) {
    asm!("syscfg = {v}", "isync", v = in(reg) v, options(nomem, nostack));
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

/// Set which core interrupts this thread refuses (a set bit masks).
///
/// # Safety
/// Changes interrupt routing for the calling thread.
#[inline(always)]
pub unsafe fn set_imask(mask: u32) {
    asm!("imask = {v}", v = in(reg) mask, options(nomem, nostack));
}

/// SYSCFG.L2GCA: a global L2 operation is still running.
pub const SYSCFG_L2GCA: u32 = 1 << 31;

/// Wait for outstanding memory transactions.
#[inline(always)]
pub fn syncht() {
    unsafe { asm!("syncht", options(nostack)) };
}

/// Invalidate the whole L1 instruction cache.
#[inline(always)]
pub fn ickill() {
    unsafe { asm!("ickill", "isync", options(nostack)) };
}

/// Invalidate the whole L2, discarding dirty lines.
///
/// # Safety
/// Loses whatever the L2 holds that memory does not.
#[inline(always)]
pub unsafe fn l2kill() {
    asm!("l2kill", "isync", options(nostack));
}

/// Start cleaning and invalidating the whole L2. It completes when
/// SYSCFG.L2GCA reads clear.
///
/// # Safety
/// Changes cache state for every thread.
#[inline(always)]
pub unsafe fn l2gcleaninv() {
    asm!("l2gcleaninv", options(nostack));
}

/// Clean and invalidate one L1 data cache line, by set and way.
///
/// # Safety
/// Changes cache state.
#[inline(always)]
pub unsafe fn dccleaninvidx(idx: u32) {
    asm!("dccleaninvidx({i})", i = in(reg) idx, options(nostack));
}

/// Clean one L1 data cache line, by set and way.
///
/// # Safety
/// Changes cache state.
#[inline(always)]
pub unsafe fn dccleanidx(idx: u32) {
    asm!("dccleanidx({i})", i = in(reg) idx, options(nostack));
}

/// Choose which hardware threads may take core interrupt `int` (0..15): a
/// set bit in `masked` masks it for that thread. Writes every thread's
/// IMASK bit for `int`.
///
/// # Safety
/// Changes interrupt routing for every thread.
#[inline(always)]
pub unsafe fn iassignw(int: u32, masked: u32) {
    let v = (int << 16) | (masked & 0xffff);
    asm!("iassignw({v})", v = in(reg) v, options(nomem, nostack));
}

/// Raise core interrupts.
#[inline(always)]
pub fn swi(mask: u32) {
    unsafe { asm!("swi({v})", v = in(reg) mask, options(nomem, nostack)) };
}

/// Cancel pending software interrupts.
#[inline(always)]
pub fn cswi(mask: u32) {
    unsafe { asm!("cswi({v})", v = in(reg) mask, options(nomem, nostack)) };
}

/// Clear core interrupts' auto-disable bits, making them deliverable again.
///
/// For the L2VIC's interrupt this is also the controller's acknowledge: it
/// retires the line being presented. QEMU does that for any mask. See
/// `hwint::ack`.
///
/// # Safety
/// The caller must have dealt with the line the L2VIC is presenting.
#[inline(always)]
pub unsafe fn ciad(mask: u32) {
    asm!("ciad({v})", v = in(reg) mask, options(nomem, nostack));
}

/// Start hardware threads. They begin at the reset vector.
///
/// # Safety
/// The reset path must be ready for them.
#[inline(always)]
pub unsafe fn start_threads(mask: u32) {
    asm!("start({v})", v = in(reg) mask, options(nomem, nostack));
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

/// Build a raw hardware TLB entry (sentinel-PPD format).
///
/// `size` is the page-size code (0 = 4K, 1 = 16K, ... 4 = 1M, 5 = 4M,
/// 6 = 16M); `va` and `pa` must be aligned to it. `xwru` is the permission
/// nibble and `cccc` the cache attribute.
pub const fn tlb_entry(va: u32, pa: u32, size: u32, xwru: u32, cccc: u32) -> u64 {
    minivm_types::tlb::TlbEntry::global_mapping(va, pa, size as u8, xwru as u8, cccc as u8).0
}

/// MMIO read.
///
/// # Safety
/// `addr` must be a mapped device register.
#[inline(always)]
pub unsafe fn mmio_read(addr: u32) -> u32 {
    core::ptr::read_volatile(addr as *const u32)
}

/// MMIO write.
///
/// # Safety
/// `addr` must be a mapped device register.
#[inline(always)]
pub unsafe fn mmio_write(addr: u32, val: u32) {
    core::ptr::write_volatile(addr as *mut u32, val);
}
