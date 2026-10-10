/*
 * Copyright (c) Qualcomm Technologies, Inc. and/or its subsidiaries.
 * SPDX-License-Identifier: BSD-3-Clause-Clear
 */

use crate::debug;
use minivm_mem::asid::AsidTable;
use minivm_types::asid::{AsidEntry, TranslationType};
use minivm_types::config::OffsetConfig;
use minivm_types::translate::Translation;
use minivm_vm::vmblock::VmBlock;
use minivm_vm::vmconfig;

pub fn run() {
    debug::write0(b"  [test] TLB fill pipeline\n\0");
    {
        let mut test_asid = AsidTable::new();
        test_asid.init();

        // Allocate ASID with 0-page offset, max size=10, all perms, L1WB_L2C
        let offset = OffsetConfig::new(10, 7, true, 0xF, 0);
        let asid = test_asid.inc(offset.0, TranslationType::Offset, false, 0, 1, |_| {});
        assert!(asid >= 0);
        let info = *test_asid.get(asid as u32);
        assert!(!info.is_empty());

        // Set up VM with fences for translation context
        let mut tlb_vm = VmBlock::new(1);
        let _r = vmconfig::set_fences(&mut tlb_vm, 0, 0xFE000);

        struct TCtx<'a> {
            vm: &'a VmBlock,
        }
        impl<'a> minivm_mem::translate::TranslateCtx for TCtx<'a> {
            fn translate(&self, input: Translation, info: AsidEntry) -> Translation {
                minivm_mem::translate::translate(self, input, info)
            }
            fn vmblock_guestmap(&self, _: u8) -> AsidEntry {
                AsidEntry::EMPTY
            }
            fn vmblock_fence_lo(&self, _: u8) -> u32 {
                self.vm.fence_lo as u32
            }
            fn vmblock_fence_hi(&self, _: u8) -> u32 {
                self.vm.fence_hi as u32
            }
            fn tcm_range(&self) -> (u32, u32) {
                (0, 0)
            }
            fn vtcm_range(&self) -> (u32, u32) {
                (0, 0)
            }
            fn physread_dword(&self, _: u64) -> u64 {
                0
            }
        }
        let tctx = TCtx { vm: &tlb_vm };

        // Test 1: Identity translation (0-offset) at VA=0x1000_0000
        let va = 0x1000_0000u32;
        let result = minivm_mem::translate::translate(&tctx, Translation::default_for_va(va), info);
        assert!(!result.is_bad());
        assert_eq!(result.pn(), va >> 12); // identity: PA == VA

        // Test 2: Format as TLB entry and verify fields
        let entry = minivm_mem::tlb_fill::tlbfmt_from_translation(result, va, asid as u32);
        assert_ne!(entry, 0);
        let hi = (entry >> 32) as u32;
        assert_eq!(hi & 0x000F_FFFF, va >> 12); // VPN
        assert_eq!((hi >> 20) & 0x7F, asid as u32); // ASID
        assert_eq!(hi >> 31, 1); // valid
        assert_eq!(((entry as u32) >> 28) & 0xF, 0xF); // all perms

        // Test 3: Offset translation (0x100 pages offset)
        let offset2 = OffsetConfig::new(10, 7, true, 0xF, 0x100);
        let asid2 = test_asid.inc(offset2.0, TranslationType::Offset, false, 0, 1, |_| {});
        let info2 = *test_asid.get(asid2 as u32);
        let va2 = 0x1000u32; // page 1
        let result2 =
            minivm_mem::translate::translate(&tctx, Translation::default_for_va(va2), info2);
        assert!(!result2.is_bad());
        assert_eq!(result2.pn(), 1 + 0x100); // offset applied

        // Test 4: Fence check — tight fences with 4K pages
        let mut tight_vm = VmBlock::new(2);
        let _r = vmconfig::set_fences(&mut tight_vm, 0, 0x100);
        let tctx2 = TCtx { vm: &tight_vm };
        // Use size=0 (4K) so page_span=1
        let offset3 = OffsetConfig::new(0, 7, true, 0xF, 0);
        let asid3 = test_asid.inc(offset3.0, TranslationType::Offset, false, 0, 2, |_| {});
        let info3 = *test_asid.get(asid3 as u32);
        // VA page 0x200 > fence_hi=0x100: should fail
        let bad_result = minivm_mem::translate::translate(
            &tctx2,
            Translation::default_for_va(0x200 << 12),
            info3,
        );
        assert!(bad_result.is_bad());
        // VA page 0x50 < fence_hi=0x100: should pass
        let ok_result = minivm_mem::translate::translate(
            &tctx2,
            Translation::default_for_va(0x50 << 12),
            info3,
        );
        assert!(!ok_result.is_bad());

        // Test 5: Full TLB fill path (mock TlbFillCtx)
        struct FillCtx<'a> {
            asid_table: &'a AsidTable,
            tctx: TCtx<'a>,
            inserted: u64,
        }
        impl<'a> minivm_mem::tlb_fill::TlbFillCtx for FillCtx<'a> {
            fn asid_entry(&self, asid: u32) -> AsidEntry {
                *self.asid_table.get(asid)
            }
            fn stlb_lookup(&self, _va: u32, _asid: u32) -> u64 {
                0
            }
            fn stlb_add(&mut self, _va: u32, _asid: u32, _entry: u64) {}
            fn tlb_insert(&mut self, entry: u64) {
                self.inserted = entry;
            }
            fn translate(&self, input: Translation, info: AsidEntry) -> Translation {
                minivm_mem::translate::translate(&self.tctx, input, info)
            }
            fn pagefault(&mut self, _va: u32) {}
        }
        let mut fill_ctx = FillCtx {
            asid_table: &test_asid,
            tctx: TCtx { vm: &tlb_vm },
            inserted: 0,
        };
        minivm_mem::tlb_fill::tlb_fill(&mut fill_ctx, 0x2000_0000, asid as u32);
        assert_ne!(fill_ctx.inserted, 0);
    }
    debug::write0(b"  [test] TLB fill pipeline OK\n\0");
}
