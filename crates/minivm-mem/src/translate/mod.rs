/*
 * Copyright (c) Qualcomm Technologies, Inc. and/or its subsidiaries.
 * SPDX-License-Identifier: BSD-3-Clause-Clear
 */

//! Address translation dispatch.
//!
//! The translation system converts virtual addresses to physical addresses
//! through a chain of ASID entries. Each entry specifies a translation type
//! (offset, linear, table, or varadix) and the dispatch function routes to
//! the appropriate translator.

pub mod linear;
pub mod offset;

use minivm_types::asid::{AsidEntry, TranslationType};
use minivm_types::translate::Translation;

/// Page fault cause codes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum PageFaultCause {
    /// TLB miss on execute (instruction fetch).
    TlbMissX = 0x20,
    /// TLB miss on read.
    TlbMissR = 0x21,
    /// TLB miss on write.
    TlbMissW = 0x22,
}

impl PageFaultCause {
    pub const fn from_raw(val: u8) -> Option<Self> {
        match val {
            0x20 => Some(Self::TlbMissX),
            0x21 => Some(Self::TlbMissR),
            0x22 => Some(Self::TlbMissW),
            _ => None,
        }
    }
}

/// Context trait for address translation.
///
/// Translation functions need access to VM state (fences, guestmaps) and
/// the ability to recursively translate through nested ASID entries.
/// This trait abstracts those dependencies for testability.
pub trait TranslateCtx {
    /// Recursively translate through a nested ASID entry.
    fn translate(&self, input: Translation, info: AsidEntry) -> Translation;

    /// Get the guestmap ASID entry for a VM.
    fn vmblock_guestmap(&self, vmidx: u8) -> AsidEntry;

    /// Get the lower fence page number for a VM.
    fn vmblock_fence_lo(&self, vmidx: u8) -> u32;

    /// Get the upper fence page number for a VM.
    fn vmblock_fence_hi(&self, vmidx: u8) -> u32;

    /// Get the TCM base page number and size in pages.
    fn tcm_range(&self) -> (u32, u32);

    /// Get the VTCM base page number and size in pages.
    fn vtcm_range(&self) -> (u32, u32);

    /// Read a 64-bit value from physical memory (for linear translation).
    fn physread_dword(&self, pa: u64) -> u64;
}

/// Dispatch translation based on the ASID entry's translation type.
pub fn translate(ctx: &dyn TranslateCtx, input: Translation, info: AsidEntry) -> Translation {
    match TranslationType::from_raw(info.trans_type()) {
        Some(TranslationType::Offset) => offset::offset_translate(ctx, input, info),
        Some(TranslationType::Linear) => linear::linear_translate(ctx, input, info),
        _ => Translation::BAD,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use minivm_types::asid::AsidEntryFields;
    use minivm_types::config::OffsetConfig;

    /// Simple test context that dispatches via our translate function.
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
        fn new() -> Self {
            Self {
                fence_lo: 0,
                fence_hi: 0xFFFFF,
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
            translate(self, input, info)
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

    #[test]
    fn test_translate_offset_dispatch() {
        let ctx = TestCtx::new();
        let offset = OffsetConfig::new(10, 7, true, 0xF, 0);
        let info = AsidEntry {
            ptb: offset.0,
            fields: AsidEntryFields(0)
                .with_type(TranslationType::Offset)
                .with_vmid(0)
                .with_count(1),
        };
        let input = Translation::default_for_va(0x1000_0000);
        let result = translate(&ctx, input, info);
        assert!(!result.is_bad());
    }

    #[test]
    fn test_translate_bad_type() {
        let ctx = TestCtx::new();
        let info = AsidEntry {
            ptb: 0,
            fields: AsidEntryFields(0xFF_FF_FF_FF), // invalid type bits
        };
        let input = Translation::default_for_va(0);
        let result = translate(&ctx, input, info);
        assert!(result.is_bad());
    }
}
