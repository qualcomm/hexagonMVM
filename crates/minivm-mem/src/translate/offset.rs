/*
 * Copyright (c) Qualcomm Technologies, Inc. and/or its subsidiaries.
 * SPDX-License-Identifier: BSD-3-Clause-Clear
 */

//! Offset translation: PA = VA + page_offset.
//!
//! Offset translation is the simplest and most common type. The ASID entry's
//! PTB field encodes an `OffsetConfig` containing a page offset, permission
//! mask, cache attributes, and maximum page size.
//!
//! If the translated address falls within TCM/VTCM ranges, the offset is
//! not applied. Addresses outside the VM's fences produce a bad translation.

use minivm_types::asid::AsidEntry;
use minivm_types::config::OffsetConfig;
use minivm_types::translate::Translation;

use super::TranslateCtx;

/// Perform offset translation.
pub fn offset_translate(
    ctx: &dyn TranslateCtx,
    input: Translation,
    info: AsidEntry,
) -> Translation {
    let orig_pn = input.pn();
    let offset = OffsetConfig(info.ptb);

    // Apply offset configuration
    let mut trans = input.with_pn(input.pn().wrapping_add(offset.pages()));

    // Clamp page size to offset's maximum
    if trans.size() > offset.size() {
        trans = trans.with_size(offset.size());
    }

    // Mask permissions
    trans = trans.with_xwru(trans.xwru() & offset.xwru());

    // Override cache attributes if weak
    if trans.weak_ccc() {
        trans = trans.with_cccc(offset.cccc());
    }
    trans = trans.with_weak_ccc(offset.weak_ccc());
    // shared is unchanged for offset translations

    // Look up the parent VM's guestmap for nested translation
    let vmidx = info.vmid();
    let guestmap = ctx.vmblock_guestmap(vmidx);

    if !trans.shared() && !guestmap.is_empty() {
        // Recursive translation through guestmap
        ctx.translate(trans, guestmap)
    } else {
        // Lowest level - check TCM/VTCM and fences
        let (tcm_base, tcm_size) = ctx.tcm_range();
        let (vtcm_base, vtcm_size) = ctx.vtcm_range();

        // TCM/VTCM addresses bypass offset and fences
        if (tcm_size > 0 && orig_pn >= tcm_base && orig_pn < tcm_base + tcm_size)
            || (vtcm_size > 0 && orig_pn >= vtcm_base && orig_pn < vtcm_base + vtcm_size)
        {
            trans.with_pn(orig_pn)
        } else {
            // Check fences
            let fence_lo = ctx.vmblock_fence_lo(vmidx);
            let fence_hi = ctx.vmblock_fence_hi(vmidx);
            let page_span = 1u32 << (trans.size() as u32 * 2);

            if trans.pn() < fence_lo || trans.pn() >= fence_hi.wrapping_add(page_span) {
                Translation::BAD
            } else {
                trans
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::translate::TranslateCtx;
    use minivm_types::asid::AsidEntryFields;
    use minivm_types::asid::TranslationType;

    struct TestCtx {
        fence_lo: u32,
        fence_hi: u32,
        guestmap: AsidEntry,
        tcm_base: u32,
        tcm_size: u32,
        vtcm_base: u32,
        vtcm_size: u32,
    }

    impl TestCtx {
        fn simple(fence_lo: u32, fence_hi: u32) -> Self {
            Self {
                fence_lo,
                fence_hi,
                guestmap: AsidEntry::EMPTY,
                tcm_base: 0,
                tcm_size: 0,
                vtcm_base: 0,
                vtcm_size: 0,
            }
        }
    }

    impl TranslateCtx for TestCtx {
        fn translate(&self, input: Translation, info: AsidEntry) -> Translation {
            crate::translate::translate(self, input, info)
        }
        fn vmblock_guestmap(&self, _vmidx: u8) -> AsidEntry {
            self.guestmap
        }
        fn vmblock_fence_lo(&self, _vmidx: u8) -> u32 {
            self.fence_lo
        }
        fn vmblock_fence_hi(&self, _vmidx: u8) -> u32 {
            self.fence_hi
        }
        fn tcm_range(&self) -> (u32, u32) {
            (self.tcm_base, self.tcm_size)
        }
        fn vtcm_range(&self) -> (u32, u32) {
            (self.vtcm_base, self.vtcm_size)
        }
        fn physread_dword(&self, _pa: u64) -> u64 {
            0
        }
    }

    fn make_offset_info(
        pages: u32,
        size: u8,
        xwru: u8,
        cccc: u8,
        weak_ccc: bool,
        vmidx: u8,
    ) -> AsidEntry {
        let offset = OffsetConfig::new(size, cccc, weak_ccc, xwru, pages);
        AsidEntry {
            ptb: offset.0,
            fields: AsidEntryFields(0)
                .with_type(TranslationType::Offset)
                .with_vmid(vmidx)
                .with_count(1),
        }
    }

    #[test]
    fn test_offset_basic_translation() {
        // Offset of 0x100 pages, max size=10, all perms, weak cache
        let ctx = TestCtx::simple(0, 0xFFFFF);
        let info = make_offset_info(0x100, 10, 0xF, 7, true, 0);
        let input = Translation::default_for_va(0x0000_1000); // page 1
        let result = offset_translate(&ctx, input, info);

        assert!(!result.is_bad());
        assert_eq!(result.pn(), 1 + 0x100); // original pn + offset
    }

    #[test]
    fn test_offset_size_clamping() {
        // Offset max size=2 (64K pages), default translation has size=10
        let ctx = TestCtx::simple(0, 0xFFFFF);
        let info = make_offset_info(0, 2, 0xF, 7, true, 0);
        let input = Translation::default_for_va(0);
        let result = offset_translate(&ctx, input, info);

        assert!(!result.is_bad());
        assert_eq!(result.size(), 2); // clamped from 10 to 2
    }

    #[test]
    fn test_offset_permission_masking() {
        // Offset allows only R (0x2), input has full perms (0xF)
        let ctx = TestCtx::simple(0, 0xFFFFF);
        let info = make_offset_info(0, 10, 0x2, 7, true, 0);
        let input = Translation::default_for_va(0);
        let result = offset_translate(&ctx, input, info);

        assert!(!result.is_bad());
        assert_eq!(result.xwru(), 0x2); // masked to R only
    }

    #[test]
    fn test_offset_cache_override() {
        // Input has weak_ccc=true, so offset's cccc overrides
        let ctx = TestCtx::simple(0, 0xFFFFF);
        let info = make_offset_info(0, 10, 0xF, 5, false, 0);
        let input = Translation::default_for_va(0);
        assert!(input.weak_ccc()); // default has weak_ccc=true

        let result = offset_translate(&ctx, input, info);
        assert_eq!(result.cccc(), 5); // overridden
        assert!(!result.weak_ccc()); // no longer weak
    }

    #[test]
    fn test_offset_fence_check_pass() {
        // Address within fences should succeed
        let ctx = TestCtx::simple(0x100, 0x200);
        let info = make_offset_info(0x100, 0, 0xF, 7, true, 0);
        // Input VA page 0 + offset 0x100 = physical page 0x100
        let input = Translation::default_for_va(0);
        let result = offset_translate(&ctx, input, info);

        assert!(!result.is_bad());
        assert_eq!(result.pn(), 0x100);
    }

    #[test]
    fn test_offset_fence_check_fail_low() {
        // Address below fence_lo should fail
        let ctx = TestCtx::simple(0x200, 0x300);
        let info = make_offset_info(0, 0, 0xF, 7, true, 0);
        let input = Translation::default_for_va(0x0010_0000); // page 0x100
        let result = offset_translate(&ctx, input, info);

        assert!(result.is_bad()); // 0x100 < fence_lo=0x200
    }

    #[test]
    fn test_offset_fence_check_fail_high() {
        // Address above fence_hi should fail
        let ctx = TestCtx::simple(0, 0x100);
        // Offset puts us at page 0x200, fence_hi=0x100
        let info = make_offset_info(0x200, 0, 0xF, 7, true, 0);
        let input = Translation::default_for_va(0);
        let result = offset_translate(&ctx, input, info);

        assert!(result.is_bad()); // 0x200 >= fence_hi + 1
    }

    #[test]
    fn test_offset_tcm_bypass() {
        // TCM addresses bypass offset and fence checks
        let mut ctx = TestCtx::simple(0, 0);
        ctx.tcm_base = 0x100;
        ctx.tcm_size = 0x10;

        let info = make_offset_info(0x1000, 10, 0xF, 7, true, 0);
        // VA page 0x108 is within TCM range [0x100, 0x110)
        let input = Translation::default_for_va(0x108 << 12);
        let result = offset_translate(&ctx, input, info);

        assert!(!result.is_bad());
        assert_eq!(result.pn(), 0x108); // original pn, offset NOT applied
    }
}
