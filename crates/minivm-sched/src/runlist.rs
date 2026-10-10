/*
 * Copyright (c) Qualcomm Technologies, Inc. and/or its subsidiaries.
 * SPDX-License-Identifier: BSD-3-Clause-Clear
 */

//! Per-hardware-thread run tracking.
//!
//! The run list tracks which thread is currently running on each hardware
//! thread. Each slot stores the index of the running ThreadContext (or
//! `IDX_NONE` if the hardware thread is idle) and the thread's priority.

use crate::context::ThreadContext;
use crate::readylist::IDX_NONE;
use minivm_types::consts::MAX_HTHREADS;
use minivm_types::vm::ThreadStatus;

/// Per-hardware-thread run tracking.
pub struct RunList {
    /// Index of the running thread on each hardware thread.
    slots: [u32; MAX_HTHREADS as usize],
    /// Priority of the running thread (-1 if empty, using i16 like C).
    prios: [i16; MAX_HTHREADS as usize],
    /// Number of active hardware threads.
    pub hthreads: u32,
}

impl RunList {
    /// Create an empty run list.
    pub const fn new(hthreads: u32) -> Self {
        Self {
            slots: [IDX_NONE; MAX_HTHREADS as usize],
            prios: [-1i16; MAX_HTHREADS as usize],
            hthreads,
        }
    }

    /// Push a thread onto a hardware thread's run slot.
    ///
    /// Sets the thread's status to Running.
    pub fn push(&mut self, threads: &mut [ThreadContext], idx: u32) {
        let hthread = threads[idx as usize].hthread as usize;
        let prio = threads[idx as usize].prio;
        threads[idx as usize].status = ThreadStatus::Running as u8;
        self.slots[hthread] = idx;
        self.prios[hthread] = prio as i16;
    }

    /// Remove a thread from its hardware thread's run slot.
    pub fn remove(&mut self, threads: &[ThreadContext], idx: u32) {
        let hthread = threads[idx as usize].hthread as usize;
        self.slots[hthread] = IDX_NONE;
        self.prios[hthread] = -1;
    }

    /// Get the worst (numerically highest) priority across all running threads.
    ///
    /// Returns `MAX_PRIOS` if no threads are running.
    pub fn worst_prio(&self) -> u32 {
        let mut worst: i32 = -1;
        for i in 0..self.hthreads as usize {
            if (self.prios[i] as i32) > worst {
                worst = self.prios[i] as i32;
            }
        }
        if worst == -1 {
            minivm_types::consts::MAX_PRIOS
        } else {
            worst as u32
        }
    }

    /// Get the hardware thread running the worst-priority thread.
    ///
    /// Returns `u32::MAX` if no threads are running.
    pub fn worst_prio_hthread(&self) -> u32 {
        let mut worst: i32 = -1;
        let mut hthread: i32 = -1;
        for i in 0..self.hthreads as usize {
            if (self.prios[i] as i32) > worst {
                worst = self.prios[i] as i32;
                hthread = i as i32;
            }
        }
        if hthread == -1 {
            u32::MAX
        } else {
            hthread as u32
        }
    }

    /// Get the thread index running on a given hardware thread.
    pub fn get(&self, hthread: u32) -> u32 {
        self.slots[hthread as usize]
    }

    /// Get the priority of the thread on a given hardware thread.
    pub fn get_prio(&self, hthread: u32) -> i16 {
        self.prios[hthread as usize]
    }
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
    fn test_runlist_empty() {
        let rl = RunList::new(4);
        assert_eq!(rl.worst_prio(), minivm_types::consts::MAX_PRIOS);
        assert_eq!(rl.worst_prio_hthread(), u32::MAX);
        assert_eq!(rl.get(0), IDX_NONE);
    }

    #[test]
    fn test_runlist_push() {
        let mut rl = RunList::new(4);
        let mut threads = make_threads(4);

        threads[1].hthread = 0;
        threads[1].prio = 10;
        rl.push(&mut threads, 1);

        assert_eq!(rl.get(0), 1);
        assert_eq!(rl.get_prio(0), 10);
        assert_eq!(threads[1].status, ThreadStatus::Running as u8);
    }

    #[test]
    fn test_runlist_worst_prio() {
        let mut rl = RunList::new(4);
        let mut threads = make_threads(4);

        threads[1].hthread = 0;
        threads[1].prio = 10;
        threads[2].hthread = 1;
        threads[2].prio = 50;
        threads[3].hthread = 2;
        threads[3].prio = 30;

        rl.push(&mut threads, 1);
        rl.push(&mut threads, 2);
        rl.push(&mut threads, 3);

        assert_eq!(rl.worst_prio(), 50);
        assert_eq!(rl.worst_prio_hthread(), 1);
    }

    #[test]
    fn test_runlist_remove() {
        let mut rl = RunList::new(4);
        let mut threads = make_threads(4);

        threads[1].hthread = 0;
        threads[1].prio = 10;
        rl.push(&mut threads, 1);
        rl.remove(&threads, 1);

        assert_eq!(rl.get(0), IDX_NONE);
        assert_eq!(rl.get_prio(0), -1);
    }
}
