/*
 * Copyright (c) Qualcomm Technologies, Inc. and/or its subsidiaries.
 * SPDX-License-Identifier: BSD-3-Clause-Clear
 */

//! Variable-radix (`TranslationType::Varadix`) translation.
//!
//! A guest selects it with `vmnewmap(root, VARADIX, flush, vpn_bits)`: the
//! tables translate the low `vpn_bits` bits of the virtual page number, and
//! any page number at or above `1 << vpn_bits` faults. Every entry is one
//! 32-bit word, and the position of its lowest set bit, `k`, says what the
//! entry is:
//!
//! * A **leaf** maps `2^k` 4K pages. It sits where `k` page number bits are
//!   left to translate. Bits 31..28 are X, W, R, U; bit 27 weak_ccc; bits
//!   26..24 the cache attributes; bits 22..k+1 the physical page number
//!   above the bottom `k` bits (a 22-bit page number in all, so 16G of
//!   physical memory). An odd `k`, or one above 16M, makes TLB entries of
//!   the next size down.
//! * Otherwise, where more than `k` bits are left, it **points** at a table
//!   of `2^(k+1)` entries, indexed by the next `k+1` page number bits: the
//!   entry, with those low `k+1` bits replaced by the index, is the
//!   table entry's physical address divided by 4. A table is therefore
//!   aligned to its size.
//! * Where fewer than `k` bits are left, or for a zero entry, nothing is
//!   mapped.
//!
//! The root entry is the ASID entry's `ptb`; it may be a leaf for the
//! whole space. Each table level consumes at least one bit, so a walk ends
//! after at most `vpn_bits` reads. The walk follows H2's
//! `kernel/mem/varadix/varadix.ref.c`.

use minivm_types::asid::AsidEntry;
use minivm_types::pmap::PAGE_BITS;
use minivm_types::translate::Translation;

use super::{physread_word, TranslateCtx};

/// Width of the physical page number in a leaf, and the widest virtual
/// page number the tables can translate.
pub const PN_BITS: u32 = 22;

/// Leaf sizes above this (16M) are clamped to it.
const MAX_LEAF_BITS: u32 = 12;

/// Whether `vmnewmap` may switch to a varadix space rooted at `root`
/// translating `vpn_bits` bits: the width must fit a leaf's page number,
/// and the root entry must not be a leaf bigger than the space.
pub const fn valid_root(root: u32, vpn_bits: u32) -> bool {
    vpn_bits <= PN_BITS && root.trailing_zeros() <= vpn_bits
}

/// The translation a leaf entry maps `2^size` pages with.
fn leaf(vpn: u32, entry: u32, size: u32) -> Translation {
    let width = PN_BITS - size;
    let ppn = (entry >> (size + 1)) & ((1 << width) - 1);
    // Keep the page number bits below the leaf, in case a later stage (or
    // an odd size) makes the page smaller.
    let pn = (ppn << size) | (vpn & ((1 << size) - 1));
    Translation(0)
        .with_pn(pn)
        .with_size((size.min(MAX_LEAF_BITS) / 2) as u8)
        .with_xwru((entry >> 28) as u8)
        .with_weak_ccc((entry >> 27) & 1 != 0)
        .with_cccc(((entry >> 24) & 7) as u8)
}

/// Walk from a table pointer whose table of `2^bits` entries is indexed by
/// the page number bits from `startbit` up.
fn walk(
    ctx: &dyn TranslateCtx,
    vpn: u32,
    mut table: u32,
    mut bits: u32,
    mut startbit: u32,
    guestmap: AsidEntry,
) -> Translation {
    loop {
        let mask = (1u32 << bits) - 1;
        let index = (vpn >> startbit) & mask;
        let mut pa = (((table & !mask) | index) as u64) << 2;

        if !guestmap.is_empty() {
            let tmp = ctx.translate(
                Translation::default_for_va(0).with_pn((pa >> PAGE_BITS) as u32),
                guestmap,
            );
            if tmp.xwru() & 0x2 == 0 {
                return Translation::BAD; // no read permission on the table
            }
            pa = ((tmp.pn() as u64) << PAGE_BITS) | (pa & ((1 << PAGE_BITS) - 1));
        }

        let entry = physread_word(ctx, pa);
        let size = entry.trailing_zeros();
        if size > startbit {
            return Translation::BAD;
        }
        if size == startbit {
            return leaf(vpn, entry, size);
        }
        table = entry;
        bits = size + 1;
        startbit -= size + 1;
    }
}

