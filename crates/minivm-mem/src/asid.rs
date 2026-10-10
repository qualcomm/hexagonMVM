/*
 * Copyright (c) Qualcomm Technologies, Inc. and/or its subsidiaries.
 * SPDX-License-Identifier: BSD-3-Clause-Clear
 */

//! ASID table (128 entries, ref-counting, eviction).
//!
//! The ASID table maps Address Space IDs to translation metadata.
//! Each entry contains a page table base (PTB), VM ID, translation type,
//! and a reference count. Entries are found via hash-based circular lookup
//! and evicted when no free slots exist.

use minivm_types::asid::{AsidEntry, AsidEntryFields, TranslationType};
use minivm_types::consts::{ASID_BITS, MAX_ASIDS};

/// Knuth multiplicative hash to map PTB to an ASID index.
const fn asid_hash(ptb: u32) -> u32 {
    let product = ptb.wrapping_mul(2654435761u32);
    // Extract ASID_BITS bits starting from bit (32 - ASID_BITS)
    (product >> (32 - ASID_BITS)) & ((1 << ASID_BITS) - 1)
}

/// Compute next index in circular probe sequence.
const fn next_idx(idx: u32, chain: u32) -> u32 {
    (idx.wrapping_add(chain)) & ((1u32 << ASID_BITS) - 1)
}

/// Bit width of x: smallest k such that 2^k > x (i.e., the number of bits needed to represent x).
const fn log2_greater(x: u32) -> u32 {
    if x == 0 {
        return 0;
    }
    32 - x.leading_zeros()
}

/// Match criteria: same PTB, same VM, same translation type.
fn asid_match(entry: &AsidEntry, ptb: u32, vmidx: u8, trans_type: u8) -> bool {
    entry.ptb == ptb && entry.fields.vmid() == vmidx && entry.fields.trans_type() == trans_type
}

/// The ASID table — 128 entries indexed by ASID number.
pub struct AsidTable {
    pub entries: [AsidEntry; MAX_ASIDS as usize],
}

impl Default for AsidTable {
    fn default() -> Self {
        Self::new()
    }
}

impl AsidTable {
    /// Create a zeroed ASID table.
    pub const fn new() -> Self {
        Self {
            entries: [AsidEntry::EMPTY; MAX_ASIDS as usize],
        }
    }

    /// Initialize the table (zero all entries).
    pub fn init(&mut self) {
        for entry in self.entries.iter_mut() {
            *entry = AsidEntry::EMPTY;
        }
    }

    /// Search for an existing entry matching (ptb, vmidx, type).
    ///
    /// Uses hash-based circular probing with `log_maxhops` to bound the search.
    /// Returns the ASID index if found, or `None`.
    pub fn search(&self, ptb: u32, vmidx: u8, trans_type: u8) -> Option<u32> {
        let start_idx = asid_hash(ptb);
        let chain = start_idx | 1; // Ensure odd step for full coverage
        let max_hops = 1u32 << self.entries[start_idx as usize].fields.log_maxhops();

        let mut idx = start_idx;
        for _ in 0..=max_hops {
            if asid_match(&self.entries[idx as usize], ptb, vmidx, trans_type) {
                return Some(idx);
            }
            idx = next_idx(idx, chain);
        }
        None
    }

    /// Find a free slot (count == 0) for a new entry with the given PTB hash.
    ///
    /// Updates the `log_maxhops` at the hash start to record chain length.
    /// Returns the ASID index if a free slot is found, or `None`.
    pub fn find_eviction_slot(&mut self, ptb: u32) -> Option<u32> {
        let start_idx = asid_hash(ptb);
        let chain = start_idx | 1;

        let mut idx = start_idx;
        for hops in 0..MAX_ASIDS {
            if self.entries[idx as usize].fields.count() == 0 {
                // Update log_maxhops at the start of the chain
                let log_maxhops = log2_greater(hops) as u8;
                let start = &mut self.entries[start_idx as usize];
                let old_lmh = start.fields.log_maxhops();
                if log_maxhops > old_lmh {
                    start.fields = start.fields.with_log_maxhops(log_maxhops);
                }
                return Some(idx);
            }
            idx = next_idx(idx, chain);
        }
        None
    }

    /// Increment the reference count for an ASID entry, or allocate a new one.
    ///
    /// If `invalidate` is true, TLB and STLB entries for this ASID are invalidated.
    /// Returns the ASID index on success, or -1 on failure.
    ///
    /// The caller must hold the ASID spinlock and provide callbacks for TLB/STLB
    /// invalidation.
    pub fn inc(
        &mut self,
        ptb: u32,
        trans_type: TranslationType,
        invalidate: bool,
        extra: u8,
        vmidx: u8,
        mut invalidate_fn: impl FnMut(u32),
    ) -> i32 {
        let ttype = trans_type as u8;

        // Check if entry already exists
        if let Some(idx) = self.search(ptb, vmidx, ttype) {
            let entry = &mut self.entries[idx as usize];
            let count = entry.fields.count();
            if count < 0xFFF {
                entry.fields = entry.fields.with_count(count + 1);
            }
            if invalidate {
                invalidate_fn(idx);
            }
            return idx as i32;
        }

        // Try to find a free slot via eviction
        if let Some(idx) = self.find_eviction_slot(ptb) {
            let entry = &mut self.entries[idx as usize];
            entry.ptb = ptb;
            entry.fields = AsidEntryFields(0)
                .with_type(trans_type)
                .with_vmid(vmidx)
                .with_count(1)
                .with_extra(extra);
            invalidate_fn(idx);
            return idx as i32;
        }

        -1 // No free slot available
    }

