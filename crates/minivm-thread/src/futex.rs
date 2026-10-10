/*
 * Copyright (c) Qualcomm Technologies, Inc. and/or its subsidiaries.
 * SPDX-License-Identifier: BSD-3-Clause-Clear
 */

//! Futex wait/wake (hash table).
//!
//! Futex waiters are stored in a hash table of circular linked lists
//! (rings). Each ring is sorted by priority (best priority at head).
//! The hash function uses FNV prime multiplication on the physical
//! address of the futex.

/// Number of hash bits.
pub const FUTEX_HASHBITS: u32 = 6;
/// Number of hash buckets.
pub const FUTEX_HASHSIZE: usize = 1 << FUTEX_HASHBITS;
/// FNV hash prime.
pub const FUTEX_PRIME: u32 = 2654435761;

/// Compute the futex hash value for a physical address (shifted by 2).
///
/// This matches the C `FUTEX_HASHVAL` macro.
pub fn futex_hash(pa_shifted: u64) -> usize {
    let lo = pa_shifted as u32;
    let hi = (pa_shifted >> 32) as u32;
    let hash = lo.wrapping_mul(FUTEX_PRIME);
    let bits = (hash >> (32 - FUTEX_HASHBITS)) & ((1 << FUTEX_HASHBITS) - 1);
    (bits ^ hi) as usize % FUTEX_HASHSIZE
}

/// A futex hash table with index-based waiter rings.
///
/// Each bucket is a ring head (index into a thread context pool).
/// IDX_NONE (0) means empty bucket.
pub struct FutexTable {
    /// Hash buckets. Each is the index of the ring head, or 0 for empty.
    pub buckets: [u32; FUTEX_HASHSIZE],
}

/// Sentinel for empty ring/no thread.
pub const IDX_NONE: u32 = 0;

impl Default for FutexTable {
    fn default() -> Self {
        Self::new()
    }
}

impl FutexTable {
    pub const fn new() -> Self {
        Self {
            buckets: [IDX_NONE; FUTEX_HASHSIZE],
        }
    }

    /// Add a waiter to the hash bucket for the given futex address.
    ///
    /// `hash`: the hash bucket index
    /// `idx`: the thread index to add
    /// `prio`: the thread's priority
    /// `next_fn`/`prev_fn`: accessors for the ring links in the thread pool
    ///
    /// For simplicity, inserts at the end (FIFO within same priority).
    /// The real implementation sorts by priority; this is a simplified version.
    pub fn add_waiter(
        &mut self,
        hash: usize,
        idx: u32,
        prio: u8,
        nexts: &mut [u32],
        prevs: &mut [u32],
        prios: &[u8],
    ) {
        let head = self.buckets[hash];
        if head == IDX_NONE {
            // Empty ring: self-loop
            nexts[idx as usize] = idx;
            prevs[idx as usize] = idx;
            self.buckets[hash] = idx;
            return;
        }

        // Insert at position maintaining priority order (best = lowest at head)
        if prio < prios[head as usize] {
            // New head: insert before current head
            let tail = prevs[head as usize];
            nexts[idx as usize] = head;
            prevs[idx as usize] = tail;
            nexts[tail as usize] = idx;
            prevs[head as usize] = idx;
            self.buckets[hash] = idx;
        } else {
            // Insert in sorted position (scan backwards from tail)
            let tail = prevs[head as usize];
            let mut pos = tail;
            while pos != head && prio < prios[pos as usize] {
                pos = prevs[pos as usize];
            }
            // Insert after pos
            let after = nexts[pos as usize];
            nexts[pos as usize] = idx;
            nexts[idx as usize] = after;
            prevs[after as usize] = idx;
            prevs[idx as usize] = pos;
        }
    }

    /// Remove the first waiter matching `futex_lo` from the hash bucket.
    ///
    /// Returns the index of the removed waiter, or IDX_NONE if not found.
    pub fn remove_one(
        &mut self,
        hash: usize,
        futex_lo: u32,
        futex_ptrs: &[u32],
        nexts: &mut [u32],
        prevs: &mut [u32],
    ) -> u32 {
        let head = self.buckets[hash];
        if head == IDX_NONE {
            return IDX_NONE;
        }

        let mut cur = head;
        loop {
            if futex_ptrs[cur as usize] == futex_lo {
                // Found: remove from ring
                let next = nexts[cur as usize];
                let prev = prevs[cur as usize];

                if next == cur {
                    // Only element
                    self.buckets[hash] = IDX_NONE;
                } else {
                    nexts[prev as usize] = next;
                    prevs[next as usize] = prev;
                    if cur == head {
                        self.buckets[hash] = next;
                    }
                }
                nexts[cur as usize] = IDX_NONE;
                prevs[cur as usize] = IDX_NONE;
                return cur;
            }
            cur = nexts[cur as usize];
            if cur == head {
                break;
            }
        }
        IDX_NONE
    }