/// Translate through the varadix tables rooted at the ASID entry's `ptb`,
/// `extra` page number bits wide.
pub fn varadix_translate(
    ctx: &dyn TranslateCtx,
    input: Translation,
    info: AsidEntry,
) -> Translation {
    let vpn_bits = info.extra() as u32;
    let root = info.ptb;
    let root_size = root.trailing_zeros();
    let guestmap = ctx.vmblock_guestmap(info.vmid());
    let vpn = input.pn();

    let new = if !valid_root(root, vpn_bits) || vpn >> vpn_bits != 0 {
        Translation::BAD
    } else if root_size == vpn_bits {
        leaf(vpn, root, root_size)
    } else {
        walk(
            ctx,
            vpn,
            root,
            root_size + 1,
            vpn_bits - (root_size + 1),
            guestmap,
        )
    };
    if new.is_bad() {
        return Translation::BAD;
    }

    let mut out = input
        .with_pn(new.pn())
        .with_xwru(input.xwru() & new.xwru())
        .with_shared(false);
    if out.weak_ccc() {
        out = out.with_cccc(new.cccc());
    }
    out = out
        .with_size(input.size().min(new.size()))
        .with_weak_ccc(new.weak_ccc());

    // A varadix leaf is never shared: it always goes through the VM's
    // guest map, when there is one.
    if !guestmap.is_empty() {
        return ctx.translate(out, guestmap);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::collections::BTreeMap;
    use alloc::vec::Vec;
    use minivm_types::asid::{AsidEntryFields, TranslationType};

    /// The guest map's `ptb` in these tests: it adds `STAGE2_DELTA` to the
    /// page number and masks the permissions with `stage2_xwru`.
    const STAGE2_PTB: u32 = 0x5747_0002;
    const STAGE2_DELTA: u32 = 0x10_0000;

    const X: u32 = 8;
    const W: u32 = 4;
    const R: u32 = 2;
    const U: u32 = 1;

    struct Ctx {
        words: BTreeMap<u64, u32>,
        guestmap: AsidEntry,
        stage2_xwru: u8,
        /// Page numbers the guest map was asked to translate, in order.
        stage2_calls: core::cell::RefCell<Vec<u32>>,
    }

    impl Ctx {
        fn new() -> Self {
            Self {
                words: BTreeMap::new(),
                guestmap: AsidEntry::EMPTY,
                stage2_xwru: 0xf,
                stage2_calls: core::cell::RefCell::new(Vec::new()),
            }
        }

        fn write(&mut self, pa: u64, val: u32) {
            assert_eq!(pa & 3, 0);
            self.words.insert(pa, val);
        }

        fn read(&self, pa: u64) -> u32 {
            self.words.get(&pa).copied().unwrap_or(0)
        }

        fn with_stage2(mut self) -> Self {
            self.guestmap = AsidEntry {
                ptb: STAGE2_PTB,
                fields: AsidEntryFields(0)
                    .with_type(TranslationType::Offset)
                    .with_vmid(1)
                    .with_count(1),
            };
            self
        }
    }

    impl TranslateCtx for Ctx {
        fn translate(&self, input: Translation, info: AsidEntry) -> Translation {
            if info.ptb == STAGE2_PTB {
                self.stage2_calls.borrow_mut().push(input.pn());
                return input
                    .with_pn(input.pn() + STAGE2_DELTA)
                    .with_xwru(input.xwru() & self.stage2_xwru);
            }
            crate::translate::translate(self, input, info)
        }
        fn vmblock_guestmap(&self, vmidx: u8) -> AsidEntry {
            if vmidx == 2 {
                self.guestmap
            } else {
                AsidEntry::EMPTY
            }
        }
        fn vmblock_fence_lo(&self, _vmidx: u8) -> u32 {
            0
        }
        fn vmblock_fence_hi(&self, _vmidx: u8) -> u32 {
            !0
        }
        fn tcm_range(&self) -> (u32, u32) {
            (0, 0)
        }
        fn vtcm_range(&self) -> (u32, u32) {
            (0, 0)
        }
        fn physread_dword(&self, pa: u64) -> u64 {
            assert_eq!(pa & 7, 0);
            self.read(pa) as u64 | (self.read(pa + 4) as u64) << 32
        }
    }

    fn info(root: u32, vpn_bits: u8) -> AsidEntry {
        AsidEntry {
            ptb: root,
            fields: AsidEntryFields(0)
                .with_type(TranslationType::Varadix)
                .with_vmid(2)
                .with_count(1)
                .with_extra(vpn_bits),
        }
    }

    /// A leaf mapping `2^k` pages at physical page `ppn`.
    fn leaf_entry(k: u32, ppn: u32, xwru: u32, cccc: u32, weak: bool) -> u32 {
        assert_eq!(ppn & ((1 << k) - 1), 0);
        (xwru << 28) | ((weak as u32) << 27) | (cccc << 24) | ((ppn >> k) << (k + 1)) | (1 << k)
    }

    /// A pointer to a table of `2^bits` entries at physical address `pa`.
    fn table_entry(pa: u64, bits: u32) -> u32 {
        assert_eq!(pa & ((4 << bits) - 1), 0, "table not aligned to its size");
        ((pa >> 2) as u32) | (1 << (bits - 1))
    }

    fn go(ctx: &Ctx, info: AsidEntry, vpn: u32) -> Translation {
        let input = Translation::default_for_va(0)
            .with_pn(vpn)
            .with_cccc(7)
            .with_weak_ccc(true);
        crate::translate::translate(ctx, input, info)
    }

    #[test]
    fn root_leaf_maps_the_whole_space() {
        let ctx = Ctx::new();
        let root = leaf_entry(8, 0x1_2300, X | R | U, 5, false);
        let i = info(root, 8);
        for vpn in [0, 0x7f, 0xff] {
            let t = go(&ctx, i, vpn);
            assert_eq!(t.pn(), 0x1_2300 | vpn);
            assert_eq!(t.xwru() as u32, X | R | U);
            assert_eq!(t.cccc(), 5);
            assert!(!t.weak_ccc());
            assert_eq!(t.size(), 4); // 2^8 pages: 1M
        }
        assert!(go(&ctx, i, 0x100).is_bad());
    }

    #[test]
    fn two_level_walk() {
        let mut ctx = Ctx::new();
        // 20-bit space: a 256-entry root of 16M slots, one of which points
        // at a 16-entry table of 1M leaves.
        let root_pa = 0x10_0000;
        let l2_pa = 0x20_0000;
        let root = table_entry(root_pa, 8);
        ctx.write(root_pa + 4 * 0x45, table_entry(l2_pa, 4));
        ctx.write(
            l2_pa + 4 * 0x3,
            leaf_entry(8, 0x8_8000, X | W | R, 7, false),
        );
        let i = info(root, 20);

        let t = go(&ctx, i, 0x45342);
        assert!(!t.is_bad());
        assert_eq!(t.pn(), 0x8_8042);
        assert_eq!(t.size(), 4);
        assert_eq!(t.xwru() as u32, X | W | R);
        assert_eq!(t.cccc(), 7);

        // The neighbours in each table are empty.
        assert!(go(&ctx, i, 0x45442).is_bad());
        assert!(go(&ctx, i, 0x46342).is_bad());
    }

    #[test]
    fn every_leaf_size() {
        // A chain of one-bit tables down to a leaf of each size.
        for k in 0..=PN_BITS {
            let vpn_bits = PN_BITS;
            let mut ctx = Ctx::new();
            let vpn = 0x2a_5a5a & ((1 << vpn_bits) - 1);
            let ppn = 0x15_0000 & (!0u32 << k) & ((1 << PN_BITS) - 1);
            let mut next_pa = 0x100u64;
            let mut alloc = |bits: u32| {
                let align = 4u64 << bits;
                next_pa = (next_pa + align - 1) & !(align - 1);
                let pa = next_pa;
                next_pa += align;
                pa
            };
            let lf = leaf_entry(k, ppn, X | W | R | U, 3, true);
            let root = if k == vpn_bits {
                lf
            } else {
                let pa = alloc(1);
                let mut table = pa;
                let mut startbit = vpn_bits - 1;
                loop {
                    let slot = table + 4 * ((vpn >> startbit) & 1) as u64;
                    if startbit == k {
                        ctx.write(slot, lf);
                        break;
                    }
                    let next = alloc(1);
                    ctx.write(slot, table_entry(next, 1));
                    table = next;
                    startbit -= 1;
                }
                table_entry(pa, 1)
            };
            let i = info(root, vpn_bits as u8);
            let t = go(&ctx, i, vpn);
            assert!(!t.is_bad(), "k={k}");
            assert_eq!(t.pn(), ppn | (vpn & ((1 << k) - 1)), "k={k}");
            assert_eq!(t.size() as u32, k.min(12) / 2, "k={k}");
            assert_eq!(t.cccc(), 3, "k={k}");
            assert!(t.weak_ccc(), "k={k}");
            if k < vpn_bits {
                assert!(go(&ctx, i, vpn ^ (1 << k)).is_bad(), "k={k}");
            }
        }
    }

    #[test]
    fn odd_sizes_round_down() {
        let ctx = Ctx::new();
        // 8K, 32K, 128K, 512K, 2M, 8M: TLB pages of 4K .. 4M.
        for k in [1u32, 3, 5, 7, 9, 11] {
            let t = go(&ctx, info(leaf_entry(k, 0, R, 0, false), k as u8), 1);
            assert_eq!(t.size() as u32, (k - 1) / 2);
            assert_eq!(t.pn(), 1);
        }
    }

    #[test]
    fn entry_too_big_for_its_level_faults() {
        let mut ctx = Ctx::new();
        let root = table_entry(0x1000, 4);
        // At startbit 16 - 4 = 12, a 64M leaf (k = 14) does not fit; entry
        // 3 is zero, with no set bit at all.
        ctx.write(0x1000 + 4 * 2, leaf_entry(14, 0, R, 0, false));
        let i = info(root, 16);
        assert!(go(&ctx, i, 0x2_000).is_bad());
        assert!(go(&ctx, i, 0x3_000).is_bad());
    }

    #[test]
    fn bad_roots_and_widths() {
        let ctx = Ctx::new();
        let l = leaf_entry(4, 0, R, 0, false);
        // A root leaf bigger than the space, a zero root, a width past the
        // leaf's page number, and a page number past the width.
        assert!(!valid_root(l, 3));
        assert!(!valid_root(0, PN_BITS));
        assert!(!valid_root(l, PN_BITS + 1));
        assert!(valid_root(l, 4));
        assert!(go(&ctx, info(l, 3), 0).is_bad());
        assert!(go(&ctx, info(0, 20), 0).is_bad());
        assert!(go(&ctx, info(l, 23), 0).is_bad());
        assert!(go(&ctx, info(l, 255), 0).is_bad());
        assert!(go(&ctx, info(l, 4), 0x10).is_bad());
        assert!(!go(&ctx, info(l, 4), 0xf).is_bad());
    }

    #[test]
    fn permissions_and_size_combine_with_the_input() {
        let ctx = Ctx::new();
        let i = info(leaf_entry(12, 0, X | W | R | U, 6, false), 12);
        let input = Translation::default_for_va(0)
            .with_pn(0x123)
            .with_xwru((R | U) as u8)
            .with_size(2)
            .with_cccc(1)
            .with_weak_ccc(false)
            .with_shared(true);
        let t = varadix_translate(&ctx, input, i);
        assert_eq!(t.xwru() as u32, R | U);
        assert_eq!(t.size(), 2);
        // A strong input cache attribute stands; the result is never shared.
        assert_eq!(t.cccc(), 1);
        assert!(!t.shared());
    }

    #[test]
    fn tables_and_leaves_go_through_the_guest_map() {
        let mut ctx = Ctx::new().with_stage2();
        // Guest-physical tables, stored at their stage-2 addresses.
        let root_gpa = 0x4000u64;
        let s2 = |gpa: u64| gpa + ((STAGE2_DELTA as u64) << PAGE_BITS);
        let root = table_entry(root_gpa, 4);
        ctx.write(s2(root_gpa) + 4 * 5, leaf_entry(4, 0x300, W | R, 7, false));
        let t = go(&ctx, info(root, 8), 0x56);
        assert_eq!(t.pn(), 0x306 + STAGE2_DELTA);
        // One stage-2 lookup for the table, one for the result.
        assert_eq!(
            *ctx.stage2_calls.borrow(),
            [(root_gpa >> PAGE_BITS) as u32, 0x306]
        );

        // Without read permission on the tables, nothing translates.
        ctx.stage2_xwru = (X | W) as u8;
        assert!(go(&ctx, info(root, 8), 0x56).is_bad());
    }

    /// H2's `kernel/mem/varadix/test/test.c`: random tables of random
    /// radix, one good path through them, and every page number that
    /// differs from it above the leaf faults.
    #[test]
    fn random_tables() {
        struct Rng(u64);
        impl Rng {
            fn next(&mut self) -> u32 {
                self.0 ^= self.0 << 13;
                self.0 ^= self.0 >> 7;
                self.0 ^= self.0 << 17;
                (self.0 >> 16) as u32
            }
            fn below(&mut self, n: u32) -> u32 {
                self.next() % n
            }
        }
        let mut rng = Rng(42);
        let iterations = if cfg!(miri) { 20 } else { 5000 };

        for _ in 0..iterations {
            let mut ctx = Ctx::new();
            let vpn_bits = rng.below(PN_BITS + 1);
            // Table radices, root first, mostly small.
            let mut radices = Vec::new();
            let mut left = vpn_bits;
            while left > 0 {
                let bits = if left < 3 || rng.below(8) == 0 {
                    1 + rng.below(left)
                } else {
                    1 + (rng.next() & 7).min(left - 1)
                };
                // Sometimes stop early with a bigger leaf.
                if !radices.is_empty() && rng.below(4) == 0 {
                    break;
                }
                radices.push(bits);
                left -= bits;
            }
            let leaf_k = left;
            let xwru = 2 + rng.below(14);
            let cccc = rng.below(8);
            let weak = rng.below(2) == 1;
            let ppn = rng.next() & ((1 << PN_BITS) - 1) & (!0u32 << leaf_k);
            let lf = leaf_entry(leaf_k, ppn, xwru, cccc, weak);

            // Lay the tables out from the bottom of a sparse memory, each
            // aligned to its size, along one random path.
            let mut good_vpn = 0u32;
            let mut next_pa = 0u64;
            let mut pointer = lf;
            let mut startbit = leaf_k;
            for &bits in radices.iter().rev() {
                let align = 4u64 << bits;
                next_pa = (next_pa + align - 1) & !(align - 1);
                let pa = next_pa;
                next_pa += align;
                let idx = rng.below(1 << bits);
                ctx.write(pa + 4 * idx as u64, pointer);
                good_vpn |= idx << startbit;
                startbit += bits;
                pointer = table_entry(pa, bits);
            }
            assert_eq!(startbit, vpn_bits);
            let i = info(pointer, vpn_bits as u8);

            for off in 0..(1u32 << leaf_k).min(64) {
                let t = go(&ctx, i, good_vpn | off);
                assert!(!t.is_bad(), "vpn_bits={vpn_bits} radices={radices:?}");
                assert_eq!(t.pn(), ppn | off);
                assert_eq!(t.xwru() as u32, xwru);
                assert_eq!(t.cccc() as u32, cccc);
                assert_eq!(t.weak_ccc(), weak);
                assert_eq!(t.size() as u32, leaf_k.min(12) / 2);
            }
            for bit in leaf_k..vpn_bits {
                assert!(go(&ctx, i, good_vpn ^ (1 << bit)).is_bad());
            }
            assert!(go(&ctx, i, 1 << vpn_bits).is_bad());
        }
    }
}
