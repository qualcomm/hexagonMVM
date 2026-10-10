/*
 * Copyright (c) Qualcomm Technologies, Inc. and/or its subsidiaries.
 * SPDX-License-Identifier: BSD-3-Clause-Clear
 */

use crate::debug;
use crate::{ALT_STACK, CTX_OLD};
use minivm_sched::context::ThreadContext;

pub fn run() {
    debug::write0(b"  [test] context switch\n\0");

    extern "C" {
        static __global_pointer: u8;
        fn minivm_switch(old: *mut ThreadContext, new: *mut ThreadContext);
    }

    let mut ctx_new = ThreadContext::zeroed();

    // Set ELR to target function
    ctx_new.elr = super::guest_entries::context_switch_round_trip as *const () as u32;

    // SSR = 0 (monitor mode, no exceptions)
    ctx_new.ssr = 0;

    // Set up stack pointer and global pointer for the new context.
    // Use an alternative stack so we don't clobber minivm_main's stack.
    // r2928 = (r29_SP << 32) | r28_GP
    let alt_sp = unsafe { core::ptr::addr_of!(ALT_STACK).cast::<u8>().add(4096) as u32 };
    let gp = unsafe { &__global_pointer as *const u8 as u32 };
    ctx_new.r2928 = ((alt_sp as u64) << 32) | (gp as u64);

    // Enter exception mode (SSR.EX = bit 17, required for rte)
    unsafe {
        core::arch::asm!(
            "r0 = ##0x20000",
            "ssr = r0",
            "isync",
            out("r0") _,
        );
    }

    // Switch: saves current state to CTX_OLD, restores ctx_new, rte's.
    // When context_switch_round_trip switches back to CTX_OLD,
    // execution resumes here.
    unsafe {
        minivm_switch(
            core::ptr::addr_of_mut!(CTX_OLD),
            &mut ctx_new as *mut ThreadContext,
        );
    }

    debug::write0(b"  [test] context switch round-trip OK\n\0");
}
