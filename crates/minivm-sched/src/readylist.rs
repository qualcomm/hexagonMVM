/*
 * Copyright (c) Qualcomm Technologies, Inc. and/or its subsidiaries.
 * SPDX-License-Identifier: BSD-3-Clause-Clear
 */

//! Priority-based ready queue.
//!
//! The ready list is a priority queue backed by:
//! - A validity bitmap (`[u32; 8]` for 256 priorities) for fast "best priority" lookup
//! - Per-priority intrusive circular doubly-linked list (ring) heads
//!
//! Ring operations work on `ThreadContext` arrays using indices rather than raw
//! pointers, making them safe and testable on the host.
//!
//! Index 0 is reserved as "null" (no thread). Valid thread indices start at 1.

use crate::context::ThreadContext;
use minivm_types::consts::{MAX_PRIO, MAX_PRIOS};
use minivm_types::vm::ThreadStatus;

/// Sentinel index meaning "no thread" (equivalent to NULL pointer).
pub const IDX_NONE: u32 = 0;

/// Number of u32 words in the validity bitmap.
const VALIDS_WORDS: usize = MAX_PRIOS as usize / 32;

/// Ready list: priority-based queue of threads.
pub struct ReadyList {
    /// Validity bitmap: bit N set means priority N has at least one ready thread.
    /// Uses u32 words for reliable codegen on 32-bit targets (Hexagon).
    valids: [u32; VALIDS_WORDS],
    /// Per-priority ring head index (into a ThreadContext array).
    /// `IDX_NONE` means empty.
    heads: [u32; MAX_PRIOS as usize],
}

impl Default for ReadyList {
    fn default() -> Self {
        Self::new()
    }
}

impl ReadyList {
    /// Create an empty ready list.
    pub const fn new() -> Self {
        Self {
            valids: [0u32; VALIDS_WORDS],
            heads: [IDX_NONE; MAX_PRIOS as usize],
        }
    }

    /// Find the highest (numerically lowest) priority that has a ready thread.
    ///
    /// Returns `MAX_PRIOS` if no threads are ready.
    ///
    /// Uses trailing-zero count to scan the bitmap efficiently.
    pub fn best_prio(&self) -> u32 {
        let mut prio: u32 = 0;
        for i in 0..VALIDS_WORDS {
            let ct0 = self.valids[i].trailing_zeros();
            prio += ct0;
            if ct0 < 32 {
                return prio;
            }
        }
        prio
    }

    /// Check whether any threads are ready.
    pub fn any_valid(&self) -> bool {
        self.best_prio() < MAX_PRIOS
    }

    /// Check whether a thread at a given priority is ready.
    pub fn prio_valid(&self, prio: u32) -> bool {
        if prio > MAX_PRIO {
            return false;
        }
        let word = (prio >> 5) as usize;
        let bit = prio & 0x1f;
        (self.valids[word] >> bit) & 1 != 0
    }

    /// Set a priority as valid in the bitmap.
    fn set_prio(&mut self, prio: u32) {
        let word = (prio >> 5) as usize;
        let bit = prio & 0x1f;
        self.valids[word] |= 1u32 << bit;
    }

    /// Clear a priority from the bitmap.
    fn clear_prio(&mut self, prio: u32) {
        let word = (prio >> 5) as usize;
        let bit = prio & 0x1f;
        // Use volatile write to prevent Hexagon optimizer from eliding the store.
        let val = self.valids[word] & !(1u32 << bit);
        unsafe {
            core::ptr::write_volatile(&mut self.valids[word], val);
        }
    }

    /// Append a thread to the end of its priority ring (last to be scheduled).
    ///
    /// Sets the thread's status to Ready.
    pub fn append(&mut self, threads: &mut [ThreadContext], idx: u32) {
        let prio = threads[idx as usize].prio as u32;
        threads[idx as usize].status = ThreadStatus::Ready as u8;
        ring_append(&mut self.heads[prio as usize], threads, idx);
        self.set_prio(prio);
    }

    /// Insert a thread at the front of its priority ring (first to be scheduled).
    ///
    /// Sets the thread's status to Ready.
    pub fn insert(&mut self, threads: &mut [ThreadContext], idx: u32) {
        let prio = threads[idx as usize].prio as u32;
        threads[idx as usize].status = ThreadStatus::Ready as u8;
        ring_insert(&mut self.heads[prio as usize], threads, idx);
        self.set_prio(prio);
    }

