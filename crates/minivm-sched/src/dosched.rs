/*
 * Copyright (c) Qualcomm Technologies, Inc. and/or its subsidiaries.
 * SPDX-License-Identifier: BSD-3-Clause-Clear
 */

//! Core scheduling algorithm.
//!
//! `dosched()` is the heart of the scheduler:
//! 1. Get the best ready thread from the ready list
//! 2. If none available, enter wait mode (sleep the hardware thread)
//! 3. Check if the new thread is the lowest priority across all running threads
//! 4. Update low-priority tracking accordingly
//! 5. Push the new thread onto the run list and perform a context switch

use crate::context::ThreadContext;
use crate::lowprio::LowPrio;
use crate::readylist::{ReadyList, IDX_NONE};
use crate::runlist::RunList;

/// Result of `dosched()` indicating what action the caller should take.
#[derive(Debug, PartialEq, Eq)]
pub enum SchedAction {
    /// Switch to the thread at the given index.
    Switch(u32),
    /// No threads available; the hardware thread should enter wait mode.
    Wait,
}

/// Core scheduling decision.
///
/// Returns the action to take: either switch to a new thread or enter wait mode.
///
/// The caller is responsible for actually performing the context switch or
/// entering wait mode (these require architecture-specific operations).
pub fn dosched(
    ready: &mut ReadyList,
    runlist: &mut RunList,
    lowprio: &mut LowPrio,
    threads: &mut [ThreadContext],
    _me: u32,
    hthread: u32,
) -> SchedAction {
    let new = ready.getbest(threads);
    if new == IDX_NONE {
        // No ready threads — enter wait mode
        lowprio.raise();
        return SchedAction::Wait;
    }

    // Check if the new thread is the worst priority
    let worst_running = runlist.worst_prio();
    let new_prio = threads[new as usize].prio as u32;

    if lowprio.wait_mask == 0 && new_prio > worst_running {
        // New thread is worse than all running — mark as low priority
        if !lowprio.is_low(hthread) {
            lowprio.raise();
            lowprio.mark_low(hthread);
        }
    } else if lowprio.is_low(hthread) {
        // Was low priority but now getting a better thread (or wait_mask != 0)
        lowprio.unmark_low(hthread);
    }

    // Assign hardware thread and push to run list
    threads[new as usize].hthread = hthread as u8;
    runlist.push(threads, new);

    SchedAction::Switch(new)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_threads(n: usize) -> alloc::vec::Vec<ThreadContext> {
        let mut v = alloc::vec::Vec::new();
        for _ in 0..n {
            v.push(ThreadContext::zeroed());
        }
        v
    }

    #[test]
    fn test_dosched_no_ready_threads() {
        let mut ready = ReadyList::new();
        let mut runlist = RunList::new(4);
        let mut lowprio = LowPrio::new();
        let mut threads = make_threads(4);

        let action = dosched(&mut ready, &mut runlist, &mut lowprio, &mut threads, 0, 0);
        assert_eq!(action, SchedAction::Wait);
    }

    #[test]
    fn test_dosched_picks_best() {
        let mut ready = ReadyList::new();
        let mut runlist = RunList::new(4);
        let mut lowprio = LowPrio::new();
        let mut threads = make_threads(8);

        threads[1].prio = 10;
        threads[2].prio = 5;
        ready.append(&mut threads, 1);
        ready.append(&mut threads, 2);

        let action = dosched(&mut ready, &mut runlist, &mut lowprio, &mut threads, 0, 0);
        assert_eq!(action, SchedAction::Switch(2));
        assert_eq!(threads[2].hthread, 0);
    }

    #[test]
    fn test_dosched_lowprio_marking() {
        let mut ready = ReadyList::new();
        let mut runlist = RunList::new(4);
        let mut lowprio = LowPrio::new();
        let mut threads = make_threads(8);

        // Thread 1 running at prio 5 on hthread 1
        threads[1].hthread = 1;
        threads[1].prio = 5;
        runlist.push(&mut threads, 1);

        // Thread 2 ready at prio 100 (worse)
        threads[2].prio = 100;
        ready.append(&mut threads, 2);

        // Schedule on hthread 0
        let action = dosched(&mut ready, &mut runlist, &mut lowprio, &mut threads, 0, 0);
        assert_eq!(action, SchedAction::Switch(2));
        // hthread 0 should be marked as low priority
        assert!(lowprio.is_low(0));
    }
}
