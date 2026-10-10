/*
 * Copyright (c) Qualcomm Technologies, Inc. and/or its subsidiaries.
 * SPDX-License-Identifier: BSD-3-Clause-Clear
 */

use crate::debug;
use crate::ALT_STACK;
use minivm_init::globals::{KernelGlobals, MAX_INTERRUPTS};
use minivm_mem::asid::AsidTable;
use minivm_vm::vmblock::VmBlock;
use minivm_vm::vmconfig;

pub fn run(asid_table: &mut AsidTable, kg: &KernelGlobals) {
    debug::write0(b"  [test] vmboot (Linux-style boot)\n\0");

    // Configure VM like loadlinux does:
    // SET_CPUS_INTS(4, 320), SET_FENCES, SET_PRIO_TRAPMASK(0, 0xFFFFFFFF)
    // Note: fences must cover guest code addresses. On sim, guest code is at
    // kernel addresses (0xff000000+), so we use very wide fences.
    // In production, Linux loads at 0x80000000 with fences (0, 0xFE000).
    let mut linux_vm = VmBlock::new(2);
    let _r = vmconfig::set_cpus_ints(&mut linux_vm, 4, MAX_INTERRUPTS as u32);
    let _r = vmconfig::set_fences(&mut linux_vm, 0, 0x7FFF_FFFF);
    let _r = vmconfig::set_prio_trapmask(&mut linux_vm, 0, 0xFFFF_FFFF);

    // Boot the guest via vmboot (ASID allocation, SSR setup, context switch)
    let alt_sp = unsafe { core::ptr::addr_of!(ALT_STACK).cast::<u8>().add(4096) as u32 };
    let exit_code = crate::vmboot(
        super::guest_entries::vmboot_linux_guest_test as *const () as u32,
        alt_sp,
        0, // arg1
        3, // prio (LINUX_VM_PRIO)
        &mut linux_vm,
        asid_table,
        kg,
    );

    if exit_code != 0xF_FFFF {
        debug::write0(b"  [FAIL] vmboot: result=0x\0");
        let hex = b"0123456789ABCDEF";
        let mut buf = [0u8; 9];
        for i in 0..8 {
            buf[7 - i] = hex[((exit_code >> (i * 4)) & 0xF) as usize];
        }
        buf[8] = 0;
        debug::write0(&buf);
        debug::write0(b"\n\0");
        assert_eq!(exit_code, 0xF_FFFF);
    }
    assert_eq!(linux_vm.num_cpus, 0); // CPU count decremented after stop

    debug::write0(b"  [test] vmboot OK (20 VM traps via ASID boot path)\n\0");
}
