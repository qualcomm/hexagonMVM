/*
 * Copyright (c) Qualcomm Technologies, Inc. and/or its subsidiaries.
 * SPDX-License-Identifier: BSD-3-Clause-Clear
 */

//! Page-table (`TranslationType::Table`) translation.
//!
//! This is the translation a guest selects with `vmnewmap(ptb, TABLE)`,
//! and the one Linux uses. The table is rooted at the ASID entry's `ptb`:
//!
//! * The L1 index is `(va >> 22) * 4`, so each L1 entry covers 4 MiB.
//! * An entry whose page size is 5 (4 MiB) or 6 (16 MiB) is a leaf.
//! * A smaller size means the entry points at an L2 table of `2^(2*(5-s))`
//!   entries, indexed by the address bits below the 4 MiB boundary. The
//!   L2 entry's own size field is ignored — the L1 entry sets the size.
//! * Size 7 is invalid.

use minivm_types::asid::AsidEntry;
use minivm_types::pmap::PAGE_BITS;
use minivm_types::translate::Translation;

use super::{physread_word, TranslateCtx};

/// A Hexagon VM page table entry.
#[derive(Clone, Copy)]
struct Pte(u32);

impl Pte {
    const INVALID: u32 = 7;

    fn size(self) -> u32 {
        self.0 & 0x7
    }
    fn shared(self) -> bool {
        (self.0 >> 3) & 1 != 0
    }
    fn user(self) -> u32 {
        (self.0 >> 5) & 1
    }
    fn ccc(self) -> u8 {
        ((self.0 >> 6) & 0x7) as u8
    }
    /// R, W, X occupy bits 9..11.
    fn xwr(self) -> u32 {
        (self.0 >> 9) & 0x7
    }
    fn ppn(self) -> u32 {
        self.0 >> 12
    }
    /// Permission nibble in `Translation` order: U | R<<1 | W<<2 | X<<3.
    fn xwru(self) -> u8 {
        ((self.xwr() << 1) | self.user()) as u8
    }
}

/// Fold a leaf PTE into the running translation.
fn update(input: Translation, pte: Pte) -> Translation {
    let size = core::cmp::min(input.size() as u32, pte.size());
    let shift = size * 2;
    // Keep the offset within the page and take the base from the PTE, so
    // a later (smaller) translation stage can still refine it.
    let mask = (1u32 << shift) - 1;
    let pn = (input.pn() & mask) | (pte.ppn() & !mask);

    let mut out = input
        .with_size(size as u8)
        .with_pn(pn)
        .with_xwru(input.xwru() & pte.xwru())
        .with_shared(pte.shared());
    if out.weak_ccc() {
        out = out.with_cccc(pte.ccc());
    }
    out.with_weak_ccc(false)
}

/// Read the L2 entry an L1 entry points at.
///
/// `table_size` is the log4 count of L2 entries and `page_size` the size
/// code the L1 entry imposes on them.
fn walk_l2(
    ctx: &dyn TranslateCtx,
    input: Translation,
    l2_base: u32,
    table_size: u32,
    page_size: u32,
    guestmap: AsidEntry,
) -> Option<Pte> {
    // Index bits sit just above the 2-bit word offset.
    let index = (input.pn() >> (2 * page_size)) & ((1u32 << (table_size * 2)) - 1);
    let mut pa = (l2_base as u64) | ((index as u64) << 2);

    if !guestmap.is_empty() {
        let tmp = ctx.translate(
            Translation::default_for_va(0).with_pn((pa >> PAGE_BITS) as u32),
            guestmap,
        );
        if tmp.xwru() & 0x2 == 0 {
            return None; // no read permission on the L2 table itself
        }
        pa = ((tmp.pn() as u64) << PAGE_BITS) | (pa & ((1 << PAGE_BITS) - 1));
    }

    let mut pte = Pte(physread_word(ctx, pa));
    // The L1 entry, not the L2 entry, determines the page size.
    pte.0 = (pte.0 & !0x7) | (page_size & 0x7);
    Some(pte)
}

/// Read the L1 entry for this address, following it to L2 if needed.
fn walk_l1(
    ctx: &dyn TranslateCtx,
    input: Translation,
    info: AsidEntry,
    guestmap: AsidEntry,
) -> Option<Pte> {
    let mut base_pn = info.ptb >> PAGE_BITS;
    if !guestmap.is_empty() {
        let tmp = ctx.translate(Translation::default_for_va(0).with_pn(base_pn), guestmap);
        if tmp.xwru() & 0x2 == 0 {
            return None;
        }
        base_pn = tmp.pn();
    }

    // L1 index: ((va >> 22) << 2) == ((pn >> 8) & 0xffc)
    let pa = ((base_pn as u64) << PAGE_BITS) | ((input.pn() >> 8) & 0xffc) as u64;
    let pte = Pte(physread_word(ctx, pa));

    let size = pte.size();
    if size == Pte::INVALID {
        return None;
    }
    if size <= 4 {
        return walk_l2(ctx, input, pte.0 & !0xF, 5 - size, size, guestmap);
    }
    Some(pte)
}

