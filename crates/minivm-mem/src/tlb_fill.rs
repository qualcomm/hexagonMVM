/*
 * Copyright (c) Qualcomm Technologies, Inc. and/or its subsidiaries.
 * SPDX-License-Identifier: BSD-3-Clause-Clear
 */

//! TLB miss handler.
//!
//! When the hardware TLB misses, this handler:
//! 1. Checks the STLB cache for a matching entry
//! 2. If STLB miss, runs the full translation path
//! 3. Converts the translation result to a TLB entry format
//! 4. Adds the result to the STLB and inserts into the hardware TLB
//! 5. If translation fails, triggers a page fault

use minivm_types::asid::AsidEntry;
use minivm_types::consts::PAGE_SIZE_MAX;
use minivm_types::pmap::PAGE_BITS;
use minivm_types::translate::Translation;

/// Convert a translation result to hardware TLB entry format.
///
/// Returns 0 if the translation has insufficient permissions (no R or W).
pub fn tlbfmt_from_translation(trans: Translation, va: u32, asid: u32) -> u64 {
    // Must have at least R or W permission
    if (trans.xwru() & 0b1110) == 0 {
        return 0;
    }

    let vpn = va >> PAGE_BITS;

    // Build high word: vpn | asid | PA high bits | valid
    let mut hi = (vpn & 0x000F_FFFF) | ((asid & 0x7F) << 20) | (1u32 << 31); // valid

    // PA bit 35
    hi |= ((trans.pn() >> (35 - PAGE_BITS)) & 0x1) << 29;
    // PA bits 36-37 (v73+)
    hi |= ((trans.pn() >> (36 - PAGE_BITS)) & 0x3) << 27;

    // Clamp TLB size
    let mut tlbsize = trans.size() as u32;
    if tlbsize > PAGE_SIZE_MAX as u32 {
        tlbsize = PAGE_SIZE_MAX as u32;
    }

    // Build PPD: ppn with sentinel bit encoding the size
    let ppn = trans.pn();
    let ppn_masked = ppn & ((!0u32) << tlbsize);
    let ppd_base = (ppn_masked << 1) | (1u32 << tlbsize);

    // Combine PPD with cache and permission bits
    let lo = (ppd_base & 0x00FF_FFFF)
        | ((trans.cccc() as u32 & 0xF) << 24)
        | ((trans.xwru() as u32 & 0xF) << 28);

    ((hi as u64) << 32) | (lo as u64)
}

/// TLB fill context: provides access to ASID table, STLB, and hardware TLB.
///
/// The caller must implement this trait to connect the TLB fill handler to
/// the rest of the system (ASID table lookup, STLB cache, TLB insertion,
/// translation dispatch, and page fault handling).
pub trait TlbFillCtx {
    /// Look up the ASID table entry for the given ASID.
    fn asid_entry(&self, asid: u32) -> AsidEntry;

    /// Check the STLB for a cached TLB entry.
    fn stlb_lookup(&self, va: u32, asid: u32) -> u64;

    /// Add an entry to the STLB cache.
    fn stlb_add(&mut self, va: u32, asid: u32, entry: u64);

    /// Insert a TLB entry into the hardware TLB.
    fn tlb_insert(&mut self, entry: u64);

    /// Run the full translation path.
    fn translate(&self, input: Translation, info: AsidEntry) -> Translation;

    /// Handle a page fault (translation failure).
    fn pagefault(&mut self, va: u32);
}

