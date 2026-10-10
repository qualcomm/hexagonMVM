/*
 * Copyright (c) Qualcomm Technologies, Inc. and/or its subsidiaries.
 * SPDX-License-Identifier: BSD-3-Clause-Clear
 */

use crate::debug;
use minivm_vm::vmblock::VmBlock;
use minivm_vm::vmconfig;

pub fn run() {
    debug::write0(b"  [test] VM block ops\n\0");
    {
        let mut vm = VmBlock::new(1);
        assert_eq!(vm.vmidx, 1);
        assert!(!vm.has_waiting_cpus());

        // Test waiting CPU tracking
        vm.mark_cpu_waiting(3);
        assert!(vm.has_waiting_cpus());
        assert_eq!(vm.waiting_cpus, 1 << 3);

        vm.mark_cpu_waiting(7);
        assert_eq!(vm.waiting_cpus, (1 << 3) | (1 << 7));

        vm.clear_cpu_waiting(3);
        assert_eq!(vm.waiting_cpus, 1 << 7);

        vm.clear_cpu_waiting(7);
        assert!(!vm.has_waiting_cpus());

        // Test version
        assert_eq!(VmBlock::version(), 0x0000_0800);

        // Test vmconfig operations
        let mut vm2 = VmBlock::new(2);

        // SET_CPUS_INTS
        let r = vmconfig::set_cpus_ints(&mut vm2, 8, 256);
        assert_eq!(r, vmconfig::ConfigResult::Ok);
        assert_eq!(vm2.max_cpus, 8);
        assert_eq!(vm2.num_ints, 256);

        // SET_FENCES
        let r = vmconfig::set_fences(&mut vm2, 0x100, 0x7FFF_FFFF);
        assert_eq!(r, vmconfig::ConfigResult::Ok);
        assert_eq!(vm2.fence_lo, 0x100);
        assert_eq!(vm2.fence_hi, 0x7FFF_FFFF);

        // SET_PRIO_TRAPMASK
        let r = vmconfig::set_prio_trapmask(&mut vm2, 5, 0xDEAD_BEEF);
        assert_eq!(r, vmconfig::ConfigResult::Ok);
        assert_eq!(vm2.bestprio, 5);
        assert_eq!(vm2.trapmask, 0xDEAD_BEEF);
    }
    debug::write0(b"  [test] VM block ops OK\n\0");
}
