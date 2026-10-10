/*
 * Copyright (c) Qualcomm Technologies, Inc. and/or its subsidiaries.
 * SPDX-License-Identifier: BSD-3-Clause-Clear
 */

//! Software TLB (set-associative cache).
//!
//! The STLB is a per-ASID set-associative cache of TLB entries that sits
//! between the hardware TLB and the full translation path. On a TLB miss,
//! the STLB is checked first; if a matching entry is found, it's inserted
//! directly into the hardware TLB without running the full translation.
//!
//! Organization:
//! - STLB_MAX_SETS sets (2048 by default)
//! - STLB_MAX_WAYS ways per set (4 by default)
//! - One `StlbAsidInfo` per ASID, each with its own validity bitmap and entries

use minivm_types::consts::{STLB_MAX_SETS, STLB_MAX_WAYS};
use minivm_types::pmap::PAGE_BITS;

/// Number of u64 words needed for the validity bitmap.
const VALIDS_LEN: usize = STLB_MAX_SETS as usize / 64;

/// Total entries per ASID.
pub const ENTRIES_PER_ASID: usize = STLB_MAX_SETS as usize * STLB_MAX_WAYS as usize;

/// Per-ASID STLB tracking info.
///
/// The `entries` pointer must point to a `STLB_MAX_SETS * STLB_MAX_WAYS`
/// array of 64-bit TLB entries allocated externally.
pub struct StlbAsidInfo {
    /// Validity bitmap: bit N indicates set N has been populated.
    pub valids: [u64; VALIDS_LEN],
    /// Pointer to TLB entry storage (STLB_MAX_SETS * STLB_MAX_WAYS entries).
    pub entries: *mut u64,
}

impl StlbAsidInfo {
    /// Create an empty STLB info with null entry pointer.
    pub const fn empty() -> Self {
        Self {
            valids: [0u64; VALIDS_LEN],
            entries: core::ptr::null_mut(),
        }
    }

    /// Check if this info has been initialized with entry storage.
    pub fn is_initialized(&self) -> bool {
        !self.entries.is_null()
    }

    /// Read the TLB entry at the given flat index.
    ///
    /// # Safety
    /// Caller must ensure `entries` is valid and `idx < ENTRIES_PER_ASID`.
    unsafe fn read_entry(&self, idx: usize) -> u64 {
        unsafe { *self.entries.add(idx) }
    }

    /// Write a TLB entry at the given flat index.
    ///
    /// # Safety
    /// Caller must ensure `entries` is valid and `idx < ENTRIES_PER_ASID`.
    unsafe fn write_entry(&mut self, idx: usize, val: u64) {
        unsafe {
            *self.entries.add(idx) = val;
        }
    }

    /// Check if a set has its validity bit set.
    fn is_set_valid(&self, set_idx: u32) -> bool {
        let word = (set_idx / 64) as usize;
        let bit = set_idx & 63;
        (self.valids[word] >> bit) & 1 != 0
    }

    /// Set the validity bit for a set.
    fn set_valid(&mut self, set_idx: u32) {
        let word = (set_idx / 64) as usize;
        let bit = set_idx & 63;
        self.valids[word] |= 1u64 << bit;
    }
}

/// Check if a TLB entry matches the given VA and ASID.
///
/// For ARCHV >= 4, uses the standard match: extract ASID from entry and
/// compare VPN with size-based masking.
fn stlb_check(va: u32, asid: u32, entry_raw: u64) -> bool {
    if entry_raw == 0 {
        return false;
    }
    // Extract fields from TLB entry
    let entry_hi = (entry_raw >> 32) as u32;
    let entry_lo = entry_raw as u32;

    let entry_vpn = entry_hi & 0x000F_FFFF; // bits 0-19 of high word
    let entry_asid = (entry_hi >> 20) & 0x7F; // bits 20-26 of high word
    let valid = (entry_hi >> 31) & 1;

    if valid == 0 || entry_asid != asid {
        return false;
    }

    // Determine page size by counting trailing zeros of low word
    let size = entry_lo.trailing_zeros();
    let vpn = va >> PAGE_BITS;
    let mask = !0u32 << (size * 2);

    (vpn & mask) == (entry_vpn & mask)
}