    /// Remove a specific thread from the ready list.
    ///
    /// The caller guarantees that the thread is actually in the ready list.
    // inline(never) works around a Hexagon LLVM codegen bug where the
    // optimizer eliminates the bitmap clear in release builds.
    #[inline(never)]
    pub fn remove(&mut self, threads: &mut [ThreadContext], idx: u32) {
        let prio = threads[idx as usize].prio as u32;
        ring_remove(&mut self.heads[prio as usize], threads, idx);
        // Unconditionally clear the bitmap bit, then re-set if ring
        // is not empty. This avoids a conditional-clear pattern that
        // Hexagon LLVM miscompiles in release builds.
        self.clear_prio(prio);
        if self.heads[prio as usize] != IDX_NONE {
            self.set_prio(prio);
        }
    }

    /// Remove and return the best (highest-priority) ready thread.
    ///
    /// Returns `IDX_NONE` if no threads are ready.
    pub fn getbest(&mut self, threads: &mut [ThreadContext]) -> u32 {
        let prio = self.best_prio();
        if prio >= MAX_PRIOS {
            return IDX_NONE;
        }
        let head = self.heads[prio as usize];
        if head != IDX_NONE {
            self.remove(threads, head);
        }
        head
    }
}

// --- Ring buffer operations (index-based) ---
//
// These operate on circular doubly-linked lists using the `next`/`prev`
// fields of ThreadContext as indices into the thread array.

/// Append a node to the end of the ring (before the head).
fn ring_append(head: &mut u32, threads: &mut [ThreadContext], idx: u32) {
    if *head == IDX_NONE {
        // Empty ring: node points to itself
        threads[idx as usize].next = idx;
        threads[idx as usize].prev = idx;
        *head = idx;
    } else {
        // Insert before head (at the end of the ring)
        let head_idx = *head;
        let tail = threads[head_idx as usize].prev;
        threads[idx as usize].next = head_idx;
        threads[idx as usize].prev = tail;
        threads[tail as usize].next = idx;
        threads[head_idx as usize].prev = idx;
    }
}

/// Insert a node at the front of the ring (becomes the new head).
fn ring_insert(head: &mut u32, threads: &mut [ThreadContext], idx: u32) {
    if *head == IDX_NONE {
        threads[idx as usize].next = idx;
        threads[idx as usize].prev = idx;
    } else {
        let head_idx = *head;
        let tail = threads[head_idx as usize].prev;
        threads[idx as usize].next = head_idx;
        threads[idx as usize].prev = tail;
        threads[tail as usize].next = idx;
        threads[head_idx as usize].prev = idx;
    }
    *head = idx;
}

