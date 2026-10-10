/*
 * Copyright (c) Qualcomm Technologies, Inc. and/or its subsidiaries.
 * SPDX-License-Identifier: BSD-3-Clause-Clear
 */

use crate::debug;
use crate::{AlignedStack, ALT_STACK};
use minivm_types::vm::VM_VERSION;

pub fn run() {
    debug::write0(b"  [test] VMOP_BOOT end-to-end\n\0");

    // SGP0 and globals already set up from CONFIG/VMOP test above.
    // Create a new child VM for this test.

    // CONFIG SET_CPUS_INTS
    let vm_idx: u32;
    unsafe {
        core::arch::asm!(
            "trap0(#30)",
            inout("r0") 0u32 => vm_idx,
            in("r1") 0u32,
            in("r2") 3u32,               // SET_CPUS_INTS
            in("r3") 4u32,               // 4 CPUs
            in("r4") 288u32,             // 288 interrupts
            out("r5") _,
        );
    }
    assert!(vm_idx > 0, "SET_CPUS_INTS failed");

    // CONFIG SET_FENCES
    let fence_r: u32;
    unsafe {
        core::arch::asm!(
            "trap0(#30)",
            inout("r0") 0u32 => fence_r,
            in("r1") vm_idx,
            in("r2") 1u32,               // SET_FENCES
            in("r3") 0u32,
            in("r4") 0xFE000u32,
            out("r5") _,
        );
    }
    assert_eq!(fence_r, vm_idx);

    // CONFIG SET_PRIO_TRAPMASK
    let trap_r: u32;
    unsafe {
        core::arch::asm!(
            "trap0(#30)",
            inout("r0") 0u32 => trap_r,
            in("r1") vm_idx,
            in("r2") 2u32,               // SET_PRIO_TRAPMASK
            in("r3") 0u32,
            in("r4") 0xFFFF_FFFFu32,
            out("r5") _,
        );
    }
    assert_eq!(trap_r, vm_idx);

    // CONFIG SET_PMAP_TYPE (identity offset, SIZE_16M for kernel addresses)
    let pmap_r: u32;
    unsafe {
        core::arch::asm!(
            "trap0(#30)",
            inout("r0") 0u32 => pmap_r,
            in("r1") vm_idx,
            in("r2") 0u32,               // SET_PMAP_TYPE
            in("r3") 0x0F70u32,          // OffsetConfig: size=0, cccc=7, weak=1, xwru=F, pages=0
            in("r4") 1u32,               // OFFSET type
            out("r5") _,
        );
    }
    assert_eq!(pmap_r, vm_idx);

    // VMOP_BOOT — boot vmboot_simple_test as guest
    // The guest queries version and stops with version as exit code.
    // After the guest stops, the stop handler sets SGP0 back to our
    // context and stores the exit code in our r0.
    let guest_pc = super::guest_entries::vmboot_simple_test as *const () as u32;

    // Use our existing ALT_STACK for guest stack
    let guest_sp =
        core::ptr::addr_of!(ALT_STACK) as u32 + core::mem::size_of::<AlignedStack>() as u32;

    let boot_result: u32;
    unsafe {
        core::arch::asm!(
            "trap0(#28)",
            inout("r0") 0u32 => boot_result,  // VMOP_BOOT
            in("r1") guest_pc,                 // pc
            in("r2") guest_sp,                 // sp
            in("r3") 0u32,                     // arg1
            in("r4") 3u32,                     // prio
            in("r5") vm_idx,                   // vm
        );
    }

    // Guest should have stopped with VM_VERSION as exit code
    assert_eq!(
        boot_result, VM_VERSION,
        "VMOP_BOOT guest exit code mismatch"
    );

    // VM CPU count should be back to 0 (decremented by stop handler)
    let cpus: u32;
    unsafe {
        core::arch::asm!(
            "trap0(#28)",
            inout("r0") 1u32 => cpus,    // VMOP_STATUS
            in("r1") 1u32,               // STATUS_CPUS
            in("r2") vm_idx,
            out("r3") _, out("r4") _, out("r5") _,
        );
    }
    assert_eq!(cpus, 0);

    // Free the test VM
    let free_r: u32;
    unsafe {
        core::arch::asm!(
            "trap0(#28)",
            inout("r0") 2u32 => free_r,  // VMOP_FREE
            in("r1") vm_idx,
            out("r2") _, out("r3") _, out("r4") _, out("r5") _,
        );
    }
    assert_eq!(free_r, 0);

    debug::write0(b"  [test] VMOP_BOOT end-to-end OK\n\0");
}
