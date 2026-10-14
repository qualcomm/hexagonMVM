/*
 * Copyright (c) Qualcomm Technologies, Inc. and/or its subsidiaries.
 * SPDX-License-Identifier: BSD-3-Clause-Clear
 */

//! Linear translation descriptor format.
//!

/// Linear descriptor entry (64-bit).
///
/// Layout:
///   Low 32 bits: `ppn:24 | cccc:3 | weak_ccc:1 | xwru:4`
///   High 32 bits: `vpn:20 | size:4 | unused:6 | shared:1 | chain:1`
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[repr(transparent)]
pub struct LinearFmt(pub u64);

impl LinearFmt {
    // Low word field accessors
    pub const fn low(self) -> u32 {
        self.0 as u32
    }
    pub const fn high(self) -> u32 {
        (self.0 >> 32) as u32
    }

    pub const fn ppn(self) -> u32 {
        self.low() & 0x00FF_FFFF
    }
    pub const fn cccc(self) -> u8 {
        ((self.low() >> 24) & 0x7) as u8
    }
    pub const fn weak_ccc(self) -> bool {
        (self.low() >> 27) & 1 != 0
    }
    pub const fn xwru(self) -> u8 {
        ((self.low() >> 28) & 0xF) as u8
    }

    // High word field accessors
    pub const fn vpn(self) -> u32 {
        self.high() & 0x000F_FFFF
    }
    pub const fn size(self) -> u8 {
        ((self.high() >> 20) & 0xF) as u8
    }
    pub const fn shared(self) -> bool {
        (self.high() >> 30) & 1 != 0
    }
    pub const fn chain(self) -> bool {
        (self.high() >> 31) & 1 != 0
    }

    pub const fn is_null(self) -> bool {
        self.0 == 0
    }

    // Builder methods
    pub const fn with_ppn(self, ppn: u32) -> Self {
        let lo = (self.low() & !0x00FF_FFFF) | (ppn & 0x00FF_FFFF);
        Self((self.0 & !0xFFFF_FFFFu64) | lo as u64)
    }

    pub const fn with_cccc(self, cccc: u8) -> Self {
        let lo = (self.low() & !(0x7 << 24)) | (((cccc as u32) & 0x7) << 24);
        Self((self.0 & !0xFFFF_FFFFu64) | lo as u64)
    }

    pub const fn with_weak_ccc(self, wc: bool) -> Self {
        let lo = (self.low() & !(1 << 27)) | ((wc as u32) << 27);
        Self((self.0 & !0xFFFF_FFFFu64) | lo as u64)
    }

    pub const fn with_xwru(self, xwru: u8) -> Self {
        let lo = (self.low() & !(0xF << 28)) | (((xwru as u32) & 0xF) << 28);
        Self((self.0 & !0xFFFF_FFFFu64) | lo as u64)
    }

    pub const fn with_vpn(self, vpn: u32) -> Self {
        let hi = (self.high() & !0x000F_FFFF) | (vpn & 0x000F_FFFF);
        Self((self.0 & 0xFFFF_FFFFu64) | ((hi as u64) << 32))
    }

    pub const fn with_size(self, size: u8) -> Self {
        let hi = (self.high() & !(0xF << 20)) | (((size as u32) & 0xF) << 20);
        Self((self.0 & 0xFFFF_FFFFu64) | ((hi as u64) << 32))
    }

    pub const fn with_shared(self, s: bool) -> Self {
        let hi = (self.high() & !(1 << 30)) | ((s as u32) << 30);
        Self((self.0 & 0xFFFF_FFFFu64) | ((hi as u64) << 32))
    }

    pub const fn with_chain(self, c: bool) -> Self {
        let hi = (self.high() & !(1u32 << 31)) | ((c as u32) << 31);
        Self((self.0 & 0xFFFF_FFFFu64) | ((hi as u64) << 32))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_linear_fmt_fields() {
        let entry = LinearFmt(0)
            .with_ppn(0x123456)
            .with_cccc(5)
            .with_weak_ccc(true)
            .with_xwru(0xA)
            .with_vpn(0xABCDE)
            .with_size(3)
            .with_shared(true)
            .with_chain(false);

        assert_eq!(entry.ppn(), 0x123456);
        assert_eq!(entry.cccc(), 5);
        assert!(entry.weak_ccc());
        assert_eq!(entry.xwru(), 0xA);
        assert_eq!(entry.vpn(), 0xABCDE);
        assert_eq!(entry.size(), 3);
        assert!(entry.shared());
        assert!(!entry.chain());
    }

    #[test]
    fn test_linear_fmt_chain() {
        let entry = LinearFmt(0).with_chain(true);
        assert!(entry.chain());
    }

    #[test]
    fn test_linear_fmt_null() {
        assert!(LinearFmt(0).is_null());
        assert!(!LinearFmt(1).is_null());
    }
}