/// Remove a node from the ring.
fn ring_remove(head: &mut u32, threads: &mut [ThreadContext], idx: u32) {
    let prev = threads[idx as usize].prev;
    let next = threads[idx as usize].next;
    threads[prev as usize].next = next;
    threads[next as usize].prev = prev;

    if *head == idx {
        *head = next;
        if *head == idx {
            // Was the only element
            *head = IDX_NONE;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Create a thread array with `n` zeroed contexts.
    /// Index 0 is reserved as "null".
    fn make_threads(n: usize) -> alloc::vec::Vec<ThreadContext> {
        let mut v = alloc::vec::Vec::new();
        for _ in 0..n {
            v.push(ThreadContext::zeroed());
        }
        v
    }

    #[test]
    fn test_readylist_empty() {
        let rl = ReadyList::new();
        assert!(!rl.any_valid());
        assert_eq!(rl.best_prio(), MAX_PRIOS);
    }

    #[test]
    fn test_readylist_single_thread() {
        let mut rl = ReadyList::new();
        let mut threads = make_threads(4);

        threads[1].prio = 10;
        rl.append(&mut threads, 1);

        assert!(rl.any_valid());
        assert_eq!(rl.best_prio(), 10);
        assert!(rl.prio_valid(10));
        assert!(!rl.prio_valid(9));
        assert_eq!(threads[1].status, ThreadStatus::Ready as u8);
    }

    #[test]
    fn test_readylist_priority_ordering() {
        let mut rl = ReadyList::new();
        let mut threads = make_threads(8);

        threads[1].prio = 50;
        threads[2].prio = 10;
        threads[3].prio = 30;

        rl.append(&mut threads, 1);
        rl.append(&mut threads, 2);
        rl.append(&mut threads, 3);

        // Best priority should be 10 (numerically lowest)
        assert_eq!(rl.best_prio(), 10);

        // getbest should return thread at prio 10 first
        let best = rl.getbest(&mut threads);
        assert_eq!(best, 2);

        // Next best should be prio 30
        assert_eq!(rl.best_prio(), 30);
        let best = rl.getbest(&mut threads);
        assert_eq!(best, 3);

        // Then prio 50
        assert_eq!(rl.best_prio(), 50);
        let best = rl.getbest(&mut threads);
        assert_eq!(best, 1);

        // Now empty
        assert!(!rl.any_valid());
        assert_eq!(rl.getbest(&mut threads), IDX_NONE);
    }

    #[test]
    fn test_readylist_same_priority_fifo() {
        let mut rl = ReadyList::new();
        let mut threads = make_threads(8);

        // All at same priority
        threads[1].prio = 5;
        threads[2].prio = 5;
        threads[3].prio = 5;

        rl.append(&mut threads, 1);
        rl.append(&mut threads, 2);
        rl.append(&mut threads, 3);

        // Should come out in FIFO order
        assert_eq!(rl.getbest(&mut threads), 1);
        assert_eq!(rl.getbest(&mut threads), 2);
        assert_eq!(rl.getbest(&mut threads), 3);
        assert_eq!(rl.getbest(&mut threads), IDX_NONE);
    }

    #[test]
    fn test_readylist_insert_front() {
        let mut rl = ReadyList::new();
        let mut threads = make_threads(8);

        threads[1].prio = 5;
        threads[2].prio = 5;
        threads[3].prio = 5;

        rl.append(&mut threads, 1);
        rl.append(&mut threads, 2);
        // Insert at front (before thread 1)
        rl.insert(&mut threads, 3);

        // Thread 3 should come out first
        assert_eq!(rl.getbest(&mut threads), 3);
        assert_eq!(rl.getbest(&mut threads), 1);
        assert_eq!(rl.getbest(&mut threads), 2);
    }

    #[test]
    fn test_readylist_remove_middle() {
        let mut rl = ReadyList::new();
        let mut threads = make_threads(8);

        threads[1].prio = 5;
        threads[2].prio = 5;
        threads[3].prio = 5;

        rl.append(&mut threads, 1);
        rl.append(&mut threads, 2);
        rl.append(&mut threads, 3);

        // Remove middle
        rl.remove(&mut threads, 2);

        assert_eq!(rl.getbest(&mut threads), 1);
        assert_eq!(rl.getbest(&mut threads), 3);
        assert_eq!(rl.getbest(&mut threads), IDX_NONE);
    }

    #[test]
    fn test_readylist_remove_head() {
        let mut rl = ReadyList::new();
        let mut threads = make_threads(8);

        threads[1].prio = 5;
        threads[2].prio = 5;

        rl.append(&mut threads, 1);
        rl.append(&mut threads, 2);

        // Remove head
        rl.remove(&mut threads, 1);

        assert_eq!(rl.getbest(&mut threads), 2);
        assert_eq!(rl.getbest(&mut threads), IDX_NONE);
    }

    #[test]
    fn test_readylist_remove_last() {
        let mut rl = ReadyList::new();
        let mut threads = make_threads(4);

        threads[1].prio = 5;
        rl.append(&mut threads, 1);
        rl.remove(&mut threads, 1);

        assert!(!rl.any_valid());
        assert!(!rl.prio_valid(5));
    }

    #[test]
    fn test_readylist_high_priorities() {
        let mut rl = ReadyList::new();
        let mut threads = make_threads(4);

        threads[1].prio = 200;
        threads[2].prio = 255;

        rl.append(&mut threads, 1);
        rl.append(&mut threads, 2);

        assert_eq!(rl.best_prio(), 200);
        assert_eq!(rl.getbest(&mut threads), 1);
        assert_eq!(rl.best_prio(), 255);
        assert_eq!(rl.getbest(&mut threads), 2);
    }

    #[test]
    fn test_ring_append_single() {
        let mut threads = make_threads(4);
        let mut head = IDX_NONE;

        ring_append(&mut head, &mut threads, 1);
        assert_eq!(head, 1);
        assert_eq!(threads[1].next, 1);
        assert_eq!(threads[1].prev, 1);
    }

    #[test]
    fn test_ring_append_multiple() {
        let mut threads = make_threads(4);
        let mut head = IDX_NONE;

        ring_append(&mut head, &mut threads, 1);
        ring_append(&mut head, &mut threads, 2);
        ring_append(&mut head, &mut threads, 3);

        // Head is still 1
        assert_eq!(head, 1);
        // Ring: 1 -> 2 -> 3 -> 1 (forward)
        assert_eq!(threads[1].next, 2);
        assert_eq!(threads[2].next, 3);
        assert_eq!(threads[3].next, 1);
        // Ring: 1 -> 3 -> 2 -> 1 (backward)
        assert_eq!(threads[1].prev, 3);
        assert_eq!(threads[3].prev, 2);
        assert_eq!(threads[2].prev, 1);
    }

    #[test]
    fn test_ring_remove_only() {
        let mut threads = make_threads(4);
        let mut head = IDX_NONE;

        ring_append(&mut head, &mut threads, 1);
        ring_remove(&mut head, &mut threads, 1);
        assert_eq!(head, IDX_NONE);
    }
}