/// Perform page-table translation.
pub fn table_translate(ctx: &dyn TranslateCtx, input: Translation, info: AsidEntry) -> Translation {
    let guestmap = ctx.vmblock_guestmap(info.vmid());

    let Some(pte) = walk_l1(ctx, input, info, guestmap) else {
        return Translation::BAD;
    };

    let out = update(input, pte);
    if !out.shared() && !guestmap.is_empty() {
        ctx.translate(out, guestmap)
    } else {
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use minivm_types::asid::{AsidEntryFields, TranslationType};

    /// Backs a page table living in a flat byte array based at `base_pa`.
    struct TableCtx {
        base_pa: u64,
        mem: [u32; 4096],
    }

    impl TableCtx {
        fn new(base_pa: u64) -> Self {
            Self {
                base_pa,
                mem: [0; 4096],
            }
        }
        fn write(&mut self, pa: u64, val: u32) {
            self.mem[((pa - self.base_pa) / 4) as usize] = val;
        }
    }

    impl TranslateCtx for TableCtx {
        fn translate(&self, input: Translation, info: AsidEntry) -> Translation {
            crate::translate::translate(self, input, info)
        }
        fn vmblock_guestmap(&self, _vmidx: u8) -> AsidEntry {
            AsidEntry::EMPTY
        }
        fn vmblock_fence_lo(&self, _vmidx: u8) -> u32 {
            0
        }
        fn vmblock_fence_hi(&self, _vmidx: u8) -> u32 {
            0xFFFFF
        }
        fn tcm_range(&self) -> (u32, u32) {
            (0, 0)
        }
        fn vtcm_range(&self) -> (u32, u32) {
            (0, 0)
        }
        fn physread_dword(&self, pa: u64) -> u64 {
            let idx = ((pa - self.base_pa) / 4) as usize;
            (self.mem[idx] as u64) | ((self.mem[idx + 1] as u64) << 32)
        }
    }

    fn table_info(ptb: u32) -> AsidEntry {
        AsidEntry {
            ptb,
            fields: AsidEntryFields(0)
                .with_type(TranslationType::Table)
                .with_vmid(0)
                .with_count(1),
        }
    }

    /// PTE with a 4M page size, full permissions, cache code 7.
    fn pte_4m(ppn: u32) -> u32 {
        (ppn << 12) | (0x7 << 9) | (0x7 << 6) | (1 << 5) | 5
    }

    #[test]
    fn test_table_4m_leaf() {
        let base = 0x1000u64;
        let mut ctx = TableCtx::new(base);
        // VA 0xc0000000 -> L1 index 0x300 -> PA 0xa0000000
        ctx.write(base + 0x300 * 4, pte_4m(0xA0000));

        let result = table_translate(
            &ctx,
            Translation::default_for_va(0xC000_0000),
            table_info(base as u32),
        );

        assert!(!result.is_bad());
        assert_eq!(result.pn(), 0xA0000);
        assert_eq!(result.size(), 5);
        assert_eq!(result.xwru(), 0xF);
    }

    #[test]
    fn test_table_offset_within_4m_page_is_preserved() {
        let base = 0x1000u64;
        let mut ctx = TableCtx::new(base);
        ctx.write(base + 0x300 * 4, pte_4m(0xA0000));

        // 0x2000 bytes into the 4M page.
        let result = table_translate(
            &ctx,
            Translation::default_for_va(0xC000_2000),
            table_info(base as u32),
        );

        assert_eq!(result.pn(), 0xA0002);
    }

    #[test]
    fn test_table_invalid_entry_is_bad() {
        let base = 0x1000u64;
        let mut ctx = TableCtx::new(base);
        ctx.write(base + 0x300 * 4, 0x7); // size 7 = invalid

        let result = table_translate(
            &ctx,
            Translation::default_for_va(0xC000_0000),
            table_info(base as u32),
        );

        assert!(result.is_bad());
    }

    #[test]
    fn test_table_permissions_are_masked() {
        let base = 0x1000u64;
        let mut ctx = TableCtx::new(base);
        // Read-only, no user bit.
        ctx.write(
            base + 0x300 * 4,
            (0xA0000 << 12) | (0x1 << 9) | (0x7 << 6) | 5,
        );

        let result = table_translate(
            &ctx,
            Translation::default_for_va(0xC000_0000),
            table_info(base as u32),
        );

        assert_eq!(result.xwru(), 0x2); // R only
    }

    #[test]
    fn test_table_l2_walk() {
        let base = 0x1000u64;
        let l2 = 0x2000u32;
        let mut ctx = TableCtx::new(base);
        // L1 entry: size 4 (1M) pointing at the L2 table.
        ctx.write(base + 0x300 * 4, l2 | 4);
        // L2 index for VA 0xc0100000: (pn >> 8) & 3 == 1
        ctx.write(
            l2 as u64 + 4,
            (0xA0100 << 12) | (0x7 << 9) | (0x7 << 6) | (1 << 5),
        );

        let result = table_translate(
            &ctx,
            Translation::default_for_va(0xC010_0000),
            table_info(base as u32),
        );

        assert!(!result.is_bad());
        assert_eq!(result.size(), 4);
        assert_eq!(result.pn(), 0xA0100);
    }
}
