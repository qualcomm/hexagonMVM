/*
 * Copyright (c) Qualcomm Technologies, Inc. and/or its subsidiaries.
 * SPDX-License-Identifier: BSD-3-Clause-Clear
 */

use crate::debug;
use crate::{ALT_STACK, BOOT_CPUINT, CTX_OLD, GUEST_VERSION_RESULT, KG_PTR};
use minivm_init::globals::{KernelGlobals, MAX_INTERRUPTS};
use minivm_intc::percpu::CpuIntState;
use minivm_sched::context::ThreadContext;
use minivm_types::vm::VmId;
use minivm_vm::vmblock::VmBlock;
use minivm_vm::vmconfig;

pub fn run(kg: &KernelGlobals) {
    debug::write0(b"  [test] boot VM traps (comprehensive)\n\0");

    extern "C" {
        static __global_pointer: u8;
        fn minivm_switch(old: *mut ThreadContext, new: *mut ThreadContext);
    }

    // Set up kernel globals pointer for the handler
    unsafe {
        KG_PTR = kg as *const KernelGlobals;
    }

    // Set up a boot VM for the comprehensive guest test
    let mut test_vm = VmBlock::new(1);
    let _r = vmconfig::set_cpus_ints(&mut test_vm, 4, MAX_INTERRUPTS as u32);
    let _r = vmconfig::set_fences(&mut test_vm, 0, 0xFE000);
    let _r = vmconfig::set_prio_trapmask(&mut test_vm, 0, 0xFFFF_FFFF);

    // Reset per-CPU interrupt state
    unsafe {
        BOOT_CPUINT = CpuIntState::new();
    }

    // Set up guest context
    let mut guest_ctx = ThreadContext::zeroed();
    guest_ctx.elr = super::guest_entries::bootvm_guest_test as *const () as u32;
    guest_ctx.ssr = 1 << 19; // SSR.GUEST = 1

    // Set up stack and GP
    let alt_sp = unsafe { core::ptr::addr_of!(ALT_STACK).cast::<u8>().add(4096) as u32 };
    let gp = unsafe { &__global_pointer as *const u8 as u32 };
    guest_ctx.r2928 = ((alt_sp as u64) << 32) | (gp as u64);

    // Set USR for proper operation
    guest_ctx.usrp30 = (minivm_types::regs::boot_defaults::THREAD_USR as u64) << 32;

    // Set VM identity: vmidx=1, cpuidx=0
    guest_ctx.id = VmId::new(1, 0, 0);

    // Point to the test VM block
    guest_ctx.vmblock = &mut test_vm as *mut VmBlock as u32;

    // Clear result
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

    // Switch to guest
    unsafe {
        minivm_switch(
            core::ptr::addr_of_mut!(CTX_OLD),
            &mut guest_ctx as *mut ThreadContext,
        );
    }

    // Verify: all 20 test bits should be set = 0xFFFFF
    let guest_result = unsafe { GUEST_VERSION_RESULT };
    if guest_result != 0xF_FFFF {
        // Print which bits failed
        debug::write0(b"  [FAIL] boot VM traps: result=0x\0");
        // Print hex digits
        let hex = b"0123456789ABCDEF";
        let mut buf = [0u8; 9];
        for i in 0..8 {
            buf[7 - i] = hex[((guest_result >> (i * 4)) & 0xF) as usize];
        }
        buf[8] = 0;
        debug::write0(&buf);
        debug::write0(b"\n\0");

        // Print per-bit failure info
        let test_names: &[&[u8]] = &[
            b"version\0",
            b"setvec\0",
            b"getie_init\0",
            b"setie_en\0",
            b"getie_1\0",
            b"intop_globen\0",
            b"intop_status\0",
            b"intop_post\0",
            b"intop_clear\0",
            b"info_rev\0",
            b"info_hthreads\0",
            b"info_timer_int\0",
            b"timer_freq\0",
            b"cachectl\0",
            b"vmpid\0",
            b"setgetregs\0",
            b"yield\0",
            b"clrmap\0",
            b"setie_dis\0",
            b"newmap\0",
        ];
        for (i, name) in test_names.iter().enumerate() {
            if guest_result & (1 << i) == 0 {
                debug::write0(b"    FAIL: \0");
                debug::write0(name);
                debug::write0(b"\n\0");
            }
        }
        assert_eq!(guest_result, 0xF_FFFF);
    }

    debug::write0(b"  [test] boot VM traps OK (20 VM traps)\n\0");
}
