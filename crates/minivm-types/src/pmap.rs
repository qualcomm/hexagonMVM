/*
 * Copyright (c) Qualcomm Technologies, Inc. and/or its subsidiaries.
 * SPDX-License-Identifier: BSD-3-Clause-Clear
 */

//! Page sizes, cache attributes, and permission bits.
//!

/// Minimum page size in bits (4K = 12 bits).
pub const PAGE_BITS: u32 = 12;

/// Minimum page size in bytes (4K).
pub const PAGE_SIZE: u32 = 1 << PAGE_BITS;

/// Page size encoding (each step is 4x larger).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u8)]
pub enum PageSize {
    Size4K = 0,
    Size16K = 1,
    Size64K = 2,
    Size256K = 3,
    Size1M = 4,
    Size4M = 5,
    Size16M = 6,
    Size64M = 7,
    Size256M = 8,
    Size1G = 9,
}

impl PageSize {
    /// Convert from raw encoding.
    pub const fn from_raw(val: u8) -> Option<Self> {
        match val {
            0 => Some(Self::Size4K),
            1 => Some(Self::Size16K),
            2 => Some(Self::Size64K),
            3 => Some(Self::Size256K),
            4 => Some(Self::Size1M),
            5 => Some(Self::Size4M),
            6 => Some(Self::Size16M),
            7 => Some(Self::Size64M),
            8 => Some(Self::Size256M),
            9 => Some(Self::Size1G),
            _ => None,
        }
    }

    /// Size in bytes for this page size.
    pub const fn bytes(self) -> u64 {
        1u64 << (PAGE_BITS + (self as u32) * 2)
    }

    /// Number of bits in the page offset.
    pub const fn offset_bits(self) -> u32 {
        PAGE_BITS + (self as u32) * 2
    }
}

/// Cache attribute encodings.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum CacheAttr {
    L1WB_L2UC = 0,
    L1WT_L2UC = 1,
    DeviceTypeSfc = 2,
    UncachedSfc = 3,
    DeviceType = 4,
    L1WT_L2C = 5,
    Uncached = 6,
    L1WB_L2C = 7,
    L1WB_L2CWT = 8,
    L1WT_L2CWB = 9,
    L1WB_L2CWB_AUX = 0xa,
    L1WT_L2CWT_AUX = 0xb,
    L1UC_L2CWT = 0xd,
    L1UC_L2CWB = 0xf,
}

/// Permission bits.
pub mod perm {
    pub const U: u8 = 1; // User
    pub const R: u8 = 2; // Read
    pub const W: u8 = 4; // Write
    pub const X: u8 = 8; // Execute

    pub const RW: u8 = R | W;
    pub const RX: u8 = R | X;
    pub const WX: u8 = W | X;
    pub const RWX: u8 = R | W | X;
    pub const UR: u8 = U | R;
    pub const UW: u8 = U | W;
    pub const UX: u8 = U | X;
    pub const URW: u8 = U | R | W;
    pub const URX: u8 = U | R | X;
    pub const UWX: u8 = U | W | X;
    pub const URWX: u8 = U | R | W | X;
    pub const NONE: u8 = 0;
}

/// Memory map entry.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MemoryMapEntry {
    pub raw: u64,
}

impl MemoryMapEntry {
    /// Construct a memory map entry matching the C MEMORY_MAP2 macro.
    pub const fn new(
        vpn: u32,
        perm: u8,
        cfield: u8,
        pgsize: PageSize,
        ppn: u32,
        shared: bool,
    ) -> Self {
        let lo = ppn | ((cfield as u32) << 24) | ((perm as u32) << 28);
        let hi = vpn | (((pgsize as u32) & 0xF) << 20) | (if shared { 1u32 << 30 } else { 0 });
        Self {
            raw: ((hi as u64) << 32) | (lo as u64),
        }
    }

    pub const fn vpn(self) -> u32 {
        (self.raw >> 32) as u32 & 0x000FFFFF
    }
    pub const fn ppn(self) -> u32 {
        self.raw as u32 & 0x00FFFFFF
    }
    pub const fn perm(self) -> u8 {
        ((self.raw >> 28) & 0xF) as u8
    }
    pub const fn cache_attr(self) -> u8 {
        ((self.raw >> 24) & 0xF) as u8
    }
    pub const fn page_size(self) -> u8 {
        ((self.raw >> 52) & 0xF) as u8
    }
    pub const fn shared(self) -> bool {
        (self.raw >> 62) & 1 != 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_page_sizes() {
        assert_eq!(PageSize::Size4K.bytes(), 4096);
        assert_eq!(PageSize::Size16K.bytes(), 16384);
        assert_eq!(PageSize::Size64K.bytes(), 65536);
        assert_eq!(PageSize::Size256K.bytes(), 262144);
        assert_eq!(PageSize::Size1M.bytes(), 1048576);
        assert_eq!(PageSize::Size1G.bytes(), 1073741824);
    }

    #[test]
    fn test_page_size_round_trip() {
        for i in 0..=9 {
            let ps = PageSize::from_raw(i).unwrap();
            assert_eq!(ps as u8, i);
        }
        assert!(PageSize::from_raw(10).is_none());
    }

    #[test]
    fn test_memory_map_entry() {
        let entry = MemoryMapEntry::new(0x100, perm::URW, 7, PageSize::Size4K, 0x200, false);
        assert_eq!(entry.vpn(), 0x100);
        assert_eq!(entry.ppn(), 0x200);
        assert_eq!(entry.perm(), perm::URW);
        assert_eq!(entry.cache_attr(), 7);
        assert_eq!(entry.page_size(), 0);
        assert!(!entry.shared());
    }
}
