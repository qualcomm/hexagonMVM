/*
 * Copyright (c) Qualcomm Technologies, Inc. and/or its subsidiaries.
 * SPDX-License-Identifier: BSD-3-Clause-Clear
 */

//! Yield to same-priority threads.
//!
//! A yield operation checks if there's another thread ready at the same
//! priority. If so, the current thread is moved to the back of its
//! priority ring and `dosched()` picks the next one. If no same-priority
//! thread is ready, yield is a no-op.

use crate::context::ThreadContext;
use crate::dosched::{dosched, SchedAction};
use crate::lowprio::LowPrio;
use crate::readylist::ReadyList;
use crate::runlist::RunList;

/// Result of a yield operation.
#[derive(Debug, PartialEq, Eq)]
pub enum YieldResult {
    /// Yielded to another thread (caller should context switch).
    Switched(SchedAction),
    /// No same-priority thread ready; continue running.
    NoOp,
}

/// Attempt to yield to another thread at the same priority.
///
/// Returns `NoOp` if no same-priority thread is ready.
pub fn sched_yield(
    ready: &mut ReadyList,
    runlist: &mut RunList,
    lowprio: &mut LowPrio,
    threads: &mut [ThreadContext],
    me: u32,
) -> YieldResult {
    let prio = threads[me as usize].prio as u32;
    if !ready.prio_valid(prio) {
        return YieldResult::NoOp;
    }
    let hthread = threads[me as usize].hthread as u32;
    runlist.remove(threads, me);
    ready.append(threads, me);
    let action = dosched(ready, runlist, lowprio, threads, me, hthread);
    YieldResult::Switched(action)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dosched::SchedAction;
    use minivm_types::vm::ThreadStatus;

    fn make_threads(n: usize) -> alloc::vec::Vec<ThreadContext> {
        let mut v = alloc::vec::Vec::new();
        for _ in 0..n {
            v.push(ThreadContext::zeroed());
        }
        v
    }

    #[test]
    fn test_yield_no_same_prio() {
        let mut ready = ReadyList::new();
        let mut runlist = RunList::new(4);
        let mut lowprio = LowPrio::new();
        let mut threads = make_threads(8);

        threads[1].hthread = 0;
        threads[1].prio = 10;
        runlist.push(&mut threads, 1);

        // No other thread at prio 10
        let result = sched_yield(&mut ready, &mut runlist, &mut lowprio, &mut threads, 1);
        assert_eq!(result, YieldResult::NoOp);
        // Thread 1 should still be running
        assert_eq!(threads[1].status, ThreadStatus::Running as u8);
    }

    #[test]
    fn test_yield_to_same_prio() {
        let mut ready = ReadyList::new();
        let mut runlist = RunList::new(4);
        let mut lowprio = LowPrio::new();
        let mut threads = make_threads(8);

        threads[1].hthread = 0;
        threads[1].prio = 10;
        runlist.push(&mut threads, 1);

        threads[2].prio = 10;
        ready.append(&mut threads, 2);

        let result = sched_yield(&mut ready, &mut runlist, &mut lowprio, &mut threads, 1);
        // Should have switched to thread 2
        match result {
            YieldResult::Switched(SchedAction::Switch(idx)) => assert_eq!(idx, 2),
            _ => panic!("Expected switch to thread 2"),
        }
    }

    #[test]
    fn test_yield_round_robin() {
        let mut ready = ReadyList::new();
        let mut runlist = RunList::new(4);
        let mut lowprio = LowPrio::new();
        let mut threads = make_threads(8);

        // Three threads at same priority
        threads[1].hthread = 0;
        threads[1].prio = 5;
        runlist.push(&mut threads, 1);

        threads[2].prio = 5;
        threads[3].prio = 5;
        ready.append(&mut threads, 2);
        ready.append(&mut threads, 3);

        // Yield from thread 1 -> should get thread 2
        let result = sched_yield(&mut ready, &mut runlist, &mut lowprio, &mut threads, 1);
        match result {
            YieldResult::Switched(SchedAction::Switch(idx)) => assert_eq!(idx, 2),
            _ => panic!("Expected switch to thread 2"),
        }
    }
}
