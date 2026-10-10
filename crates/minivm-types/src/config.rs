/*
 * Copyright (c) Qualcomm Technologies, Inc. and/or its subsidiaries.
 * SPDX-License-Identifier: BSD-3-Clause-Clear
 */

//! VM configuration operation types.
//!

/// Top-level configuration types (trap0 CONFIG dispatch).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum ConfigType {
    VmblockInit = 0,
    StlbAlloc = 1,
    FatalHook = 2,
    ClusterSched = 3,
    Noc = 4,
}

impl ConfigType {
    pub const fn from_raw(val: u8) -> Option<Self> {
        match val {
            0 => Some(Self::VmblockInit),
            1 => Some(Self::StlbAlloc),
            2 => Some(Self::FatalHook),
            3 => Some(Self::ClusterSched),
            4 => Some(Self::Noc),
            _ => None,
        }
    }
}

/// VM block initialization sub-operations.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum VmblockInitOp {
    SetPmapType = 0,
    SetFences = 1,
    SetPrioTrapmask = 2,
    SetCpusInts = 3,
    MapPhysIntr = 4,
}

impl VmblockInitOp {
    pub const fn from_raw(val: u8) -> Option<Self> {
        match val {
            0 => Some(Self::SetPmapType),
            1 => Some(Self::SetFences),
            2 => Some(Self::SetPrioTrapmask),
            3 => Some(Self::SetCpusInts),
            4 => Some(Self::MapPhysIntr),
            _ => None,
        }
    }
}

/// Physical interrupt + CPU configuration word.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[repr(transparent)]
pub struct PhysintConfig(pub u32);

impl PhysintConfig {
    pub const CPU_BITS: u32 = 16;

    pub const fn new(physint: u16, cpuidx: u16) -> Self {
        Self(((physint as u32) << Self::CPU_BITS) | (cpuidx as u32))
    }

    pub const fn cpuidx(self) -> u16 {
        self.0 as u16
    }
    pub const fn physint(self) -> u16 {
        (self.0 >> Self::CPU_BITS) as u16
    }
}

/// Offset translation configuration.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[repr(transparent)]
pub struct OffsetConfig(pub u32);

impl OffsetConfig {
    pub const fn size(self) -> u8 {
        (self.0 & 0xF) as u8
    }
    pub const fn cccc(self) -> u8 {
        ((self.0 >> 4) & 0x7) as u8
    }
    pub const fn weak_ccc(self) -> bool {
        (self.0 >> 7) & 1 != 0
    }
    pub const fn xwru(self) -> u8 {
        ((self.0 >> 8) & 0xF) as u8
    }
    pub const fn pages(self) -> u32 {
        self.0 >> 12
    }

    pub const fn new(size: u8, cccc: u8, weak_ccc: bool, xwru: u8, pages: u32) -> Self {
        Self(
            (size as u32 & 0xF)
                | ((cccc as u32 & 0x7) << 4)
                | (if weak_ccc { 1u32 << 7 } else { 0 })
                | ((xwru as u32 & 0xF) << 8)
                | (pages << 12),
        )
    }
}

/// Extension usage flag for CONFIG_CPUS.
pub const CONFIG_USE_EXT: u32 = 0x80000000;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_physint_config() {
        let cfg = PhysintConfig::new(42, 7);
        assert_eq!(cfg.physint(), 42);
        assert_eq!(cfg.cpuidx(), 7);
    }

    #[test]
    fn test_offset_config() {
        let cfg = OffsetConfig::new(3, 7, true, 0xF, 0x1000);
        assert_eq!(cfg.size(), 3);
        assert_eq!(cfg.cccc(), 7);
        assert!(cfg.weak_ccc());
        assert_eq!(cfg.xwru(), 0xF);
        assert_eq!(cfg.pages(), 0x1000);
    }

    #[test]
    fn test_vmblock_init_op_round_trip() {
        for i in 0..=4 {
            let op = VmblockInitOp::from_raw(i).unwrap();
            assert_eq!(op as u8, i);
        }
        assert!(VmblockInitOp::from_raw(5).is_none());
    }
}
