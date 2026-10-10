/*
 * Copyright (c) Qualcomm Technologies, Inc. and/or its subsidiaries.
 * SPDX-License-Identifier: BSD-3-Clause-Clear
 */

//! Cache operations (dcclean, icinva, syncht, etc.).

/// D-cache clean (write-back) by address.
#[inline(always)]
pub fn dccleana(addr: *const u8) {
    hexagon_insn_r!("dccleana", addr);
}

/// D-cache invalidate by address.
#[inline(always)]
pub fn dcinva(addr: *const u8) {
    hexagon_insn_r!("dcinva", addr);
}

/// D-cache clean and invalidate by address.
#[inline(always)]
pub fn dccleaninva(addr: *const u8) {
    hexagon_insn_r!("dccleaninva", addr);
}

/// D-cache zero-allocate by address (allocate a cache line and zero it).
#[inline(always)]
pub fn dczeroa(addr: *mut u8) {
    hexagon_insn_r!("dczeroa", addr);
}

/// I-cache invalidate by address.
#[inline(always)]
pub fn icinva(addr: *const u8) {
    hexagon_insn_r!("icinva", addr);
}

/// L2 fetch (prefetch into L2 cache).
///
/// Uses `l2fetch(Rs, Rtt)` — the 64-bit config is composed from two GPRs
/// via `combine` into an explicit register pair.
#[inline(always)]
pub fn l2fetch(addr: *const u8, config: u64) {
    #[cfg(target_arch = "hexagon")]
    unsafe {
        let lo = config as u32;
        let hi = (config >> 32) as u32;
        core::arch::asm!(
            "r7:6 = combine({hi}, {lo})",
            "l2fetch({addr}, r7:6)",
            lo = in(reg) lo,
            hi = in(reg) hi,
            addr = in(reg) addr,
            out("r6") _,
            out("r7") _,
            options(nostack),
        );
    }
    #[cfg(not(target_arch = "hexagon"))]
    {
        let _ = (addr, config);
    }
}
