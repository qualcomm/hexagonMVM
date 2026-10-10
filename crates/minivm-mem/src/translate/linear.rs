/*
 * Copyright (c) Qualcomm Technologies, Inc. and/or its subsidiaries.
 * SPDX-License-Identifier: BSD-3-Clause-Clear
 */

//! Linear translation: walk a descriptor list in guest memory.
//!
//! Linear translation walks a linked list of descriptor entries stored in
//! guest physical memory. Each 64-bit entry is either a mapping (with VPN,
//! PPN, size, permissions) or a chain pointer to continue the list elsewhere.
//!
//! The descriptor list address is stored in the ASID entry's PTB field.

use minivm_types::asid::AsidEntry;
use minivm_types::linear::LinearFmt;
use minivm_types::pmap::PAGE_BITS;
use minivm_types::translate::Translation;

use super::TranslateCtx;

/// Update translation with fields from a linear descriptor entry.
fn linear_translate_update(input: Translation, entry: LinearFmt) -> Translation {
    let size = if input.size() < entry.size() {
        input.size()
    } else {
        entry.size()
    };
    let mut out = input.with_size(size).with_pn(entry.ppn());

    if out.weak_ccc() {
        out = out.with_cccc(entry.cccc());
    }
    out = out
        .with_xwru(out.xwru() & entry.xwru())
        .with_weak_ccc(entry.weak_ccc())
        .with_shared(entry.shared());
    out
}

