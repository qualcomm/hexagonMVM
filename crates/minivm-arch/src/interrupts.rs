/*
 * Copyright (c) Qualcomm Technologies, Inc. and/or its subsidiaries.
 * SPDX-License-Identifier: BSD-3-Clause-Clear
 */

//! Hardware interrupt control (IMASK, IPEND, IPI).

/// Read the IMASK register (interrupt mask for current HW thread).
#[inline(always)]
pub fn read_imask() -> u32 {
    hexagon_read_reg!("imask", u32, 0)
}

/// Write the IMASK register.
#[inline(always)]
pub fn write_imask(val: u32) {
    hexagon_write_reg!("imask", val);
}

/// Read the IPEND register (pending interrupts).
#[inline(always)]
pub fn read_ipend() -> u32 {
    hexagon_read_reg!("ipend", u32, 0)
}

/// Send an inter-processor interrupt.
///
/// `thread_mask` is a bitmask of hardware threads to interrupt.
#[inline(always)]
pub fn send_ipi(thread_mask: u32) {
    #[cfg(target_arch = "hexagon")]
    unsafe {
        core::arch::asm!("iassignw({mask})", mask = in(reg) thread_mask, options(nomem, nostack))
    };
    #[cfg(not(target_arch = "hexagon"))]
    {
        let _ = thread_mask;
    }
}

/// Clear a software interrupt.
#[inline(always)]
pub fn swi_clear(int_num: u32) {
    #[cfg(target_arch = "hexagon")]
    unsafe {
        core::arch::asm!("swi({num})", num = in(reg) int_num, options(nomem, nostack))
    };
    #[cfg(not(target_arch = "hexagon"))]
    {
        let _ = int_num;
    }
}

/// Enable interrupts (set IE bit in SSR).
#[inline(always)]
pub fn enable_interrupts() {
    #[cfg(target_arch = "hexagon")]
    unsafe {
        let ssr: u32;
        core::arch::asm!("{ssr} = ssr", ssr = out(reg) ssr, options(nomem, nostack));
        let ssr = ssr | (1 << minivm_types::consts::SSR_IE_BIT);
        core::arch::asm!("ssr = {ssr}", ssr = in(reg) ssr, options(nomem, nostack));
    }
}

/// Disable interrupts (clear IE bit in SSR). Returns previous SSR value.
#[inline(always)]
pub fn disable_interrupts() -> u32 {
    #[cfg(target_arch = "hexagon")]
    {
        let ssr: u32;
        unsafe {
            core::arch::asm!("{ssr} = ssr", ssr = out(reg) ssr, options(nomem, nostack));
            let new_ssr = ssr & !(1 << minivm_types::consts::SSR_IE_BIT);
            core::arch::asm!("ssr = {val}", val = in(reg) new_ssr, options(nomem, nostack));
        };
        ssr
    }
    #[cfg(not(target_arch = "hexagon"))]
    0
}
