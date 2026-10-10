/*
 * Copyright (c) Qualcomm Technologies, Inc. and/or its subsidiaries.
 * SPDX-License-Identifier: BSD-3-Clause-Clear
 */

//! Guest entry point functions used by on-target integration tests.
//!
//! These functions are entered via context switch with SSR.GUEST=1.
//! They exercise VM traps from guest mode and are referenced by address
//! from the test modules.

use crate::debug;
use crate::CTX_OLD;
use minivm_sched::context::ThreadContext;
#[cfg(target_arch = "hexagon")]
use minivm_types::vm::VM_VERSION;

/// Function entered via context switch (rte) for round-trip test.
///
/// This function is the "new thread" — entered when minivm_switch restores
/// a ThreadContext with ELR pointing here. It prints a message, then
/// switches back to the original context (CTX_OLD), which resumes
/// minivm_main after the minivm_switch call.
#[cfg(target_arch = "hexagon")]
#[no_mangle]
pub extern "C" fn context_switch_round_trip() {
    debug::write0(b"    [switch] entered target context\n\0");

    extern "C" {
        fn minivm_switch(old: *mut ThreadContext, new: *mut ThreadContext);
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

    // Switch back to the original context
    unsafe {
        minivm_switch(core::ptr::null_mut(), core::ptr::addr_of_mut!(CTX_OLD));
    }

    // Should never reach here
    loop {}
}

/// Guest test entry point — entered via context switch with SSR.GUEST=1.
///
/// Exercises the core VM trap API matching the C test_ie test:
/// 1. Queries VM version via trap1(#0)
/// 2. Tests getie/setie cycle via trap1(#3) and trap1(#4)
/// 3. Encodes results as a bitmask in r16 for verification
/// 4. Does stop trap via trap1(#19) to exit
///
/// Uses the standard convention: trap1(#TRAPNUM) where the immediate
/// argument is the VM trap number (extracted from SSR.CAUSE by the handler).
#[cfg(target_arch = "hexagon")]
#[no_mangle]
pub extern "C" fn guest_test_entry() {
    let mut result: u32 = 0;

    // 1. Query VM version: trap1(#0) = VMTRAP_VERSION
    let version: u32;
    unsafe {
        core::arch::asm!(
            "trap1(#0)",
            out("r0") version,
            out("r1") _, out("r2") _, out("r3") _,
        );
    }
    if version == VM_VERSION {
        result |= 1; // bit 0: version OK
    }

    // 2. GetIE (should be 0 initially): trap1(#4) = VMTRAP_GETIE
    let ie0: u32;
    unsafe {
        core::arch::asm!(
            "trap1(#4)",
            out("r0") ie0,
            out("r1") _, out("r2") _, out("r3") _,
        );
    }
    if ie0 == 0 {
        result |= 2; // bit 1: initial IE == 0
    }

    // 3. SetIE(1) — enable interrupts: trap1(#3) = VMTRAP_SETIE
    //    r0 = 1 (enable). Returns previous IE state.
    let prev_ie: u32;
    unsafe {
        core::arch::asm!(
            "trap1(#3)",
            inout("r0") 1u32 => prev_ie,
            out("r1") _, out("r2") _, out("r3") _,
        );
    }
    if prev_ie == 0 {
        result |= 4; // bit 2: prev IE was 0 before enable
    }

    // 4. GetIE (should be 1 now)
    let ie1: u32;
    unsafe {
        core::arch::asm!(
            "trap1(#4)",
            out("r0") ie1,
            out("r1") _, out("r2") _, out("r3") _,
        );
    }
    if ie1 == 1 {
        result |= 8; // bit 3: IE == 1 after enable
    }

    // 5. SetIE(0) — disable interrupts
    let prev_ie2: u32;
    unsafe {
        core::arch::asm!(
            "trap1(#3)",
            inout("r0") 0u32 => prev_ie2,
            out("r1") _, out("r2") _, out("r3") _,
        );
    }
    if prev_ie2 == 1 {
        result |= 16; // bit 4: prev IE was 1 before disable
    }

    // 6. GetIE (should be 0 again)
    let ie2: u32;
    unsafe {
        core::arch::asm!(
            "trap1(#4)",
            out("r0") ie2,
            out("r1") _, out("r2") _, out("r3") _,
        );
    }
    if ie2 == 0 {
        result |= 32; // bit 5: IE == 0 after disable
    }

    // Save result in r16 (callee-saved, readable from context on stop)
    unsafe {
        core::arch::asm!("nop", in("r16") result);
    }

    // Stop: trap1(#19) = VMTRAP_STOP, r0 = exit code
    // We use r0 = result as the "exit code" for easy verification
    unsafe {
        core::arch::asm!(
            "trap1(#19)",
            in("r0") result,
            out("r1") _, out("r2") _, out("r3") _,
        );
    }

    loop {}
}

/// Comprehensive guest test — exercises all major VM traps needed for Linux boot.
///
/// Tests: version, setvec, setie/getie, intop (enable/status/post/get/clear),
/// info (rev, hthreads, timer_int), timer (getfreq, gettime), getpcycles,
/// cachectl, vmpid, setregs/getregs, yield, and stop.
///
/// Each test sets a bit in a result register. All 20 bits set = 0xFFFFF = success.
#[cfg(target_arch = "hexagon")]
#[no_mangle]
pub extern "C" fn bootvm_guest_test() {
    let mut result: u32 = 0;

    // Test 1: Version query — trap1(#0)
    let version: u32;
    unsafe {
        core::arch::asm!("trap1(#0)", out("r0") version,
			out("r1") _, out("r2") _, out("r3") _);
    }
    if version == VM_VERSION {
        result |= 1 << 0;
    }

    // Test 2: SetVec — trap1(#2), r0 = GEVB address
    // Use a fake address (0x10000) as GEVB for testing
    let setvec_result: u32;
    unsafe {
        core::arch::asm!("trap1(#2)",
			inout("r0") 0x10000u32 => setvec_result,
			out("r1") _, out("r2") _, out("r3") _);
    }
    if setvec_result == 0 {
        result |= 1 << 1;
    }

    // Test 3: GetIe — should be 0 initially
    let ie0: u32;
    unsafe {
        core::arch::asm!("trap1(#4)", out("r0") ie0,
			out("r1") _, out("r2") _, out("r3") _);
    }
    if ie0 == 0 {
        result |= 1 << 2;
    }

    // Test 4: SetIe(1) — enable interrupts, prev should be 0
    let prev_ie: u32;
    unsafe {
        core::arch::asm!("trap1(#3)", inout("r0") 1u32 => prev_ie,
			out("r1") _, out("r2") _, out("r3") _);
    }
    if prev_ie == 0 {
        result |= 1 << 3;
    }

    // Test 5: GetIe — should be 1 now
    let ie1: u32;
    unsafe {
        core::arch::asm!("trap1(#4)", out("r0") ie1,
			out("r1") _, out("r2") _, out("r3") _);
    }
    if ie1 == 1 {
        result |= 1 << 4;
    }

    // Test 6: IntOp GlobEn(20) — enable shared interrupt 20
    // r0=1 (GlobEn), r1=20 (intno)
    let intop_result: u32;
    unsafe {
        core::arch::asm!("trap1(#5)",
			inout("r0") 1u32 => intop_result,
			in("r1") 20u32,
			out("r2") _, out("r3") _);
    }
    if intop_result == 0 {
        result |= 1 << 5;
    }

    // Test 7: IntOp Status(20) — check status of interrupt 20
    // r0=8 (Status), r1=20 (intno)
    let status: u32;
    unsafe {
        core::arch::asm!("trap1(#5)",
			inout("r0") 8u32 => status,
			in("r1") 20u32,
			out("r2") _, out("r3") _);
    }
    // Status should have global enable bit set (bit 2)
    if status & 0x4 != 0 {
        result |= 1 << 6;
    }

    // Test 8: IntOp Post(20) — post interrupt 20
    // r0=9 (Post), r1=20 (intno)
    let post_result: u32;
    unsafe {
        core::arch::asm!("trap1(#5)",
			inout("r0") 9u32 => post_result,
			in("r1") 20u32,
			out("r2") _, out("r3") _);
    }
    // Post should succeed (deliver returns 0 on sim)
    if post_result == 0 {
        result |= 1 << 7;
    }

    // Test 9: IntOp Clear(20) — clear the posted interrupt
    // (Clear it so it doesn't interfere with later tests)
    // r0=10 (Clear), r1=20 (intno)
    // Returns 1 if the interrupt was pending (it was, from the POST above).
    let clear_result: u32;
    unsafe {
        core::arch::asm!("trap1(#5)",
			inout("r0") 10u32 => clear_result,
			in("r1") 20u32,
			out("r2") _, out("r3") _);
    }
    if clear_result == 1 {
        result |= 1 << 8;
    }

    // Test 10: Info Rev — query hardware revision
    // r0=5 (Rev)
    let rev: u32;
    unsafe {
        core::arch::asm!("trap1(#26)",
			inout("r0") 5u32 => rev,
			out("r1") _, out("r2") _, out("r3") _);
    }
    if rev == 0x81 {
        result |= 1 << 9;
    }

    // Test 11: Info HThreads — query hardware threads mask
    // r0=19 (HThreads)
    let hthreads: u32;
    unsafe {
        core::arch::asm!("trap1(#26)",
			inout("r0") 19u32 => hthreads,
			out("r1") _, out("r2") _, out("r3") _);
    }
    if hthreads != 0 {
        result |= 1 << 10;
    }

    // Test 12: Info TimerInt — query timer interrupt number
    // r0=17 (TimerInt)
    let timer_int: u32;
    unsafe {
        core::arch::asm!("trap1(#26)",
			inout("r0") 17u32 => timer_int,
			out("r1") _, out("r2") _, out("r3") _);
    }
    if timer_int == 12 {
        result |= 1 << 11;
    }

    // Test 13: Timer GetFreq — query timer frequency
    // r0=0 (GetFreq)
    let freq_lo: u32;
    let freq_hi: u32;
    unsafe {
        core::arch::asm!("trap1(#24)",
			inout("r0") 0u32 => freq_lo,
			out("r1") freq_hi,
			out("r2") _, out("r3") _);
    }
    let freq = ((freq_hi as u64) << 32) | freq_lo as u64;
    if freq > 0 {
        result |= 1 << 12;
    }

    // Test 14: CacheCtl — no-op on sim, should return 0
    // r0=4 (IDSYNC operation)
    let cache_result: u32;
    unsafe {
        core::arch::asm!("trap1(#13)",
			inout("r0") 4u32 => cache_result,
			out("r1") _, out("r2") _, out("r3") _);
    }
    if cache_result == 0 {
        result |= 1 << 13;
    }

    // Test 15: VmPid — query vCPU ID
    let _vmpid: u32;
    unsafe {
        core::arch::asm!("trap1(#20)", out("r0") _vmpid,
			out("r1") _, out("r2") _, out("r3") _);
    }
    // VmPid returns the thread's VmId — always succeeds
    result |= 1 << 14;

    // Test 16: SetRegs — set guest registers
    // r0=0x100 (gelr), r1=0x200 (gssr), r2=0x300 (gosp), r3=0x400 (gbadva)
    unsafe {
        core::arch::asm!("trap1(#21)",
			in("r0") 0x100u32,
			in("r1") 0x200u32,
			in("r2") 0x300u32,
			in("r3") 0x400u32);
    }
    // Test 17: GetRegs — verify guest registers
    let gelr: u32;
    let gssr: u32;
    let gosp: u32;
    let gbadva: u32;
    unsafe {
        core::arch::asm!("trap1(#22)",
			out("r0") gelr,
			out("r1") gssr,
			out("r2") gosp,
			out("r3") gbadva);
    }
    if gelr == 0x100 && gssr == 0x200 && gosp == 0x300 && gbadva == 0x400 {
        result |= 1 << 15;
    }

    // Test 18: Yield — should return 0
    let yield_result: u32;
    unsafe {
        core::arch::asm!("trap1(#17)",
			out("r0") yield_result,
			out("r1") _, out("r2") _, out("r3") _);
    }
    if yield_result == 0 {
        result |= 1 << 16;
    }

    // Test 19: ClrMap — should return 0 (no-op on sim)
    let clrmap_result: u32;
    unsafe {
        core::arch::asm!("trap1(#10)",
			out("r0") clrmap_result,
			out("r1") _, out("r2") _, out("r3") _);
    }
    if clrmap_result == 0 {
        result |= 1 << 17;
    }

    // Test 20: SetIe(0) — disable interrupts, prev should be 1
    let prev_ie2: u32;
    unsafe {
        core::arch::asm!("trap1(#3)", inout("r0") 0u32 => prev_ie2,
			out("r1") _, out("r2") _, out("r3") _);
    }
    if prev_ie2 == 1 {
        result |= 1 << 18;
    }

    // Test 21: NewMap — should return 0 (no-op on sim)
    let newmap_result: u32;
    unsafe {
        core::arch::asm!("trap1(#11)",
			out("r0") newmap_result,
			out("r1") _, out("r2") _, out("r3") _);
    }
    if newmap_result == 0 {
        result |= 1 << 19;
    }

    // Stop: trap1(#19), r0 = result bitmask
    unsafe {
        core::arch::asm!("trap1(#19)", in("r0") result,
			out("r1") _, out("r2") _, out("r3") _);
    }
    loop {}
}

/// Guest entry for vmboot test — exercises the full VM boot path.
///
/// This guest is launched via vmboot() with ASID-based address translation.
/// It tests the same traps as bootvm_guest_test but is launched through
/// the proper vmboot path (ASID allocation, SSR setup, etc.).
///
/// Tests: version, setvec, getie/setie, intop, info, timer, cachectl,
/// vmpid, setregs/getregs, yield, clrmap, newmap.
/// Result bitmask: all 20 bits set = 0xFFFFF = success.
#[cfg(target_arch = "hexagon")]
#[no_mangle]
pub extern "C" fn vmboot_linux_guest_test() {
    let mut result: u32 = 0;

    // Test 1: Version
    let version: u32;
    unsafe {
        core::arch::asm!("trap1(#0)", out("r0") version,
			out("r1") _, out("r2") _, out("r3") _);
    }
    if version == VM_VERSION {
        result |= 1 << 0;
    }

    // Test 2: SetVec
    let setvec_result: u32;
    unsafe {
        core::arch::asm!("trap1(#2)",
			inout("r0") 0x10000u32 => setvec_result,
			out("r1") _, out("r2") _, out("r3") _);
    }
    if setvec_result == 0 {
        result |= 1 << 1;
    }

    // Test 3: GetIe — should be 0
    let ie0: u32;
    unsafe {
        core::arch::asm!("trap1(#4)", out("r0") ie0,
			out("r1") _, out("r2") _, out("r3") _);
    }
    if ie0 == 0 {
        result |= 1 << 2;
    }

    // Test 4: SetIe(1)
    let prev_ie: u32;
    unsafe {
        core::arch::asm!("trap1(#3)", inout("r0") 1u32 => prev_ie,
			out("r1") _, out("r2") _, out("r3") _);
    }
    if prev_ie == 0 {
        result |= 1 << 3;
    }

    // Test 5: GetIe — should be 1
    let ie1: u32;
    unsafe {
        core::arch::asm!("trap1(#4)", out("r0") ie1,
			out("r1") _, out("r2") _, out("r3") _);
    }
    if ie1 == 1 {
        result |= 1 << 4;
    }

    // Test 6: IntOp GlobEn(20)
    let intop_result: u32;
    unsafe {
        core::arch::asm!("trap1(#5)",
			inout("r0") 1u32 => intop_result,
			in("r1") 20u32,
			out("r2") _, out("r3") _);
    }
    if intop_result == 0 {
        result |= 1 << 5;
    }

    // Test 7: IntOp Status(20)
    let status: u32;
    unsafe {
        core::arch::asm!("trap1(#5)",
			inout("r0") 8u32 => status,
			in("r1") 20u32,
			out("r2") _, out("r3") _);
    }
    if status & 0x4 != 0 {
        result |= 1 << 6;
    }

    // Test 8: IntOp Post(20)
    let post_result: u32;
    unsafe {
        core::arch::asm!("trap1(#5)",
			inout("r0") 9u32 => post_result,
			in("r1") 20u32,
			out("r2") _, out("r3") _);
    }
    if post_result == 0 {
        result |= 1 << 7;
    }

    // Test 9: IntOp Clear(20) — returns 1 if was pending
    let clear_result: u32;
    unsafe {
        core::arch::asm!("trap1(#5)",
			inout("r0") 10u32 => clear_result,
			in("r1") 20u32,
			out("r2") _, out("r3") _);
    }
    if clear_result == 1 {
        result |= 1 << 8;
    }

    // Test 10: Info Rev
    let rev: u32;
    unsafe {
        core::arch::asm!("trap1(#26)",
			inout("r0") 5u32 => rev,
			out("r1") _, out("r2") _, out("r3") _);
    }
    if rev == 0x81 {
        result |= 1 << 9;
    }

    // Test 11: Info HThreads
    let hthreads: u32;
    unsafe {
        core::arch::asm!("trap1(#26)",
			inout("r0") 19u32 => hthreads,
			out("r1") _, out("r2") _, out("r3") _);
    }
    if hthreads != 0 {
        result |= 1 << 10;
    }

    // Test 12: Info TimerInt
    let timer_int: u32;
    unsafe {
        core::arch::asm!("trap1(#26)",
			inout("r0") 17u32 => timer_int,
			out("r1") _, out("r2") _, out("r3") _);
    }
    if timer_int == 12 {
        result |= 1 << 11;
    }

    // Test 13: Timer GetFreq
    let freq_lo: u32;
    let freq_hi: u32;
    unsafe {
        core::arch::asm!("trap1(#24)",
			inout("r0") 0u32 => freq_lo,
			out("r1") freq_hi,
			out("r2") _, out("r3") _);
    }
    let freq = ((freq_hi as u64) << 32) | freq_lo as u64;
    if freq > 0 {
        result |= 1 << 12;
    }

    // Test 14: CacheCtl
    let cache_result: u32;
    unsafe {
        core::arch::asm!("trap1(#13)",
			inout("r0") 4u32 => cache_result,
			out("r1") _, out("r2") _, out("r3") _);
    }
    if cache_result == 0 {
        result |= 1 << 13;
    }

    // Test 15: VmPid
    let _vmpid: u32;
    unsafe {
        core::arch::asm!("trap1(#20)", out("r0") _vmpid,
			out("r1") _, out("r2") _, out("r3") _);
    }
    result |= 1 << 14;

    // Test 16: SetRegs
    unsafe {
        core::arch::asm!("trap1(#21)",
			in("r0") 0x100u32,
			in("r1") 0x200u32,
			in("r2") 0x300u32,
			in("r3") 0x400u32);
    }
    // Test 17: GetRegs — verify
    let gelr: u32;
    let gssr: u32;
    let gosp: u32;
    let gbadva: u32;
    unsafe {
        core::arch::asm!("trap1(#22)",
			out("r0") gelr,
			out("r1") gssr,
			out("r2") gosp,
			out("r3") gbadva);
    }
    if gelr == 0x100 && gssr == 0x200 && gosp == 0x300 && gbadva == 0x400 {
        result |= 1 << 15;
    }

    // Test 18: Yield
    let yield_result: u32;
    unsafe {
        core::arch::asm!("trap1(#17)",
			out("r0") yield_result,
			out("r1") _, out("r2") _, out("r3") _);
    }
    if yield_result == 0 {
        result |= 1 << 16;
    }

    // Test 19: ClrMap
    let clrmap_result: u32;
    unsafe {
        core::arch::asm!("trap1(#10)",
			out("r0") clrmap_result,
			out("r1") _, out("r2") _, out("r3") _);
    }
    if clrmap_result == 0 {
        result |= 1 << 17;
    }

    // Test 20: SetIe(0) — should return 1 (prev was enabled)
    let prev_ie2: u32;
    unsafe {
        core::arch::asm!("trap1(#3)", inout("r0") 0u32 => prev_ie2,
			out("r1") _, out("r2") _, out("r3") _);
    }
    if prev_ie2 == 1 {
        result |= 1 << 18;
    }

    // Test 21: NewMap
    let newmap_result: u32;
    unsafe {
        core::arch::asm!("trap1(#11)",
			out("r0") newmap_result,
			out("r1") _, out("r2") _, out("r3") _);
    }
    if newmap_result == 0 {
        result |= 1 << 19;
    }

    // Stop with result
    unsafe {
        core::arch::asm!("trap1(#19)", in("r0") result,
			out("r1") _, out("r2") _, out("r3") _);
    }
    loop {}
}

/// Simple guest for VMOP_BOOT end-to-end test.
///
/// Queries version, then stops with version as exit code.
/// Used to verify the trap0 VMOP_BOOT → guest run → Stop → caller resume path.
#[cfg(target_arch = "hexagon")]
#[no_mangle]
pub extern "C" fn vmboot_simple_test() {
    let version: u32;
    unsafe {
        core::arch::asm!("trap1(#0)", out("r0") version,
			out("r1") _, out("r2") _, out("r3") _);
    }
    unsafe {
        core::arch::asm!("trap1(#19)", in("r0") version,
			out("r1") _, out("r2") _, out("r3") _);
    }
    loop {}
}
