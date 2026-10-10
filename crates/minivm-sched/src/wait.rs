/*
 * Copyright (c) Qualcomm Technologies, Inc. and/or its subsidiaries.
 * SPDX-License-Identifier: BSD-3-Clause-Clear
 */

//! Wait mode (idle hardware thread management).
//!
//! When a hardware thread has no threads to run, it enters wait mode.
//! The wait_mask tracks which hardware threads are in wait mode so that
//! reschedule IPIs can wake them when new work becomes available.

use crate::lowprio::LowPrio;

/// Enter wait mode on the given hardware thread.
///
/// The caller must actually put the hardware thread to sleep after calling
/// this (via architecture-specific wait instruction).
pub fn enter_wait(lowprio: &mut LowPrio, hthread: u32) {
    lowprio.mark_wait(hthread);
}

/// Exit wait mode on the given hardware thread.
pub fn exit_wait(lowprio: &mut LowPrio, hthread: u32) {
    lowprio.unmark_wait(hthread);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_enter_exit_wait() {
        let mut lp = LowPrio::new();
        assert_eq!(lp.wait_mask, 0);

        enter_wait(&mut lp, 2);
        assert_eq!(lp.wait_mask, 1 << 2);

        exit_wait(&mut lp, 2);
        assert_eq!(lp.wait_mask, 0);
    }

    #[test]
    fn test_multiple_waiters() {
        let mut lp = LowPrio::new();

        enter_wait(&mut lp, 0);
        enter_wait(&mut lp, 3);
        assert_eq!(lp.wait_mask, (1 << 0) | (1 << 3));

        exit_wait(&mut lp, 0);
        assert_eq!(lp.wait_mask, 1 << 3);
    }
}
