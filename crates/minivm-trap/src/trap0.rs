/*
 * Copyright (c) Qualcomm Technologies, Inc. and/or its subsidiaries.
 * SPDX-License-Identifier: BSD-3-Clause-Clear
 */

//! trap0 handler dispatch (semihosting / kernel syscalls).
//!
//! trap0 is used for kernel-level operations (not VM traps).
//! The cause field in SSR selects the operation.

/// Kernel trap operations (trap0 dispatch table).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum KernelTrap {
    /// Semihosting (trap0 #0).
    Semihosting = 0,
    /// Get thread ID (trap0 #1).
    ThreadId = 1,
    /// Futex wait (trap0 #2).
    FutexWait = 2,
    /// Futex resume (trap0 #3).
    FutexResume = 3,
    /// Create a new thread (trap0 #4).
    ThreadCreate = 4,
    /// Stop current thread (trap0 #5).
    ThreadStop = 5,
    /// Get CPU time (trap0 #6).
    CputimeGet = 6,
    /// Get thread ID (alias, trap0 #7).
    ThreadId2 = 7,
    /// Register fast interrupt handler (trap0 #8).
    RegisterFastint = 8,
    /// Set thread priority (trap0 #9).
    PrioSet = 9,
    /// Get thread priority (trap0 #10).
    PrioGet = 10,
    /// Wait for popup interrupt (trap0 #11).
    PopupWait = 11,
    /// Yield to same-priority threads (trap0 #12).
    SchedYield = 12,
    /// Configure interrupt pool (trap0 #14).
    IntpoolConfigure = 14,
    /// Wait for interrupt pool event (trap0 #15).
    IntpoolWait = 15,
    /// Get pcycles (trap0 #16).
    PcyclesGet = 16,
    /// Get thread ID (alias, trap0 #17).
    ThreadId3 = 17,
    /// Set TID register (trap0 #18).
    TidSet = 18,
    /// Get TID register (trap0 #19).
    TidGet = 19,
    /// Futex lock PI (trap0 #20).
    FutexLockPi = 20,
    /// Futex unlock PI (trap0 #21).
    FutexUnlockPi = 21,
    /// Timer operation (trap0 #22).
    Timer = 22,
    /// Fatal crash (trap0 #23).
    FatalCrash = 23,
    /// TLB operation (trap0 #24).
    TlbOp = 24,
    /// Query thread state (trap0 #25).
    ThreadState = 25,
    /// Query system info (trap0 #26).
    TrapInfo = 26,
    /// Get wait cycles (trap0 #27).
    WaitcyclesGet = 27,
    /// VM operation (trap0 #28).
    VmOp = 28,
    /// PMU control (trap0 #29).
    PmuCtrl = 29,
    /// VM configuration (trap0 #30).
    TrapConfig = 30,
    /// Hardware configuration (trap0 #31).
    HwConfig = 31,
}

/// Maximum trap0 number (5-bit field).
pub const KERNEL_TRAP_MAX: u8 = 32;

impl KernelTrap {
    /// Convert a raw trap number to a KernelTrap.
    pub const fn from_raw(val: u8) -> Option<Self> {
        match val {
            0 => Some(Self::Semihosting),
            1 => Some(Self::ThreadId),
            2 => Some(Self::FutexWait),
            3 => Some(Self::FutexResume),
            4 => Some(Self::ThreadCreate),
            5 => Some(Self::ThreadStop),
            6 => Some(Self::CputimeGet),
            7 => Some(Self::ThreadId2),
            8 => Some(Self::RegisterFastint),
            9 => Some(Self::PrioSet),
            10 => Some(Self::PrioGet),
            11 => Some(Self::PopupWait),
            12 => Some(Self::SchedYield),
            // 13: sample_start (conditional on DO_PROFILE)
            14 => Some(Self::IntpoolConfigure),
            15 => Some(Self::IntpoolWait),
            16 => Some(Self::PcyclesGet),
            17 => Some(Self::ThreadId3),
            18 => Some(Self::TidSet),
            19 => Some(Self::TidGet),
            20 => Some(Self::FutexLockPi),
            21 => Some(Self::FutexUnlockPi),
            22 => Some(Self::Timer),
            23 => Some(Self::FatalCrash),
            24 => Some(Self::TlbOp),
            25 => Some(Self::ThreadState),
            26 => Some(Self::TrapInfo),
            27 => Some(Self::WaitcyclesGet),
            28 => Some(Self::VmOp),
            29 => Some(Self::PmuCtrl),
            30 => Some(Self::TrapConfig),
            31 => Some(Self::HwConfig),
            _ => None,
        }
    }
}

