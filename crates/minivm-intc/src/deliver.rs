/*
 * Copyright (c) Qualcomm Technologies, Inc. and/or its subsidiaries.
 * SPDX-License-Identifier: BSD-3-Clause-Clear
 */

//! Interrupt delivery.
//!
//! When an interrupt is posted and deliverable, the delivery logic determines
//! what action to take based on the target thread's status:
//! - VMWAIT: wake the thread, enqueue it on the ready list
//! - RUNNING: send an IPI to preempt it
//! - BLOCKED/INTBLOCKED: cancel the block if interrupts are enabled, re-enqueue
//! - READY: no action needed (will be delivered on next context switch)
//! - DEAD: return error

use minivm_types::vm::ThreadStatus;

/// Action the caller should take after attempting to deliver an interrupt.
#[derive(Debug, PartialEq, Eq)]
pub enum DeliverAction {
    /// Thread was woken from wait; enqueue it on the ready list.
    Enqueue,
    /// Thread is running; send an IPI to its hardware thread.
    SendIpi(u8),
    /// Thread was blocked; cancel the block and enqueue.
    CancelBlockAndEnqueue,
    /// No action needed (thread is ready or will pick up the interrupt).
    None,
    /// Thread is dead; delivery failed.
    Dead,
}

/// Determine the delivery action for an interrupt targeting a thread.
///
/// `status` is the target thread's current status.
/// `hthread` is the hardware thread the target is running on (for IPI).
/// `ie_enabled` is whether the target has guest interrupts enabled.
pub fn deliver_action(status: ThreadStatus, hthread: u8, ie_enabled: bool) -> DeliverAction {
    match status {
        ThreadStatus::VmWait => DeliverAction::Enqueue,
        ThreadStatus::Running => {
            if ie_enabled {
                DeliverAction::SendIpi(hthread)
            } else {
                DeliverAction::None
            }
        }
        ThreadStatus::IntBlocked => {
            if ie_enabled {
                DeliverAction::CancelBlockAndEnqueue
            } else {
                DeliverAction::None
            }
        }
        ThreadStatus::Blocked => {
            if ie_enabled {
                DeliverAction::CancelBlockAndEnqueue
            } else {
                DeliverAction::None
            }
        }
        ThreadStatus::Ready => DeliverAction::None,
        ThreadStatus::Dead => DeliverAction::Dead,
    }
}

/// Check if a thread has deliverable interrupts and determine the action.
///
/// `cpuint_pending` and `cpuint_enabled` are the per-CPU interrupt bitmasks.
/// `ie_enabled` is whether guest interrupts are globally enabled.
///
/// Returns true if there are deliverable interrupts.
pub fn has_deliverable_interrupts(
    cpuint_pending: u16,
    cpuint_enabled: u16,
    ie_enabled: bool,
) -> bool {
    if !ie_enabled {
        return false;
    }
    (cpuint_pending & cpuint_enabled) != 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_deliver_vmwait() {
        assert_eq!(
            deliver_action(ThreadStatus::VmWait, 0, false),
            DeliverAction::Enqueue
        );
    }

    #[test]
    fn test_deliver_running_ie() {
        assert_eq!(
            deliver_action(ThreadStatus::Running, 2, true),
            DeliverAction::SendIpi(2)
        );
    }

    #[test]
    fn test_deliver_running_no_ie() {
        assert_eq!(
            deliver_action(ThreadStatus::Running, 2, false),
            DeliverAction::None
        );
    }

    #[test]
    fn test_deliver_blocked_ie() {
        assert_eq!(
            deliver_action(ThreadStatus::Blocked, 0, true),
            DeliverAction::CancelBlockAndEnqueue
        );
    }

    #[test]
    fn test_deliver_ready() {
        assert_eq!(
            deliver_action(ThreadStatus::Ready, 0, true),
            DeliverAction::None
        );
    }

    #[test]
    fn test_deliver_dead() {
        assert_eq!(
            deliver_action(ThreadStatus::Dead, 0, true),
            DeliverAction::Dead
        );
    }

    #[test]
    fn test_has_deliverable() {
        assert!(has_deliverable_interrupts(0x08, 0x08, true));
        assert!(!has_deliverable_interrupts(0x08, 0x08, false)); // IE disabled
        assert!(!has_deliverable_interrupts(0x08, 0x04, true)); // Not enabled
        assert!(!has_deliverable_interrupts(0x00, 0x08, true)); // Not pending
    }
}
