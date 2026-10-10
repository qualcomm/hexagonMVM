/*
 * Copyright (c) Qualcomm Technologies, Inc. and/or its subsidiaries.
 * SPDX-License-Identifier: BSD-3-Clause-Clear
 */

//! Deferred VCPU work.
//!
//! The VM work handler checks for pending kill flags and interrupts
//! on a thread before returning to the guest.

use minivm_types::vm::{VMSTATUS_KILL, VMSTATUS_VMWORK};

#[cfg(test)]
use minivm_types::vm::VMSTATUS_IE;

/// Result of checking deferred VM work.
#[derive(Debug, PartialEq, Eq)]
pub enum VmWorkResult {
    /// Thread should be killed with status 0xd1eed1ee.
    Kill,
    /// An interrupt is deliverable (interrupt number returned).
    Interrupt(i32),
    /// An interrupt is pending but IE is disabled (for vmwait).
    PendingNoDeliver(i32),
    /// No work pending.
    None,
}

/// Check if a thread has deferred VM work to process.
///
/// `vmstatus`: the thread's vmstatus byte.
/// `ie_enabled`: whether guest interrupts are enabled.
/// `pending_int`: the result of checking for pending interrupts (-1 = none).
pub fn do_work(vmstatus: u8, ie_enabled: bool, pending_int: i32) -> VmWorkResult {
    // Check kill flag first
    if vmstatus & VMSTATUS_KILL != 0 {
        return VmWorkResult::Kill;
    }

    // Check VMWORK flag
    if vmstatus & VMSTATUS_VMWORK == 0 {
        return VmWorkResult::None;
    }

    // Check for pending interrupts
    if pending_int >= 0 {
        if ie_enabled {
            return VmWorkResult::Interrupt(pending_int);
        } else {
            // IE disabled: return pending int number (used by vmwait)
            return VmWorkResult::PendingNoDeliver(pending_int);
        }
    }

    VmWorkResult::None
}

/// Compute the new vmstatus after clearing the VMWORK flag.
///
/// Only clears if IE is enabled (matching C behavior where VMWORK
/// persists when interrupts are disabled for vmwait handling).
pub fn clear_vmwork(vmstatus: u8, ie_enabled: bool) -> u8 {
    if ie_enabled {
        vmstatus & !VMSTATUS_VMWORK
    } else {
        vmstatus
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_no_work() {
        assert_eq!(do_work(0, false, -1), VmWorkResult::None);
    }

    #[test]
    fn test_kill() {
        assert_eq!(do_work(VMSTATUS_KILL, false, -1), VmWorkResult::Kill);
    }

    #[test]
    fn test_kill_takes_priority() {
        assert_eq!(
            do_work(VMSTATUS_KILL | VMSTATUS_VMWORK, true, 5),
            VmWorkResult::Kill
        );
    }

    #[test]
    fn test_vmwork_with_interrupt() {
        assert_eq!(
            do_work(VMSTATUS_VMWORK, true, 3),
            VmWorkResult::Interrupt(3)
        );
    }

    #[test]
    fn test_vmwork_ie_disabled() {
        // VMWORK set but IE disabled → pending but not delivered (for vmwait)
        assert_eq!(
            do_work(VMSTATUS_VMWORK, false, 3),
            VmWorkResult::PendingNoDeliver(3)
        );
    }

    #[test]
    fn test_vmwork_ie_disabled_no_pending() {
        // VMWORK set, IE disabled, no pending interrupt
        assert_eq!(do_work(VMSTATUS_VMWORK, false, -1), VmWorkResult::None);
    }

    #[test]
    fn test_vmwork_no_pending() {
        // VMWORK set, IE enabled, but no pending interrupt
        assert_eq!(do_work(VMSTATUS_VMWORK, true, -1), VmWorkResult::None);
    }

    #[test]
    fn test_clear_vmwork_ie_enabled() {
        let vs = VMSTATUS_VMWORK | VMSTATUS_IE;
        assert_eq!(clear_vmwork(vs, true) & VMSTATUS_VMWORK, 0);
    }

    #[test]
    fn test_clear_vmwork_ie_disabled() {
        let vs = VMSTATUS_VMWORK;
        // Should NOT clear when IE disabled
        assert_ne!(clear_vmwork(vs, false) & VMSTATUS_VMWORK, 0);
    }
}
