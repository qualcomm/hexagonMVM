/*
 * Copyright (c) Qualcomm Technologies, Inc. and/or its subsidiaries.
 * SPDX-License-Identifier: BSD-3-Clause-Clear
 */

//! Reschedule interrupt handler.
//!
//! When a reschedule interrupt fires, the current thread is moved from
//! the run list back to the ready list, and `dosched()` is called to
//! pick the next thread.

use crate::context::ThreadContext;
use crate::dosched::{dosched, SchedAction};
use crate::lowprio::LowPrio;
use crate::readylist::{ReadyList, IDX_NONE};
use crate::runlist::RunList;

/// Handle a reschedule event on the given hardware thread.
///
/// `me` is the index of the currently running thread (or `IDX_NONE` if
/// the hardware thread was in wait mode).
///
/// Returns the scheduling action (Switch or Wait).
pub fn resched(
    ready: &mut ReadyList,
    runlist: &mut RunList,
    lowprio: &mut LowPrio,
    threads: &mut [ThreadContext],
    me: u32,
    hthread: u32,
) -> SchedAction {
    if me != IDX_NONE {
        runlist.remove(threads, me);
        ready.append(threads, me);
    } else {
        // Interrupted wait mode
        lowprio.unmark_wait(hthread);
    }
    dosched(ready, runlist, lowprio, threads, me, hthread)
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
    fn test_resched_current_goes_to_ready() {
        let mut ready = ReadyList::new();
        let mut runlist = RunList::new(4);
        let mut lowprio = LowPrio::new();
        let mut threads = make_threads(8);

        // Thread 1 running on hthread 0 at prio 10
        threads[1].hthread = 0;
        threads[1].prio = 10;
        runlist.push(&mut threads, 1);

        // Thread 2 ready at prio 5 (better)
        threads[2].prio = 5;
        ready.append(&mut threads, 2);

        // Reschedule on hthread 0
        let action = resched(&mut ready, &mut runlist, &mut lowprio, &mut threads, 1, 0);
        assert_eq!(action, SchedAction::Switch(2));

        // Thread 1 should be back in ready list
        assert!(ready.prio_valid(10));
    }

    #[test]
    fn test_resched_from_wait_mode() {
        let mut ready = ReadyList::new();
        let mut runlist = RunList::new(4);
        let mut lowprio = LowPrio::new();
        let mut threads = make_threads(8);

        lowprio.mark_wait(0);

        threads[1].prio = 5;
        ready.append(&mut threads, 1);

        // Reschedule from wait mode (me = IDX_NONE)
        let action = resched(
            &mut ready,
            &mut runlist,
            &mut lowprio,
            &mut threads,
            IDX_NONE,
            0,
        );
        assert_eq!(action, SchedAction::Switch(1));
        assert_eq!(lowprio.wait_mask & 1, 0); // Wait cleared
    }

    #[test]
    fn test_resched_nothing_ready() {
        let mut ready = ReadyList::new();
        let mut runlist = RunList::new(4);
        let mut lowprio = LowPrio::new();
        let mut threads = make_threads(4);

        // Thread 1 running, but no other threads ready
        threads[1].hthread = 0;
        threads[1].prio = 10;
        runlist.push(&mut threads, 1);

        let action = resched(&mut ready, &mut runlist, &mut lowprio, &mut threads, 1, 0);
        // Thread 1 goes to ready, then dosched picks it back
        assert_eq!(action, SchedAction::Switch(1));
    }
}