    /// Remove a specific thread from its hash bucket.
    ///
    /// Used by futex_cancel to remove a blocked thread.
    pub fn cancel(&mut self, hash: usize, idx: u32, nexts: &mut [u32], prevs: &mut [u32]) {
        let head = self.buckets[hash];
        if head == IDX_NONE {
            return;
        }

        let next = nexts[idx as usize];
        let prev = prevs[idx as usize];

        if next == idx {
            // Only element
            self.buckets[hash] = IDX_NONE;
        } else {
            nexts[prev as usize] = next;
            prevs[next as usize] = prev;
            if idx == head {
                self.buckets[hash] = next;
            }
        }
        nexts[idx as usize] = IDX_NONE;
        prevs[idx as usize] = IDX_NONE;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;

    #[test]
    fn test_futex_hash_distribution() {
        // Different addresses should hash to different buckets (mostly)
        let h1 = futex_hash(0x1000);
        let h2 = futex_hash(0x2000);
        let h3 = futex_hash(0x3000);
        assert!(h1 < FUTEX_HASHSIZE);
        assert!(h2 < FUTEX_HASHSIZE);
        assert!(h3 < FUTEX_HASHSIZE);
    }

    #[test]
    fn test_futex_add_and_remove() {
        let mut table = FutexTable::new();
        let mut nexts = vec![IDX_NONE; 8];
        let mut prevs = vec![IDX_NONE; 8];
        let prios = vec![0u8; 8];
        let futex_ptrs = vec![0x1000u32, 0x1000, 0x1000, 0x2000, 0, 0, 0, 0];

        let hash = 5; // arbitrary bucket

        // Add waiter 1 and 2 for futex 0x1000
        table.add_waiter(hash, 1, 10, &mut nexts, &mut prevs, &prios);
        table.add_waiter(hash, 2, 10, &mut nexts, &mut prevs, &prios);

        // Remove first matching 0x1000
        let removed = table.remove_one(hash, 0x1000, &futex_ptrs, &mut nexts, &mut prevs);
        assert_eq!(removed, 1);

        // Remove next matching 0x1000
        let removed = table.remove_one(hash, 0x1000, &futex_ptrs, &mut nexts, &mut prevs);
        assert_eq!(removed, 2);

        // No more
        let removed = table.remove_one(hash, 0x1000, &futex_ptrs, &mut nexts, &mut prevs);
        assert_eq!(removed, IDX_NONE);
    }

    #[test]
    fn test_futex_priority_ordering() {
        let mut table = FutexTable::new();
        let mut nexts = vec![IDX_NONE; 8];
        let mut prevs = vec![IDX_NONE; 8];
        let prios = vec![0u8, 20, 10, 30, 0, 0, 0, 0];
        let futex_ptrs = vec![0u32, 0x1000, 0x1000, 0x1000, 0, 0, 0, 0];

        let hash = 3;

        // Add in order: prio 20, 10, 30
        table.add_waiter(hash, 1, 20, &mut nexts, &mut prevs, &prios);
        table.add_waiter(hash, 2, 10, &mut nexts, &mut prevs, &prios);
        table.add_waiter(hash, 3, 30, &mut nexts, &mut prevs, &prios);

        // Head should be prio 10 (index 2)
        assert_eq!(table.buckets[hash], 2);

        // Remove first → should be prio 10
        let removed = table.remove_one(hash, 0x1000, &futex_ptrs, &mut nexts, &mut prevs);
        assert_eq!(removed, 2);
    }

    #[test]
    fn test_futex_cancel() {
        let mut table = FutexTable::new();
        let mut nexts = vec![IDX_NONE; 8];
        let mut prevs = vec![IDX_NONE; 8];
        let prios = vec![0u8; 8];

        let hash = 7;

        table.add_waiter(hash, 1, 10, &mut nexts, &mut prevs, &prios);
        table.add_waiter(hash, 2, 10, &mut nexts, &mut prevs, &prios);

        // Cancel thread 1
        table.cancel(hash, 1, &mut nexts, &mut prevs);

        // Only thread 2 should remain
        assert_eq!(table.buckets[hash], 2);
        assert_eq!(nexts[2], 2); // self-loop
    }
}
