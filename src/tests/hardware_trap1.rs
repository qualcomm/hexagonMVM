/*
 * Copyright (c) Qualcomm Technologies, Inc. and/or its subsidiaries.
 * SPDX-License-Identifier: BSD-3-Clause-Clear
 */

use crate::debug;
use crate::MONITOR_CTX;
use minivm_types::trap::VmTrap;
use minivm_types::vm::VM_VERSION;

/// Issue a trap1(#0) hypercall with the given argument in r0.
fn do_trap1(arg: u32) -> u32 {
    let result: u32;
    unsafe {
        core::arch::asm!(
            "trap1(#0)",
            inout("r0") arg => result,
            out("r1") _,
            out("r2") _,
            out("r3") _,
        );
    }
    result
}

pub fn run() {
    debug::write0(b"  [test] hardware trap1\n\0");

    // Set up SGP0 to point to MONITOR_CTX for the full-save trap1 handler.
    // The handler uses crswap(r0, sgp0) to get the context pointer.
    unsafe {
        let ctx_ptr = core::ptr::addr_of_mut!(MONITOR_CTX) as u32;
        core::arch::asm!("sgp0 = {val}", val = in(reg) ctx_ptr);
    }

    // VMVERSION: trap 0
    let version = do_trap1(0x000);
    assert_eq!(version, VM_VERSION);

    // VMGETIE: trap 4
    let ie = do_trap1(0x400);
    assert_eq!(ie, 0);

    // VMSETIE: trap 3 (enable=0)
    let prev = do_trap1(0x300);
    assert_eq!(prev, 0); // was disabled

    // VMRETURN: trap 1 → returns trap number 1
    let ret = do_trap1(0x100);
    assert_eq!(ret, VmTrap::Return as u32);

    // VMSETVEC: trap 2 → returns trap number 2
    let vec = do_trap1(0x200);
    assert_eq!(vec, VmTrap::SetVec as u32);

    // VMINTOP: trap 5 → returns trap number 5
    let intop = do_trap1(0x500);
    assert_eq!(intop, VmTrap::IntOp as u32);

    // VMCLRMAP: trap 10 → returns trap number 10
    let clrmap = do_trap1(0xA00);
    assert_eq!(clrmap, VmTrap::ClrMap as u32);

    // VMWAIT: trap 16 → returns trap number 16
    let wait = do_trap1(0x1000);
    assert_eq!(wait, VmTrap::Wait as u32);

    // VMYIELD: trap 17 → returns trap number 17
    let yld = do_trap1(0x1100);
    assert_eq!(yld, VmTrap::Yield as u32);

    // VMSTART: trap 18 → returns trap number 18
    let start = do_trap1(0x1200);
    assert_eq!(start, VmTrap::Start as u32);

    // VMSTOP: trap 19 → returns trap number 19
    let stop = do_trap1(0x1300);
    assert_eq!(stop, VmTrap::Stop as u32);

    // VMINFO: trap 26 → returns trap number 26
    let info = do_trap1(0x1A00);
    assert_eq!(info, VmTrap::Info as u32);

    // Invalid trap 31 → returns 0xFFFFFFFF
    let bad = do_trap1(0x1F00);
    assert_eq!(bad, 0xFFFF_FFFF);

    // Invalid trap 6 → returns 0xFFFFFFFF
    let bad2 = do_trap1(0x600);
    assert_eq!(bad2, 0xFFFF_FFFF);

    debug::write0(b"  [test] hardware trap1 OK (14 traps)\n\0");
}
