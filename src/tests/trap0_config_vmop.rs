/*
 * Copyright (c) Qualcomm Technologies, Inc. and/or its subsidiaries.
 * SPDX-License-Identifier: BSD-3-Clause-Clear
 */

use crate::debug;
use crate::{ASID_TABLE_MUT_PTR, KG_PTR};
use minivm_init::globals::KernelGlobals;
use minivm_mem::asid::AsidTable;
use minivm_sched::context::ThreadContext;
use minivm_types::vm::VmId;

pub fn run(kg: &KernelGlobals, asid_table: &mut AsidTable) {
    debug::write0(b"  [test] trap0 CONFIG/VMOP\n\0");

    // Wire up globals for trap0 handlers
    unsafe {
        KG_PTR = kg as *const KernelGlobals;
        ASID_TABLE_MUT_PTR = asid_table as *mut AsidTable;
    }

    // Static context for monitor-mode trap0 context save/restore.
    // Must NOT be on the stack (trap handler resets SP to __stack_end).
    #[repr(C, align(32))]
    struct AlignedCtx(ThreadContext);
    static mut TRAP0_CTX: AlignedCtx = unsafe { core::mem::zeroed() };

    // Set boot VM identity so permission checks in config handler pass
    unsafe {
        TRAP0_CTX.0.id = VmId::new(0, 0, 0); // boot VM = vmidx 0
    }

    // Point SGP0 to our static context
    unsafe {
        core::arch::asm!(
            "sgp0 = {val}",
            val = in(reg) core::ptr::addr_of_mut!(TRAP0_CTX),
        );
    }

    let mut result: u32 = 0;

    // Test 1: CONFIG SET_CPUS_INTS — allocate a new child VM
    let vm_idx: u32;
    unsafe {
        core::arch::asm!(
            "trap0(#30)",
            inout("r0") 0u32 => vm_idx,  // config_type = CONFIG_VMBLOCK_INIT
            in("r1") 0u32,               // vm = 0 (unused for alloc)
            in("r2") 3u32,               // op = SET_CPUS_INTS
            in("r3") 4u32,               // 4 CPUs
            in("r4") 288u32,             // 288 interrupts
            out("r5") _,
        );
    }
    if vm_idx > 0 {
        result |= 1 << 0;
    }

    // Test 2: CONFIG SET_FENCES — set memory fences on child VM
    let fence_result: u32;
    unsafe {
        core::arch::asm!(
            "trap0(#30)",
            inout("r0") 0u32 => fence_result,
            in("r1") vm_idx,
            in("r2") 1u32,               // SET_FENCES
            in("r3") 0u32,               // fence_lo = 0
            in("r4") 0xFE000u32,         // fence_hi = 0xFE000 pages
            out("r5") _,
        );
    }
    if fence_result == vm_idx {
        result |= 1 << 1;
    }

    // Test 3: CONFIG SET_PRIO_TRAPMASK
    let trapmask_result: u32;
    unsafe {
        core::arch::asm!(
            "trap0(#30)",
            inout("r0") 0u32 => trapmask_result,
            in("r1") vm_idx,
            in("r2") 2u32,               // SET_PRIO_TRAPMASK
            in("r3") 0u32,               // bestprio = 0
            in("r4") 0xFFFF_FFFFu32,     // trapmask = all
            out("r5") _,
        );
    }
    if trapmask_result == vm_idx {
        result |= 1 << 2;
    }

    // Test 4: CONFIG SET_PMAP_TYPE (offset translation)
    let pmap_result: u32;
    unsafe {
        core::arch::asm!(
            "trap0(#30)",
            inout("r0") 0u32 => pmap_result,
            in("r1") vm_idx,
            in("r2") 0u32,               // SET_PMAP_TYPE
            in("r3") 0x0F70u32,          // OffsetConfig: size=0, cccc=7, weak=1, xwru=F, pages=0
            in("r4") 1u32,               // type = OFFSET
            out("r5") _,
        );
    }
    if pmap_result == vm_idx {
        result |= 1 << 3;
    }

    // Test 5: VMOP_STATUS — check status (should be 0 = OK)
    let status: u32;
    unsafe {
        core::arch::asm!(
            "trap0(#28)",
            inout("r0") 1u32 => status,  // op = STATUS
            in("r1") 0u32,               // STATUS_STATUS
            in("r2") vm_idx,
            out("r3") _, out("r4") _, out("r5") _,
        );
    }
    if status == 0 {
        result |= 1 << 4;
    }

    // Test 6: VMOP_STATUS — check CPU count (should be 0)
    let cpus: u32;
    unsafe {
        core::arch::asm!(
            "trap0(#28)",
            inout("r0") 1u32 => cpus,    // op = STATUS
            in("r1") 1u32,               // STATUS_CPUS
            in("r2") vm_idx,
            out("r3") _, out("r4") _, out("r5") _,
        );
    }
    if cpus == 0 {
        result |= 1 << 5;
    }

    // Test 7: VMOP_FREE — free the child VM
    let free_result: u32;
    unsafe {
        core::arch::asm!(
            "trap0(#28)",
            inout("r0") 2u32 => free_result,  // op = FREE
            in("r1") vm_idx,
            out("r2") _, out("r3") _, out("r4") _, out("r5") _,
        );
    }
    if free_result == 0 {
        result |= 1 << 6;
    }

    if result != 0x7F {
        debug::write0(b"  [FAIL] CONFIG/VMOP: result=0x\0");
        let hex = b"0123456789ABCDEF";
        let mut buf = [0u8; 9];
        for i in 0..8 {
            buf[7 - i] = hex[((result >> (i * 4)) & 0xF) as usize];
        }
        buf[8] = 0;
        debug::write0(&buf);
        debug::write0(b"\n\0");

        let test_names: &[&[u8]] = &[
            b"config_cpus_ints\0",
            b"config_fences\0",
            b"config_trapmask\0",
            b"config_pmap\0",
            b"vmop_status\0",
            b"vmop_cpus\0",
            b"vmop_free\0",
        ];
        for (i, name) in test_names.iter().enumerate() {
            if result & (1 << i) == 0 {
                debug::write0(b"    FAIL: \0");
                debug::write0(name);
                debug::write0(b"\n\0");
            }
        }
        assert_eq!(result, 0x7F);
    }

    debug::write0(b"  [test] trap0 CONFIG/VMOP OK (7 multi-VM ops)\n\0");
}