/// Check if a TLB entry's ASID matches.
fn stlb_match_asid(asid: u32, entry_raw: u64) -> bool {
    let entry_hi = (entry_raw >> 32) as u32;
    let entry_asid = (entry_hi >> 20) & 0x7F;
    entry_asid == asid
}

/// Get a pseudo-random way index for replacement.
///
/// On real hardware uses pcyclelo; for host tests uses a simple counter.
fn quick_random_way() -> u32 {
    #[cfg(target_arch = "hexagon")]
    {
        let tmp: u32;
        unsafe {
            core::arch::asm!("{tmp} = pcyclelo", tmp = out(reg) tmp, options(nomem, nostack));
        }
        tmp & (STLB_MAX_WAYS - 1)
    }
    #[cfg(not(target_arch = "hexagon"))]
    {
        // Simple deterministic replacement for testing
        static COUNTER: core::sync::atomic::AtomicU32 = core::sync::atomic::AtomicU32::new(0);
        let val = COUNTER.fetch_add(1, core::sync::atomic::Ordering::Relaxed);
        val & (STLB_MAX_WAYS - 1)
    }
}

/// Look up a VA in the STLB for a given ASID.
///
/// Returns the raw TLB entry if found, or 0 if not.
///
/// # Safety
/// Caller must ensure `info.entries` points to valid storage.
pub unsafe fn stlb_lookup(va: u32, asid: u32, info: &StlbAsidInfo) -> u64 {
    if !info.is_initialized() {
        return 0;
    }

    let set_idx = (va >> PAGE_BITS) & (STLB_MAX_SETS - 1);

    if !info.is_set_valid(set_idx) {
        return 0;
    }

    let base = (set_idx * STLB_MAX_WAYS) as usize;
    for way in 0..STLB_MAX_WAYS as usize {
        let entry = unsafe { info.read_entry(base + way) };
        if stlb_check(va, asid, entry) {
            return entry;
        }
    }

    0
}

/// Add a TLB entry to the STLB for a given ASID.
///
/// Uses pseudo-random replacement within the set.
///
/// # Safety
/// Caller must ensure `info.entries` points to valid storage.
pub unsafe fn stlb_add(va: u32, asid: u32, entry: u64, info: &mut StlbAsidInfo) {
    if !info.is_initialized() {
        return;
    }

    let set_idx = (va >> PAGE_BITS) & (STLB_MAX_SETS - 1);

    if !info.is_set_valid(set_idx) {
        // First use of this set - clear any stale entries matching this ASID
        let base = (set_idx * STLB_MAX_WAYS) as usize;
        for way in 0..STLB_MAX_WAYS as usize {
            let existing = unsafe { info.read_entry(base + way) };
            if stlb_match_asid(asid, existing) {
                unsafe {
                    info.write_entry(base + way, 0);
                }
            }
        }
        info.set_valid(set_idx);
    }

    let way = quick_random_way() as usize;
    let idx = (set_idx * STLB_MAX_WAYS) as usize + way;
    unsafe {
        info.write_entry(idx, entry);
    }
}

/// Invalidate STLB entries for a VA range within a given ASID.
///
/// # Safety
/// Caller must ensure `info.entries` points to valid storage.
pub unsafe fn stlb_invalidate_va(va: u32, count: u32, asid: u32, info: &mut StlbAsidInfo) {
    if !info.is_initialized() {
        return;
    }

    let start = va >> PAGE_BITS;
    let end = (va.wrapping_add(count).wrapping_sub(1)) >> PAGE_BITS;

    for page in start..=end {
        let search_va = page << PAGE_BITS;
        let set_idx = page & (STLB_MAX_SETS - 1);

        if !info.is_set_valid(set_idx) {
            continue;
        }

        let base = (set_idx * STLB_MAX_WAYS) as usize;
        for way in 0..STLB_MAX_WAYS as usize {
            let existing = unsafe { info.read_entry(base + way) };
            if stlb_check(search_va, asid, existing) {
                unsafe {
                    info.write_entry(base + way, 0);
                }
            }
        }
    }
}