/// Handle a TLB miss for the given virtual address and ASID.
pub fn tlb_fill(ctx: &mut dyn TlbFillCtx, va: u32, asid: u32) {
    // Check STLB first
    let cached = ctx.stlb_lookup(va, asid);
    if cached != 0 {
        ctx.tlb_insert(cached);
        return;
    }

    // STLB miss - run full translation
    let info = ctx.asid_entry(asid);
    let trans = Translation::default_for_va(va);
    let result = ctx.translate(trans, info);

    let entry = tlbfmt_from_translation(result, va, asid);
    if entry == 0 {
        ctx.pagefault(va);
    } else {
        ctx.stlb_add(va, asid, entry);
        ctx.tlb_insert(entry);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec::Vec;
    use minivm_types::asid::{AsidEntryFields, TranslationType};
    use minivm_types::config::OffsetConfig;

    #[test]
    fn test_tlbfmt_from_translation_basic() {
        let trans = Translation(0)
            .with_pn(0x100)
            .with_size(0) // 4K
            .with_xwru(0xF) // all perms
            .with_cccc(7);

        let entry = tlbfmt_from_translation(trans, 0x100 << 12, 5);
        assert_ne!(entry, 0);

        // Verify high word fields
        let hi = (entry >> 32) as u32;
        let vpn = hi & 0x000F_FFFF;
        let entry_asid = (hi >> 20) & 0x7F;
        let valid = (hi >> 31) & 1;
        assert_eq!(vpn, 0x100);
        assert_eq!(entry_asid, 5);
        assert_eq!(valid, 1);

        // Verify low word fields
        let lo = entry as u32;
        let xwru = (lo >> 28) & 0xF;
        let cccc = (lo >> 24) & 0xF;
        assert_eq!(xwru, 0xF);
        assert_eq!(cccc, 7);

        // Verify size encoding via trailing zeros
        let ppd = lo & 0x00FF_FFFF;
        assert_eq!(ppd.trailing_zeros(), 0); // size=0 means sentinel at bit 0
    }

    #[test]
    fn test_tlbfmt_from_translation_no_perms() {
        // xwru=1 means only U bit, no R/W/X -> should return 0
        let trans = Translation(0).with_pn(0x100).with_size(0).with_xwru(0x1);

        let entry = tlbfmt_from_translation(trans, 0x100 << 12, 5);
        assert_eq!(entry, 0);
    }

    #[test]
    fn test_tlbfmt_from_translation_large_page() {
        let trans = Translation(0)
            .with_pn(0x100)
            .with_size(2) // 64K pages
            .with_xwru(0xF)
            .with_cccc(5);

        let entry = tlbfmt_from_translation(trans, 0x100 << 12, 3);
        assert_ne!(entry, 0);

        // Size 2 means sentinel at bit 2
        let ppd = entry as u32 & 0x00FF_FFFF;
        assert_eq!(ppd.trailing_zeros(), 2);
    }

    struct MockTlbFillCtx {
        asid_info: AsidEntry,
        stlb_entries: Vec<(u32, u32, u64)>, // (va, asid, entry)
        inserted: Vec<u64>,
        pagefaults: Vec<u32>,
    }

    impl MockTlbFillCtx {
        fn new(asid_info: AsidEntry) -> Self {
            Self {
                asid_info,
                stlb_entries: Vec::new(),
                inserted: Vec::new(),
                pagefaults: Vec::new(),
            }
        }
    }

    impl TlbFillCtx for MockTlbFillCtx {
        fn asid_entry(&self, _asid: u32) -> AsidEntry {
            self.asid_info
        }

        fn stlb_lookup(&self, va: u32, asid: u32) -> u64 {
            for &(v, a, e) in &self.stlb_entries {
                if v == va && a == asid {
                    return e;
                }
            }
            0
        }

        fn stlb_add(&mut self, va: u32, asid: u32, entry: u64) {
            self.stlb_entries.push((va, asid, entry));
        }

        fn tlb_insert(&mut self, entry: u64) {
            self.inserted.push(entry);
        }

        fn translate(&self, input: Translation, _info: AsidEntry) -> Translation {
            // Simple identity translation for testing
            input
        }

        fn pagefault(&mut self, va: u32) {
            self.pagefaults.push(va);
        }
    }

    #[test]
    fn test_tlb_fill_stlb_hit() {
        let info = AsidEntry::EMPTY;
        let mut ctx = MockTlbFillCtx::new(info);
        let fake_entry = 0xDEAD_BEEF_1234_5678u64;
        ctx.stlb_entries.push((0x1000, 5, fake_entry));

        tlb_fill(&mut ctx, 0x1000, 5);
        assert_eq!(ctx.inserted.len(), 1);
        assert_eq!(ctx.inserted[0], fake_entry);
        assert!(ctx.pagefaults.is_empty());
    }

    #[test]
    fn test_tlb_fill_stlb_miss_translate_success() {
        let offset = OffsetConfig::new(10, 7, true, 0xF, 0);
        let info = AsidEntry {
            ptb: offset.0,
            fields: AsidEntryFields(0)
                .with_type(TranslationType::Offset)
                .with_vmid(0)
                .with_count(1),
        };
        let mut ctx = MockTlbFillCtx::new(info);

        tlb_fill(&mut ctx, 0x1000, 5);
        assert_eq!(ctx.inserted.len(), 1);
        assert_ne!(ctx.inserted[0], 0);
        assert_eq!(ctx.stlb_entries.len(), 1); // Added to STLB
        assert!(ctx.pagefaults.is_empty());
    }

    #[test]
    fn test_tlb_fill_pagefault() {
        // Use a context that returns bad translation (no permissions)
        struct PfCtx {
            pagefaults: Vec<u32>,
            inserted: Vec<u64>,
        }
        impl TlbFillCtx for PfCtx {
            fn asid_entry(&self, _asid: u32) -> AsidEntry {
                AsidEntry::EMPTY
            }
            fn stlb_lookup(&self, _va: u32, _asid: u32) -> u64 {
                0
            }
            fn stlb_add(&mut self, _va: u32, _asid: u32, _entry: u64) {}
            fn tlb_insert(&mut self, entry: u64) {
                self.inserted.push(entry);
            }
            fn translate(&self, _input: Translation, _info: AsidEntry) -> Translation {
                Translation::BAD // No permissions
            }
            fn pagefault(&mut self, va: u32) {
                self.pagefaults.push(va);
            }
        }

        let mut pf_ctx = PfCtx {
            pagefaults: Vec::new(),
            inserted: Vec::new(),
        };
        tlb_fill(&mut pf_ctx, 0x1000, 5);
        assert!(pf_ctx.inserted.is_empty());
        assert_eq!(pf_ctx.pagefaults.len(), 1);
        assert_eq!(pf_ctx.pagefaults[0], 0x1000);
    }
}