    /// Decrement the reference count for an ASID entry.
    ///
    /// The count is decremented atomically in the C implementation;
    /// here we do a simple decrement (caller must hold appropriate lock).
    pub fn dec(&mut self, asid: u32) {
        let entry = &mut self.entries[asid as usize];
        let count = entry.fields.count();
        if count > 0 {
            entry.fields = entry.fields.with_count(count - 1);
        }
    }

    /// Get a reference to an entry by ASID index.
    pub fn get(&self, asid: u32) -> &AsidEntry {
        &self.entries[asid as usize]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec::Vec;

    #[test]
    fn test_asid_hash_deterministic() {
        let h1 = asid_hash(0x12345678);
        let h2 = asid_hash(0x12345678);
        assert_eq!(h1, h2);
        assert!(h1 < MAX_ASIDS);
    }

    #[test]
    fn test_asid_hash_range() {
        for ptb in [0u32, 1, 0xFFFFFFFF, 0x80000000, 0xDEADBEEF] {
            let h = asid_hash(ptb);
            assert!(h < MAX_ASIDS, "hash {} out of range for ptb {:#x}", h, ptb);
        }
    }

    #[test]
    fn test_log2_greater() {
        assert_eq!(log2_greater(0), 0);
        assert_eq!(log2_greater(1), 1);
        assert_eq!(log2_greater(2), 2);
        assert_eq!(log2_greater(3), 2);
        assert_eq!(log2_greater(4), 3);
        assert_eq!(log2_greater(127), 7);
        assert_eq!(log2_greater(128), 8);
    }

    #[test]
    fn test_table_init() {
        let table = AsidTable::new();
        for entry in &table.entries {
            assert!(entry.is_empty());
        }
    }

    #[test]
    fn test_inc_allocates_new_entry() {
        let mut table = AsidTable::new();
        let mut invalidated = Vec::new();
        let asid = table.inc(0x1000_0000, TranslationType::Offset, false, 0, 1, |idx| {
            invalidated.push(idx)
        });
        assert!(asid >= 0);
        assert!(!invalidated.is_empty()); // New entries always invalidated

        let entry = table.get(asid as u32);
        assert_eq!(entry.ptb, 0x1000_0000);
        assert_eq!(entry.fields.count(), 1);
        assert_eq!(entry.fields.vmid(), 1);
        assert_eq!(entry.fields.trans_type(), TranslationType::Offset as u8);
    }

    #[test]
    fn test_inc_increments_existing() {
        let mut table = AsidTable::new();
        let asid1 = table.inc(0x2000, TranslationType::Linear, false, 0, 2, |_| {});
        let asid2 = table.inc(0x2000, TranslationType::Linear, false, 0, 2, |_| {});
        assert_eq!(asid1, asid2);
        assert_eq!(table.get(asid1 as u32).fields.count(), 2);
    }

    #[test]
    fn test_dec() {
        let mut table = AsidTable::new();
        let asid = table.inc(0x3000, TranslationType::Table, false, 0, 3, |_| {});
        assert_eq!(table.get(asid as u32).fields.count(), 1);
        table.dec(asid as u32);
        assert_eq!(table.get(asid as u32).fields.count(), 0);
    }

    #[test]
    fn test_search_not_found() {
        let table = AsidTable::new();
        assert!(table.search(0x9999, 1, 0).is_none());
    }

    #[test]
    fn test_search_after_inc() {
        let mut table = AsidTable::new();
        table.inc(0x4000, TranslationType::Offset, false, 0, 5, |_| {});
        assert!(table
            .search(0x4000, 5, TranslationType::Offset as u8)
            .is_some());
        assert!(table
            .search(0x4000, 5, TranslationType::Linear as u8)
            .is_none());
        assert!(table
            .search(0x4000, 6, TranslationType::Offset as u8)
            .is_none());
    }

    #[test]
    fn test_invalidate_flag() {
        let mut table = AsidTable::new();

        // First allocation always invalidates (new entry)
        let mut count = 0;
        table.inc(0x5000, TranslationType::Offset, false, 0, 1, |_| count += 1);
        assert_eq!(count, 1); // New entry → invalidate

        // Re-inc with invalidate=false should not call invalidate_fn
        let mut count2 = 0;
        table.inc(0x5000, TranslationType::Offset, false, 0, 1, |_| {
            count2 += 1
        });
        assert_eq!(count2, 0);

        // Re-inc with invalidate=true should call invalidate_fn
        let mut count3 = 0;
        table.inc(0x5000, TranslationType::Offset, true, 0, 1, |_| count3 += 1);
        assert_eq!(count3, 1);
    }
}
