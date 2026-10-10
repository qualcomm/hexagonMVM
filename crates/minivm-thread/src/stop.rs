/*
 * Copyright (c) Qualcomm Technologies, Inc. and/or its subsidiaries.
 * SPDX-License-Identifier: BSD-3-Clause-Clear
 */

//! Thread termination.
//!
//! Thread stop cancels timers, returns the context to the free pool,
//! decrements the VM CPU count, and signals the parent VM if needed.

/// Actions the caller must take after thread stop.
#[derive(Debug, PartialEq, Eq)]
pub enum StopAction {
    /// Signal the parent VM with a child interrupt.
    SignalParent,
    /// No parent to signal, and other CPUs remain.
    NoSignal,
    /// No parent and no remaining CPUs; deallocate the VM.
    DeallocateVm,
}

/// Determine what stop action to take.
///
/// `status`: exit status passed to thread_stop
/// `remaining_cpus`: number of CPUs remaining in the VM after decrement
/// `has_parent`: whether the VM has a valid parent
pub fn stop_action(status: i32, remaining_cpus: u32, has_parent: bool) -> StopAction {
    if status != 0 || remaining_cpus == 0 {
        if has_parent {
            StopAction::SignalParent
        } else if remaining_cpus == 0 {
            StopAction::DeallocateVm
        } else {
            StopAction::NoSignal
        }
    } else {
        StopAction::NoSignal
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_stop_normal_with_siblings() {
        // Normal exit (status=0) with other CPUs remaining
        assert_eq!(stop_action(0, 3, true), StopAction::NoSignal);
    }

    #[test]
    fn test_stop_last_cpu_with_parent() {
        // Last CPU exiting → signal parent
        assert_eq!(stop_action(0, 0, true), StopAction::SignalParent);
    }

    #[test]
    fn test_stop_error_with_parent() {
        // Error exit → signal parent
        assert_eq!(stop_action(-1, 2, true), StopAction::SignalParent);
    }

    #[test]
    fn test_stop_last_cpu_no_parent() {
        // Last CPU, no parent → deallocate
        assert_eq!(stop_action(0, 0, false), StopAction::DeallocateVm);
    }

    #[test]
    fn test_stop_error_no_parent_with_cpus() {
        // Error, no parent, but other CPUs still alive
        assert_eq!(stop_action(-1, 2, false), StopAction::NoSignal);
    }
}