/// Invalidate all STLB entries for a given ASID.
///
/// Clears the validity bitmap, effectively invalidating everything.
pub fn stlb_invalidate_asid(info: &mut StlbAsidInfo) {
    for v in info.valids.iter_mut() {
        *v = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;
    use alloc::vec::Vec;

    /// Helper to create STLB info backed by a Vec.
    struct StlbTestHelper {
        storage: Vec<u64>,
    }

    impl StlbTestHelper {
        fn new() -> Self {
            Self {
                storage: vec![0u64; ENTRIES_PER_ASID],
            }
        }

        fn info(&mut self) -> StlbAsidInfo {
            StlbAsidInfo {
                valids: [0u64; VALIDS_LEN],
                entries: self.storage.as_mut_ptr(),
            }
        }
    }

    /// Build a simple TLB entry for testing.
    /// Format: low=ppd, high=vpn|asid|valid
    fn make_test_entry(vpn: u32, asid: u32, ppn: u32, size: u8) -> u64 {
        // PPD: ppn shifted left 1 with sentinel bit at position `size`
        let ppn_masked = ppn & !((1u32 << size) - 1);
        let ppd = (ppn_masked << 1) | (1u32 << size);
        // High word: vpn | asid | valid
        let hi = (vpn & 0x000F_FFFF) | ((asid & 0x7F) << 20) | (1u32 << 31); // valid
        ((hi as u64) << 32) | (ppd as u64)
    }

    #[test]
    fn test_stlb_lookup_empty() {
        let mut helper = StlbTestHelper::new();
        let info = helper.info();
        let result = unsafe { stlb_lookup(0x1000, 1, &info) };
        assert_eq!(result, 0);
    }

    #[test]
    fn test_stlb_add_and_lookup() {
        let mut helper = StlbTestHelper::new();
        let mut info = helper.info();
        let entry = make_test_entry(0x10, 5, 0x20, 0);

        unsafe {
            stlb_add(0x10 << 12, 5, entry, &mut info);
        }
        let result = unsafe { stlb_lookup(0x10 << 12, 5, &info) };
        assert_eq!(result, entry);
    }

    #[test]
    fn test_stlb_lookup_wrong_asid() {
        let mut helper = StlbTestHelper::new();
        let mut info = helper.info();
        let entry = make_test_entry(0x10, 5, 0x20, 0);

        unsafe {
            stlb_add(0x10 << 12, 5, entry, &mut info);
        }
        // Look up with different ASID
        let result = unsafe { stlb_lookup(0x10 << 12, 6, &info) };
        assert_eq!(result, 0);
    }

    #[test]
    fn test_stlb_invalidate_asid() {
        let mut helper = StlbTestHelper::new();
        let mut info = helper.info();
        let entry = make_test_entry(0x10, 5, 0x20, 0);

        unsafe {
            stlb_add(0x10 << 12, 5, entry, &mut info);
        }
        stlb_invalidate_asid(&mut info);
        let result = unsafe { stlb_lookup(0x10 << 12, 5, &info) };
        assert_eq!(result, 0);
    }

    #[test]
    fn test_stlb_invalidate_va() {
        let mut helper = StlbTestHelper::new();
        let mut info = helper.info();
        let entry = make_test_entry(0x10, 5, 0x20, 0);

        unsafe {
            stlb_add(0x10 << 12, 5, entry, &mut info);
        }
        unsafe {
            stlb_invalidate_va(0x10 << 12, 0x1000, 5, &mut info);
        }
        let result = unsafe { stlb_lookup(0x10 << 12, 5, &info) };
        assert_eq!(result, 0);
    }

    #[test]
    fn test_stlb_uninit_returns_zero() {
        let info = StlbAsidInfo::empty();
        let result = unsafe { stlb_lookup(0x1000, 1, &info) };
        assert_eq!(result, 0);
    }
}