/// Perform linear translation by walking a descriptor list.
pub fn linear_translate(
    ctx: &dyn TranslateCtx,
    input: Translation,
    info: AsidEntry,
) -> Translation {
    let vmidx = info.vmid();
    let guestmap = ctx.vmblock_guestmap(vmidx);
    let badvpn = input.pn();

    let mut list = info.ptb;
    let mut last_gpn: u32 = !0;
    let mut list_pa: u64;
    let mut list_ppn: u32 = 0;

    // Limit iterations to prevent infinite loops from cyclic descriptor lists.
    const MAX_LINEAR_HOPS: u32 = 4096;
    for _hop in 0..MAX_LINEAR_HOPS {
        let list_gpn = list >> PAGE_BITS;

        if list_gpn != last_gpn {
            // Walked over a page boundary, retranslate the list pointer
            last_gpn = list_gpn;

            if !guestmap.is_empty() {
                let mut tmp = Translation::default_for_va(0).with_pn(list_gpn);
                tmp = ctx.translate(tmp, guestmap);
                list_ppn = tmp.pn();
                if tmp.xwru() & 0x2 == 0 {
                    break; // No read permission
                }
            } else {
                list_ppn = list_gpn;
            }

            list_pa = (list_ppn as u64) << PAGE_BITS;
            list_pa |= (list & ((1 << PAGE_BITS) - 1)) as u64;
        } else {
            list_pa = (list_ppn as u64) << PAGE_BITS;
            list_pa |= (list & ((1 << PAGE_BITS) - 1)) as u64;
        }

        let raw = ctx.physread_dword(list_pa);
        let entry = LinearFmt(raw);

        if entry.is_null() {
            break;
        }

        if entry.chain() {
            // Chain entry: follow the pointer
            list = entry.low() & !7u32; // Align to 8 bytes
                                        // list_pa is recomputed at top of loop based on new list value
            continue;
        }

        // Check if this entry matches the faulting VPN
        let evpn = entry.vpn();
        let mask = !0u32 << (entry.size() as u32 * 2);
        if (evpn & mask) == (badvpn & mask) {
            // Match found - apply translation
            let out = linear_translate_update(input, entry);
            if !out.shared() && !guestmap.is_empty() {
                return ctx.translate(out, guestmap);
            }
            return out;
        }

        // Move to next entry (8 bytes per descriptor)
        list = list.wrapping_add(8);
        // list_pa is recalculated at top of loop if page boundary crossed
    }

    // No matching entry found (or hop limit reached)
    Translation::BAD
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::translate::TranslateCtx;
    use alloc::vec;
    use alloc::vec::Vec;
    use minivm_types::asid::{AsidEntryFields, TranslationType};

    /// Test context that stores descriptor entries in a flat memory array.
    struct LinearTestCtx {
        memory: Vec<u8>,
        guestmap: AsidEntry,
    }

    impl LinearTestCtx {
        fn new(mem_size: usize) -> Self {
            Self {
                memory: vec![0u8; mem_size],
                guestmap: AsidEntry::EMPTY,
            }
        }

        fn write_dword(&mut self, pa: u64, val: u64) {
            let offset = pa as usize;
            if offset + 8 <= self.memory.len() {
                self.memory[offset..offset + 8].copy_from_slice(&val.to_le_bytes());
            }
        }
    }

    impl TranslateCtx for LinearTestCtx {
        fn translate(&self, input: Translation, info: AsidEntry) -> Translation {
            crate::translate::translate(self, input, info)
        }
        fn vmblock_guestmap(&self, _vmidx: u8) -> AsidEntry {
            self.guestmap
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
            let offset = pa as usize;
            if offset + 8 <= self.memory.len() {
                u64::from_le_bytes(self.memory[offset..offset + 8].try_into().unwrap())
            } else {
                0
            }
        }
    }

    fn make_linear_info(list_addr: u32, vmidx: u8) -> AsidEntry {
        AsidEntry {
            ptb: list_addr,
            fields: AsidEntryFields(0)
                .with_type(TranslationType::Linear)
                .with_vmid(vmidx)
                .with_count(1),
        }
    }

    #[test]
    fn test_linear_empty_list() {
        let ctx = LinearTestCtx::new(4096);
        // List at physical address 0, all zeros = null entry
        let info = make_linear_info(0, 0);
        let input = Translation::default_for_va(0x1000);
        let result = linear_translate(&ctx, input, info);
        assert!(result.is_bad());
    }

    #[test]
    fn test_linear_single_entry_match() {
        let mut ctx = LinearTestCtx::new(4096);

        // Create a descriptor entry at PA 0x100 mapping VPN 0x10 -> PPN 0x20
        let entry = LinearFmt(0)
            .with_vpn(0x10)
            .with_ppn(0x20)
            .with_size(0) // 4K page
            .with_xwru(0xF)
            .with_cccc(7)
            .with_weak_ccc(true)
            .with_shared(false)
            .with_chain(false);
        ctx.write_dword(0x100, entry.0);

        let info = make_linear_info(0x100, 0);
        let input = Translation::default_for_va(0x10 << 12); // VPN 0x10
        let result = linear_translate(&ctx, input, info);

        assert!(!result.is_bad());
        assert_eq!(result.pn(), 0x20);
        assert_eq!(result.xwru(), 0xF);
    }

    #[test]
    fn test_linear_no_match() {
        let mut ctx = LinearTestCtx::new(4096);

        // Entry maps VPN 0x10
        let entry = LinearFmt(0)
            .with_vpn(0x10)
            .with_ppn(0x20)
            .with_size(0)
            .with_xwru(0xF)
            .with_cccc(7)
            .with_weak_ccc(true)
            .with_shared(false)
            .with_chain(false);
        ctx.write_dword(0x100, entry.0);
        // Null terminator
        ctx.write_dword(0x108, 0);

        let info = make_linear_info(0x100, 0);
        let input = Translation::default_for_va(0x30 << 12); // VPN 0x30, not 0x10
        let result = linear_translate(&ctx, input, info);
        assert!(result.is_bad());
    }

    #[test]
    fn test_linear_multiple_entries() {
        let mut ctx = LinearTestCtx::new(4096);

        // Entry 0: VPN 0x10 -> PPN 0x20
        let entry0 = LinearFmt(0)
            .with_vpn(0x10)
            .with_ppn(0x20)
            .with_size(0)
            .with_xwru(0xF)
            .with_cccc(7)
            .with_weak_ccc(true)
            .with_chain(false);
        ctx.write_dword(0x100, entry0.0);

        // Entry 1: VPN 0x30 -> PPN 0x40
        let entry1 = LinearFmt(0)
            .with_vpn(0x30)
            .with_ppn(0x40)
            .with_size(0)
            .with_xwru(0xA)
            .with_cccc(5)
            .with_weak_ccc(false)
            .with_chain(false);
        ctx.write_dword(0x108, entry1.0);

        // Null terminator
        ctx.write_dword(0x110, 0);

        let info = make_linear_info(0x100, 0);

        // Look up VPN 0x30 - should find second entry
        let input = Translation::default_for_va(0x30 << 12);
        let result = linear_translate(&ctx, input, info);
        assert!(!result.is_bad());
        assert_eq!(result.pn(), 0x40);
        assert_eq!(result.xwru(), 0xA); // masked: 0xF & 0xA = 0xA
    }

    #[test]
    fn test_linear_large_page_match() {
        let mut ctx = LinearTestCtx::new(4096);

        // Entry with size=1 (16K page) maps VPN range 0x10-0x13 -> PPN 0x20
        let entry = LinearFmt(0)
            .with_vpn(0x10)
            .with_ppn(0x20)
            .with_size(1) // 16K = 4 pages
            .with_xwru(0xF)
            .with_cccc(7)
            .with_weak_ccc(true)
            .with_chain(false);
        ctx.write_dword(0x100, entry.0);
        ctx.write_dword(0x108, 0); // null terminator

        let info = make_linear_info(0x100, 0);

        // VPN 0x12 should match (within the 16K page starting at 0x10)
        // mask for size=1: !0 << 2 = 0xFFFFFFFC
        // 0x10 & mask = 0x10, 0x12 & mask = 0x10 -> match
        let input = Translation::default_for_va(0x12 << 12);
        let result = linear_translate(&ctx, input, info);
        assert!(!result.is_bad());
        assert_eq!(result.pn(), 0x20);
        assert_eq!(result.size(), 1);
    }

    #[test]
    fn test_linear_permission_masking() {
        let mut ctx = LinearTestCtx::new(4096);

        // Entry allows only R+U (0x3)
        let entry = LinearFmt(0)
            .with_vpn(0x10)
            .with_ppn(0x20)
            .with_size(0)
            .with_xwru(0x3) // R+U only
            .with_cccc(7)
            .with_weak_ccc(true)
            .with_chain(false);
        ctx.write_dword(0x100, entry.0);

        let info = make_linear_info(0x100, 0);
        let input = Translation::default_for_va(0x10 << 12); // default xwru=0xF
        let result = linear_translate(&ctx, input, info);

        assert!(!result.is_bad());
        assert_eq!(result.xwru(), 0x3); // 0xF & 0x3 = 0x3
    }
}
