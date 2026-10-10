/*
 * Copyright (c) Qualcomm Technologies, Inc. and/or its subsidiaries.
 * SPDX-License-Identifier: BSD-3-Clause-Clear
 */

//! Translation result type.
//!

use crate::pmap::PAGE_BITS;

/// Translation result (64-bit).
///
/// Layout:
///   Low 32 bits: `pn` (page number)
///   High 32 bits: `size:4 | xwru:4 | cccc:3 | weak_ccc:1 | shared:1 | unused:19`
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[repr(transparent)]
pub struct Translation(pub u64);

impl Translation {
    pub const BAD: Self = Self(0);

    pub const fn is_bad(self) -> bool {
        self.0 == 0
    }

    // Field accessors
    pub const fn pn(self) -> u32 {
        self.0 as u32
    }
    pub const fn size(self) -> u8 {
        ((self.0 >> 32) & 0xF) as u8
    }
    pub const fn xwru(self) -> u8 {
        ((self.0 >> 36) & 0xF) as u8
    }
    pub const fn cccc(self) -> u8 {
        ((self.0 >> 40) & 0x7) as u8
    }
    pub const fn weak_ccc(self) -> bool {
        (self.0 >> 43) & 1 != 0
    }
    pub const fn shared(self) -> bool {
        (self.0 >> 44) & 1 != 0
    }

    // Field setters (builder pattern)
    pub const fn with_pn(self, pn: u32) -> Self {
        Self((self.0 & !0xFFFF_FFFFu64) | pn as u64)
    }

    pub const fn with_size(self, size: u8) -> Self {
        Self((self.0 & !(0xFu64 << 32)) | ((size as u64 & 0xF) << 32))
    }

    pub const fn with_xwru(self, xwru: u8) -> Self {
        Self((self.0 & !(0xFu64 << 36)) | ((xwru as u64 & 0xF) << 36))
    }

    pub const fn with_cccc(self, cccc: u8) -> Self {
        Self((self.0 & !(0x7u64 << 40)) | ((cccc as u64 & 0x7) << 40))
    }

    pub const fn with_weak_ccc(self, wc: bool) -> Self {
        Self((self.0 & !(1u64 << 43)) | ((wc as u64) << 43))
    }

    pub const fn with_shared(self, s: bool) -> Self {
        Self((self.0 & !(1u64 << 44)) | ((s as u64) << 44))
    }

    /// Create a default translation for a virtual address.
    ///
    /// Starts with maximum size, all permissions, weak cache, not shared.
    pub const fn default_for_va(va: u32) -> Self {
        let pn = va >> PAGE_BITS;
        Self(0)
            .with_pn(pn)
            .with_size(((32 - PAGE_BITS) / 2) as u8)
            .with_xwru(0xF)
            .with_cccc(0)
            .with_weak_ccc(true)
            .with_shared(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_translation_default() {
        let t = Translation::default_for_va(0x1000_0000);
        assert_eq!(t.pn(), 0x1000_0000 >> 12);
        assert_eq!(t.size(), 10); // (32-12)/2
        assert_eq!(t.xwru(), 0xF);
        assert_eq!(t.cccc(), 0);
        assert!(t.weak_ccc());
        assert!(!t.shared());
        assert!(!t.is_bad());
    }

    #[test]
    fn test_translation_bad() {
        let t = Translation::BAD;
        assert!(t.is_bad());
        assert_eq!(t.pn(), 0);
        assert_eq!(t.xwru(), 0);
    }

    #[test]
    fn test_translation_field_setters() {
        let t = Translation(0)
            .with_pn(0xDEAD_BEEF)
            .with_size(5)
            .with_xwru(0xA)
            .with_cccc(3)
            .with_weak_ccc(true)
            .with_shared(true);

        assert_eq!(t.pn(), 0xDEAD_BEEF);
        assert_eq!(t.size(), 5);
        assert_eq!(t.xwru(), 0xA);
        assert_eq!(t.cccc(), 3);
        assert!(t.weak_ccc());
        assert!(t.shared());
    }

    #[test]
    fn test_translation_fields_independent() {
        // Verify setting one field doesn't clobber another
        let t = Translation(0)
            .with_pn(0xFFFF_FFFF)
            .with_size(0xF)
            .with_xwru(0xF)
            .with_cccc(0x7)
            .with_weak_ccc(true)
            .with_shared(true);

        let t2 = t.with_pn(0);
        assert_eq!(t2.pn(), 0);
        assert_eq!(t2.size(), 0xF);
        assert_eq!(t2.xwru(), 0xF);
        assert_eq!(t2.cccc(), 0x7);
        assert!(t2.weak_ccc());
        assert!(t2.shared());
    }
}
