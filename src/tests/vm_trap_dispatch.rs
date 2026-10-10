/*
 * Copyright (c) Qualcomm Technologies, Inc. and/or its subsidiaries.
 * SPDX-License-Identifier: BSD-3-Clause-Clear
 */

use crate::debug;
use minivm_types::trap::VmTrap;
use minivm_vm::{vmfuncs, vmtrap};

pub fn run() {
    debug::write0(b"  [test] VM trap dispatch\n\0");
    let action = vmtrap::dispatch(0);
    assert_eq!(action, vmtrap::TrapAction::Handle(VmTrap::Version));
    assert!(vmfuncs::vmtrap_version() > 0);
    let bad = vmtrap::dispatch(31);
    assert_eq!(bad, vmtrap::TrapAction::Bad);

    // Test all valid VM trap numbers dispatch correctly
    let valid_traps: &[(u8, VmTrap)] = &[
        (0, VmTrap::Version),
        (1, VmTrap::Return),
        (2, VmTrap::SetVec),
        (3, VmTrap::SetIe),
        (4, VmTrap::GetIe),
        (5, VmTrap::IntOp),
        (10, VmTrap::ClrMap),
        (11, VmTrap::NewMap),
        (13, VmTrap::CacheCtl),
        (14, VmTrap::GetPcycles),
        (15, VmTrap::SetPcycles),
        (16, VmTrap::Wait),
        (17, VmTrap::Yield),
        (18, VmTrap::Start),
        (19, VmTrap::Stop),
        (20, VmTrap::VmPid),
        (21, VmTrap::SetRegs),
        (22, VmTrap::GetRegs),
        (24, VmTrap::TimerOp),
        (25, VmTrap::PmuCtrl),
        (26, VmTrap::Info),
    ];
    for &(num, expected) in valid_traps {
        assert_eq!(vmtrap::dispatch(num), vmtrap::TrapAction::Handle(expected));
    }

    // Test invalid trap numbers
    for num in [6u8, 7, 8, 9, 12, 23, 27, 28, 29, 30, 31] {
        assert_eq!(vmtrap::dispatch(num), vmtrap::TrapAction::Bad);
    }
    debug::write0(b"  [test] VM trap dispatch OK\n\0");
}
