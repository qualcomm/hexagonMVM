/*
 * Copyright (c) Qualcomm Technologies, Inc. and/or its subsidiaries.
 * SPDX-License-Identifier: BSD-3-Clause-Clear
 */

use crate::debug;
use crate::{ALT_STACK, CTX_OLD, GUEST_VERSION_RESULT};
use minivm_sched::context::ThreadContext;

pub fn run() {
    debug::write0(b"  [test] guest mode execution\n\0");

    extern "C" {
        static __global_pointer: u8;
        fn minivm_switch(old: *mut ThreadContext, new: *mut ThreadContext);
    }

    // Set up a guest context with SSR.GUEST=1
    let mut guest_ctx = ThreadContext::zeroed();
    guest_ctx.elr = super::guest_entries::guest_test_entry as *const () as u32;
    // SSR with GUEST bit set (bit 19).
    // Also set ASID=0 and clear other bits.
    guest_ctx.ssr = 1 << 19; // SSR_GUEST_BIT

    // Set up stack and GP on the alternate stack
    let alt_sp = unsafe { core::ptr::addr_of!(ALT_STACK).cast::<u8>().add(4096) as u32 };
    let gp = unsafe { &__global_pointer as *const u8 as u32 };
    guest_ctx.r2928 = ((alt_sp as u64) << 32) | (gp as u64);

    // Set USR to boot defaults (needed for proper operation)
    // usrp30 = (usr << 32) | p3:0
    guest_ctx.usrp30 = (minivm_types::regs::boot_defaults::THREAD_USR as u64) << 32;

    // Clear guest version result
    unsafe {
        GUEST_VERSION_RESULT = 0;
    }

    // Enter exception mode (required for rte in minivm_switch)
    unsafe {
        core::arch::asm!(
            "r0 = ##0x20000",
            "ssr = r0",
            "isync",
            out("r0") _,
        );
    }

    // Switch to guest context (saves minivm_main state to CTX_OLD).
    // The guest runs, does trap1 calls, and eventually the "return"
    // handler switches back to CTX_OLD, resuming here.
    unsafe {
        minivm_switch(
            core::ptr::addr_of_mut!(CTX_OLD),
            &mut guest_ctx as *mut ThreadContext,
        );
    }

    // Verify the guest ran correctly.
    // The guest encodes its test results as a bitmask in r0 (exit code):
    //   bit 0: version == VM_VERSION
    //   bit 1: initial IE == 0
    //   bit 2: prev IE was 0 before enable
    //   bit 3: IE == 1 after enable
    //   bit 4: prev IE was 1 before disable
    //   bit 5: IE == 0 after disable
    // All bits set = 0x3F = 63 = all 6 checks passed.
    let guest_result = unsafe { GUEST_VERSION_RESULT };
    assert_eq!(guest_result, 0x3F);

    debug::write0(b"  [test] guest mode execution OK (6 VM traps)\n\0");
}
