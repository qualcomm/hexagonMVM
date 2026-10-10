/*
 * Copyright (c) Qualcomm Technologies, Inc. and/or its subsidiaries.
 * SPDX-License-Identifier: BSD-3-Clause-Clear
 */

//! Hexagon-specific register access via inline assembly.
//!
//! These functions provide safe wrappers around Hexagon supervisor registers.
//! On non-Hexagon targets, they are stubbed out for testing.

use minivm_types::regs::{Ccr, Ssr, Syscfg};

/// Read the SSR (Supervisor Status Register).
#[inline(always)]
pub fn read_ssr() -> Ssr {
    Ssr(hexagon_read_reg!("ssr", u32, 0))
}

/// Write the SSR.
#[inline(always)]
pub fn write_ssr(ssr: Ssr) {
    hexagon_write_reg!("ssr", ssr.0);
}

/// Read the CCR (Cache Control Register).
#[inline(always)]
pub fn read_ccr() -> Ccr {
    Ccr(hexagon_read_reg!("ccr", u32, 0))
}

/// Write the CCR.
#[inline(always)]
pub fn write_ccr(ccr: Ccr) {
    hexagon_write_reg!("ccr", ccr.0);
}

/// Read the SYSCFG register.
#[inline(always)]
pub fn read_syscfg() -> Syscfg {
    Syscfg(hexagon_read_reg!("syscfg", u32, 0))
}

/// Write the SYSCFG register.
#[inline(always)]
pub fn write_syscfg(cfg: Syscfg) {
    hexagon_write_reg!("syscfg", cfg.0);
}

/// Read the ELR (Exception Link Register).
#[inline(always)]
pub fn read_elr() -> u32 {
    hexagon_read_reg!("elr", u32, 0)
}

/// Write the ELR.
#[inline(always)]
pub fn write_elr(val: u32) {
    hexagon_write_reg!("elr", val);
}

/// Read the BADVA (Bad Virtual Address) register.
#[inline(always)]
pub fn read_badva() -> u32 {
    hexagon_read_reg!("badva", u32, 0)
}

/// Read the PCYCLELO/HI registers as a 64-bit value.
#[inline(always)]
pub fn read_pcycles() -> u64 {
    #[cfg(target_arch = "hexagon")]
    {
        let lo: u32;
        let hi: u32;
        unsafe {
            core::arch::asm!("{lo} = pcyclelo", lo = out(reg) lo, options(nomem, nostack));
            core::arch::asm!("{hi} = pcyclehi", hi = out(reg) hi, options(nomem, nostack));
        };
        ((hi as u64) << 32) | (lo as u64)
    }
    #[cfg(not(target_arch = "hexagon"))]
    0
}

/// Read the HTID (Hardware Thread ID) register.
#[inline(always)]
pub fn read_htid() -> u32 {
    hexagon_read_reg!("htid", u32, 0)
}

/// Read the REV (Core Revision) register.
#[inline(always)]
pub fn read_rev() -> u32 {
    hexagon_read_reg!("rev", u32, 0x65) // Default to V65 for tests
}

/// Read the MODECTL register.
#[inline(always)]
pub fn read_modectl() -> u32 {
    hexagon_read_reg!("modectl", u32, 0)
}

/// Write the MODECTL register.
#[inline(always)]
pub fn write_modectl(val: u32) {
    hexagon_write_reg!("modectl", val);
}

/// Read the SGP0 (Supervisor Global Pointer 0) register.
#[inline(always)]
pub fn read_sgp0() -> u32 {
    hexagon_read_reg!("sgp0", u32, 0)
}

/// Write SGP0.
#[inline(always)]
pub fn write_sgp0(val: u32) {
    hexagon_write_reg!("sgp0", val);
}

/// Read the SGP1 register.
#[inline(always)]
pub fn read_sgp1() -> u32 {
    hexagon_read_reg!("sgp1", u32, 0)
}

/// Write SGP1.
#[inline(always)]
pub fn write_sgp1(val: u32) {
    hexagon_write_reg!("sgp1", val);
}

/// Read the GELR (Guest Exception Link Register).
#[inline(always)]
pub fn read_gelr() -> u32 {
    hexagon_read_reg!("gelr", u32, 0)
}

/// Read the GOSP (Guest Old Stack Pointer).
#[inline(always)]
pub fn read_gosp() -> u32 {
    hexagon_read_reg!("gosp", u32, 0)
}

/// Instruction synchronization barrier.
#[inline(always)]
pub fn isync() {
    hexagon_insn!("isync");
}

/// Data synchronization barrier.
#[inline(always)]
pub fn syncht() {
    hexagon_insn!("syncht");
}

/// Barrier instruction.
#[inline(always)]
pub fn barrier() {
    hexagon_insn!("barrier");
}
