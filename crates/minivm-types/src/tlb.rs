/*
 * Copyright (c) Qualcomm Technologies, Inc. and/or its subsidiaries.
 * SPDX-License-Identifier: BSD-3-Clause-Clear
 */

//! The hardware-TLB entry representation shared by the monitor and the
//! translation library.

use crate::pmap::PAGE_BITS;
use crate::translate::Translation;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[repr(transparent)]
pub struct TlbEntry(pub u64);

impl TlbEntry {
    pub const fn invalid() -> Self {
        Self(0)
    }
    pub const fn is_valid(self) -> bool {
        self.0 >> 63 != 0
    }
    pub const fn is_global(self) -> bool {
        self.0 & (1u64 << 62) != 0
    }
    pub const fn asid(self) -> u8 {
        ((self.0 >> 52) & 0x7f) as u8
    }
    pub const fn size(self) -> u8 {
        (self.0 as u32).trailing_zeros() as u8
    }
    pub const fn permissions(self) -> u8 {
        ((self.0 >> 28) & 0xf) as u8
    }
    pub const fn cache(self) -> u8 {
        ((self.0 >> 24) & 0x7) as u8
    }

    pub const fn base_pa(self) -> u64 {
        let lo = self.0 as u32;
        let hi = (self.0 >> 32) as u32;
        let mut ppd = (lo & 0x00ff_ffff) as u64;
        ppd |= (((hi >> 29) & 1) as u64) << (35 - PAGE_BITS + 1);
        ppd |= (((hi >> 27) & 3) as u64) << (36 - PAGE_BITS + 1);
        ppd &= ppd.wrapping_sub(1);
        ppd << (PAGE_BITS - 1)
    }

    /// A global monitor mapping. Addresses must be aligned to `size`.
    pub const fn global_mapping(va: u32, pa: u32, size: u8, permissions: u8, cache: u8) -> Self {
        let ppn = (pa >> PAGE_BITS) & (!0u32 << size);
        let ppd = (ppn << 1) | (1u32 << size);
        let lo = (ppd & 0x00ff_ffff)
            | (((cache as u32) & 0x7) << 24)
            | (((permissions as u32) & 0xf) << 28);
        let hi = ((va >> PAGE_BITS) & 0x000f_ffff) | (1 << 30) | (1 << 31);
        Self(((hi as u64) << 32) | lo as u64)
    }

    /// Format a translated guest mapping. `max_size` is the running core's
    /// architectural maximum rather than the format's largest value.
    pub const fn from_translation(
        trans: Translation,
        va: u32,
        asid: u8,
        max_size: u8,
        extended_pa: bool,
    ) -> Self {
        if trans.xwru() & 0b1110 == 0 {
            return Self::invalid();
        }
        let size = if trans.size() > max_size {
            max_size
        } else {
            trans.size()
        };
        let ppn = trans.pn() & (!0u32 << size);
        let ppd = (ppn << 1) | (1u32 << size);
        let lo = (ppd & 0x00ff_ffff)
            | (((trans.cccc() as u32) & 0x7) << 24)
            | (((trans.xwru() as u32) & 0xf) << 28);
        let mut hi = ((va >> PAGE_BITS) & 0x000f_ffff) | (((asid as u32) & 0x7f) << 20) | (1 << 31);
        hi |= ((trans.pn() >> (35 - PAGE_BITS)) & 1) << 29;
        if extended_pa {
            hi |= ((trans.pn() >> (36 - PAGE_BITS)) & 3) << 27;
        }
        Self(((hi as u64) << 32) | lo as u64)
    }
}

/// Physical byte range covered by a translation after clamping it to the
/// running core's maximum page size.
pub const fn mapped_phys_range(trans: Translation, max_size: u8) -> (u64, u64) {
    let size = if trans.size() > max_size {
        max_size
    } else {
        trans.size()
    };
    let pages = 1u64 << (2 * size as u32);
    let base_page = (trans.pn() as u64) & !(pages - 1);
    let start = base_page << PAGE_BITS;
    (start, start + (pages << PAGE_BITS))
}

pub const fn overlaps(trans: Translation, max_size: u8, protected: (u64, u64)) -> bool {
    let mapped = mapped_phys_range(trans, max_size);
    mapped.0 < protected.1 && protected.0 < mapped.1
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn global_mapping_round_trip() {
        let entry = TlbEntry::global_mapping(0xff00_0000, 0x8800_0000, 5, 0xf, 7);
        assert!(entry.is_valid());
        assert!(entry.is_global());
        assert_eq!(entry.size(), 5);
        assert_eq!(entry.permissions(), 0xf);
        assert_eq!(entry.cache(), 7);
        assert_eq!(entry.base_pa(), 0x8800_0000);
    }

    #[test]
    fn translation_is_clamped_to_core_limit() {
        let trans = Translation::default_for_va(0).with_pn(0x80000);
        let entry = TlbEntry::from_translation(trans, 0, 3, 6, false);
        assert_eq!(entry.size(), 6);
        assert_eq!(entry.asid(), 3);
    }

    #[test]
    fn overlap_checks_the_whole_large_page() {
        let trans = Translation::default_for_va(0).with_pn(0x88f00).with_size(5);
        assert!(overlaps(trans, 6, (0x88f0_0000, 0x8900_0000)));
        assert!(!overlaps(trans, 6, (0x9000_0000, 0x9010_0000)));
    }
}
