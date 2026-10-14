/*
 * Copyright (c) Qualcomm Technologies, Inc. and/or its subsidiaries.
 * SPDX-License-Identifier: BSD-3-Clause-Clear
 */

//! Conversion of a translation result to the hardware TLB format.

use minivm_types::pmap::PAGE_BITS;
use minivm_types::tlb::{mapped_phys_range, TlbEntry};
use minivm_types::translate::Translation;

pub fn tlbfmt_from_translation(
    trans: Translation,
    va: u32,
    asid: u32,
    max_page_size: u8,
    extended_pa: bool,
) -> u64 {
    TlbEntry::from_translation(trans, va, asid as u8, max_page_size, extended_pa).0
}

/// Confine a translation to the physical fences (page numbers, both
/// inclusive) and keep it clear of the `protected` byte ranges.
///
/// A large page is only as usable as all of it: one that reaches a
/// protected range or a fence is cut down, a size at a time, until the
/// page holding the faulting address is clear of them, so a guest keeps
/// the memory beside a protected range. Where even that 4K page is
/// not, the result is `Translation::BAD`.
pub fn confine(
    trans: Translation,
    max_size: u8,
    fences: (u32, u32),
    protected: &[(u64, u64)],
) -> Translation {
    if trans.is_bad() {
        return trans;
    }
    let mut t = if trans.size() > max_size {
        trans.with_size(max_size)
    } else {
        trans
    };
    let lo = (fences.0 as u64) << PAGE_BITS;
    let end = (fences.1 as u64 + 1) << PAGE_BITS;
    loop {
        let (start, stop) = mapped_phys_range(t, max_size);
        let clear =
            start >= lo && stop <= end && !protected.iter().any(|p| start < p.1 && p.0 < stop);
        if clear {
            return t;
        }
        if t.size() == 0 {
            return Translation::BAD;
        }
        t = t.with_size(t.size() - 1);
    }
}

/// How many 4K pages the byte range [va, va + bytes) touches.
pub fn pages_spanned(va: u32, bytes: u32) -> u64 {
    if bytes == 0 {
        return 0;
    }
    let first = (va >> PAGE_BITS) as u64;
    let last = (va as u64 + bytes as u64 - 1) >> PAGE_BITS;
    last - first + 1
}

#[cfg(test)]
mod tests {
    use super::*;

    const FENCES: (u32, u32) = (0, 0xf_ffff);

    fn large(pn: u32, size: u8) -> Translation {
        Translation(0).with_pn(pn).with_size(size).with_xwru(0xf)
    }

    #[test]
    fn confine_keeps_a_clear_page() {
        let t = large(0x88c00, 5);
        assert_eq!(confine(t, 6, FENCES, &[]), t);
    }

    #[test]
    fn confine_cuts_a_page_around_the_monitor() {
        // Monitor image at 0x88f00000; the 4M page [0x88c00000, 0x89000000)
        // is cut until it holds only the faulting page, at 0x88e00000.
        let monitor = (0x88f0_0000u64, 0x8900_0000u64);
        let t = confine(large(0x88e00, 5), 6, FENCES, &[monitor]);
        assert_eq!(t.size(), 4); // 1M: [0x88e00000, 0x88f00000)
        assert_eq!(mapped_phys_range(t, 6), (0x88e0_0000, 0x88f0_0000));
    }

    #[test]
    fn confine_refuses_the_protected_page_itself() {
        let monitor = (0x88f0_0000u64, 0x8900_0000u64);
        assert!(confine(large(0x88f00, 5), 6, FENCES, &[monitor]).is_bad());
        assert!(confine(large(0x88f80, 0), 6, FENCES, &[monitor]).is_bad());
    }

    #[test]
    fn confine_cuts_a_page_at_a_fence() {
        let t = confine(large(0xfe000, 5), 6, (0, 0xfe000), &[]);
        assert_eq!(t.size(), 0);
        assert!(confine(large(0xfe001, 0), 6, (0, 0xfe000), &[]).is_bad());
        // Below the lower fence, too.
        assert!(confine(large(0x0ff, 0), 6, (0x100, 0xfe000), &[]).is_bad());
    }

    #[test]
    fn confine_clamps_to_the_core_page_size() {
        assert_eq!(confine(large(0x10000, 6), 5, FENCES, &[]).size(), 5);
    }

    #[test]
    fn pages_spanned_counts_bytes() {
        assert_eq!(pages_spanned(0x1000, 0), 0);
        assert_eq!(pages_spanned(0x1000, 1), 1);
        assert_eq!(pages_spanned(0x1000, 0x1000), 1);
        assert_eq!(pages_spanned(0x1000, 0x1001), 2);
        assert_eq!(pages_spanned(0x1fff, 2), 2);
        assert_eq!(pages_spanned(0x1800, 0x1000), 2);
        assert_eq!(pages_spanned(0xffff_f000, 0x1000), 1);
        assert_eq!(pages_spanned(0xffff_f000, 0x2000), 2);
    }

    #[test]
    fn formats_basic_mapping() {
        let trans = Translation(0)
            .with_pn(0x100)
            .with_size(0)
            .with_xwru(0xf)
            .with_cccc(7);
        let entry = TlbEntry(tlbfmt_from_translation(trans, 0x10_0000, 5, 6, false));
        assert!(entry.is_valid());
        assert_eq!(entry.asid(), 5);
        assert_eq!(entry.permissions(), 0xf);
        assert_eq!(entry.cache(), 7);
    }

    #[test]
    fn rejects_mapping_without_access() {
        let trans = Translation(0).with_pn(0x100).with_xwru(1);
        assert_eq!(tlbfmt_from_translation(trans, 0, 5, 6, false), 0);
    }

    #[test]
    fn clamps_oversized_mapping() {
        let trans = Translation(0).with_pn(0x100).with_size(9).with_xwru(0xf);
        let entry = TlbEntry(tlbfmt_from_translation(trans, 0, 3, 6, false));
        assert_eq!(entry.size(), 6);
    }
}