/// Extract the full 8-bit cause from SSR and validate as a trap number.
///
/// Returns `Some(trap_num)` if the cause is 0-31, `None` if > 31.
/// The full 8-bit cause is extracted and range-checked against 0x1F.
pub const fn extract_trap_num(ssr: u32) -> Option<u8> {
    let cause = (ssr & 0xFF) as u8;
    if cause > 31 {
        None
    } else {
        Some(cause)
    }
}

/// Calculate CPU time from cycle counters.
///
/// `total_cycles`: accumulated cycles from previous scheduling quanta.
/// `current_pcycles`: current pcycle counter value.
/// `oncpu_start`: pcycle value when this thread last started running.
pub const fn calculate_cputime(total_cycles: u64, current_pcycles: u64, oncpu_start: u64) -> u64 {
    total_cycles.wrapping_add(current_pcycles.wrapping_sub(oncpu_start))
}

/// Calculate wait cycles for a thread.
///
/// `accumulated_wait`: cycles accumulated in previous wait periods.
/// `current_pcycles`: current pcycle counter value.
/// `wait_start`: pcycle value when current wait period started.
/// `is_running`: whether the thread is currently on a HW thread.
pub const fn calculate_wait_cycles(
    accumulated_wait: u64,
    current_pcycles: u64,
    wait_start: u64,
    is_running: bool,
) -> u64 {
    if is_running {
        accumulated_wait.wrapping_add(current_pcycles.wrapping_sub(wait_start))
    } else {
        accumulated_wait
    }
}

/// Validate a TID value (must fit in u8).
pub const fn validate_tid(tid: u32) -> bool {
    tid <= 0xFF
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_kernel_trap_from_raw() {
        assert_eq!(KernelTrap::from_raw(0), Some(KernelTrap::Semihosting));
        assert_eq!(KernelTrap::from_raw(4), Some(KernelTrap::ThreadCreate));
        assert_eq!(KernelTrap::from_raw(5), Some(KernelTrap::ThreadStop));
        assert_eq!(KernelTrap::from_raw(12), Some(KernelTrap::SchedYield));
        assert_eq!(KernelTrap::from_raw(22), Some(KernelTrap::Timer));
        assert_eq!(KernelTrap::from_raw(28), Some(KernelTrap::VmOp));
        assert_eq!(KernelTrap::from_raw(31), Some(KernelTrap::HwConfig));
    }

    #[test]
    fn test_kernel_trap_invalid() {
        assert_eq!(KernelTrap::from_raw(13), None); // sample_start (conditional)
        assert_eq!(KernelTrap::from_raw(32), None);
        assert_eq!(KernelTrap::from_raw(255), None);
    }

    #[test]
    fn test_extract_trap_num() {
        assert_eq!(extract_trap_num(0x00), Some(0));
        assert_eq!(extract_trap_num(0x1C), Some(28)); // VmOp
        assert_eq!(extract_trap_num(0x1F), Some(31)); // max valid
        assert_eq!(extract_trap_num(0x20), None); // > 31 rejected
        assert_eq!(extract_trap_num(0xFF), None); // > 31 rejected
        assert_eq!(extract_trap_num(0x100), Some(0)); // bits above 7 ignored, cause=0
    }

    #[test]
    fn test_calculate_cputime() {
        assert_eq!(calculate_cputime(1000, 5000, 3000), 3000);
        assert_eq!(calculate_cputime(0, 100, 0), 100);
    }

    #[test]
    fn test_calculate_wait_cycles() {
        assert_eq!(calculate_wait_cycles(100, 500, 300, true), 300);
        assert_eq!(calculate_wait_cycles(100, 500, 300, false), 100);
    }

    #[test]
    fn test_validate_tid() {
        assert!(validate_tid(0));
        assert!(validate_tid(255));
        assert!(!validate_tid(256));
    }
}
