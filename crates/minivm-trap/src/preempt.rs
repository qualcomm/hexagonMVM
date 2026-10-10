/*
 * Copyright (c) Qualcomm Technologies, Inc. and/or its subsidiaries.
 * SPDX-License-Identifier: BSD-3-Clause-Clear
 */

//! Preemption point support.
//!
//! Preemption points allow the kernel to check for pending reschedule
//! requests at safe points during long-running operations. The
//! preemption handler saves the current context and calls the
//! scheduler if a higher-priority thread is ready.

/// Preemption point marker value.
///
/// When a preemption point is taken, this marker is stored in the
/// context to distinguish preemption returns from normal returns.
pub const PREEMPT_MARKER: u32 = 1;

/// Check if a reschedule is needed at a preemption point.
///
/// `current_prio`: the running thread's priority (lower = higher priority).
/// `best_ready_prio`: the best priority in the ready queue (lower = higher priority).
///
/// Reschedule is needed when a higher-priority thread is ready.
pub const fn needs_reschedule(current_prio: u8, best_ready_prio: u8) -> bool {
    best_ready_prio < current_prio
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_needs_reschedule_higher_priority() {
        // Thread at prio 10, prio 5 thread is ready → reschedule
        assert!(needs_reschedule(10, 5));
    }

    #[test]
    fn test_needs_reschedule_same_priority() {
        // Same priority → no reschedule at preemption point
        assert!(!needs_reschedule(10, 10));
    }

    #[test]
    fn test_needs_reschedule_lower_priority() {
        // Thread at prio 5, only prio 10 is ready → no reschedule
        assert!(!needs_reschedule(5, 10));
    }

    #[test]
    fn test_preempt_marker() {
        assert_eq!(PREEMPT_MARKER, 1);
    }
}
