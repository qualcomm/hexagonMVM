/*
 * Copyright (c) Qualcomm Technologies, Inc. and/or its subsidiaries.
 * SPDX-License-Identifier: BSD-3-Clause-Clear
 */

//! INFO query types, boot flags, and STLB configuration.
//!

/// Information query type for the INFO trap.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum InfoType {
    BuildId = 0,
    BootFlags = 1,
    Stlb = 2,
    Syscfg = 3,
    Livelock = 4,
    Rev = 5,
    SsBase = 6,
    TlbFree = 7,
    TlbSize = 8,
    PhysAddr = 9,
    TcmBase = 10,
    L2MemSize = 11,
    TcmSize = 12,
    KernelPgSize = 13,
    KernelNPages = 14,
    L2VicBase = 15,
    TimerBase = 16,
    TimerInt = 17,
    Error = 18,
    HThreads = 19,
    L2TagSize = 20,
    L2CfgBase = 21,
    CladeBase = 22,
    CfgBase = 23,
    HvxVLength = 24,
    CoprocContexts = 25,
    HvxSwitch = 26,
    VtcmBase = 27,
    VtcmSize = 28,
    EccBase = 29,
    L2LineSz = 30,
    AudioExt = 31,
    VtcmBankWidth = 32,
    L1dSize = 33,
    MaxClusterCoproc = 34,
    _Reserved35 = 35,
    CorecfgBase = 36,
    _Reserved37 = 37,
    UnitStart = 38,
    UnitEntry = 39,
    CoreId = 40,
    CoreCount = 41,
    Shift = 42,
    TcmOffset = 43,
    NocMBase = 44,
    NocSBase = 45,
}

/// Max valid info type value.
pub const INFO_MAX: u32 = 46;

impl InfoType {
    pub const fn from_raw(val: u32) -> Option<Self> {
        match val {
            0 => Some(Self::BuildId),
            1 => Some(Self::BootFlags),
            2 => Some(Self::Stlb),
            3 => Some(Self::Syscfg),
            4 => Some(Self::Livelock),
            5 => Some(Self::Rev),
            6 => Some(Self::SsBase),
            7 => Some(Self::TlbFree),
            8 => Some(Self::TlbSize),
            9 => Some(Self::PhysAddr),
            10 => Some(Self::TcmBase),
            11 => Some(Self::L2MemSize),
            12 => Some(Self::TcmSize),
            13 => Some(Self::KernelPgSize),
            14 => Some(Self::KernelNPages),
            15 => Some(Self::L2VicBase),
            16 => Some(Self::TimerBase),
            17 => Some(Self::TimerInt),
            18 => Some(Self::Error),
            19 => Some(Self::HThreads),
            20 => Some(Self::L2TagSize),
            21 => Some(Self::L2CfgBase),
            22 => Some(Self::CladeBase),
            23 => Some(Self::CfgBase),
            24 => Some(Self::HvxVLength),
            25 => Some(Self::CoprocContexts),
            26 => Some(Self::HvxSwitch),
            27 => Some(Self::VtcmBase),
            28 => Some(Self::VtcmSize),
            29 => Some(Self::EccBase),
            30 => Some(Self::L2LineSz),
            31 => Some(Self::AudioExt),
            32 => Some(Self::VtcmBankWidth),
            33 => Some(Self::L1dSize),
            34 => Some(Self::MaxClusterCoproc),
            36 => Some(Self::CorecfgBase),
            38 => Some(Self::UnitStart),
            39 => Some(Self::UnitEntry),
            40 => Some(Self::CoreId),
            41 => Some(Self::CoreCount),
            42 => Some(Self::Shift),
            43 => Some(Self::TcmOffset),
            44 => Some(Self::NocMBase),
            45 => Some(Self::NocSBase),
            _ => None,
        }
    }
}

/// Compatibility aliases.
pub const INFO_HVX_CONTEXTS: InfoType = InfoType::CoprocContexts;
pub const INFO_MAX_CLUSTER_HVX: InfoType = InfoType::MaxClusterCoproc;

/// Boot flags (returned by INFO_BOOT_FLAGS query).
///
/// Binary-compatible with C `info_boot_flags_type`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[repr(transparent)]
pub struct BootFlags(pub u32);

impl BootFlags {
    pub const fn use_tcm(self) -> bool {
        self.0 & (1 << 0) != 0
    }
    pub const fn have_hvx(self) -> bool {
        self.0 & (1 << 1) != 0
    }
    pub const fn have_sample(self) -> bool {
        self.0 & (1 << 2) != 0
    }
    pub const fn ext_ok(self) -> bool {
        self.0 & (1 << 3) != 0
    }
    pub const fn have_dma(self) -> bool {
        self.0 & (1 << 4) != 0
    }
    pub const fn with_use_tcm(self, v: bool) -> Self {
        Self(if v {
            self.0 | (1 << 0)
        } else {
            self.0 & !(1 << 0)
        })
    }
    pub const fn with_have_hvx(self, v: bool) -> Self {
        Self(if v {
            self.0 | (1 << 1)
        } else {
            self.0 & !(1 << 1)
        })
    }
    pub const fn with_ext_ok(self, v: bool) -> Self {
        Self(if v {
            self.0 | (1 << 3)
        } else {
            self.0 & !(1 << 3)
        })
    }
}

/// STLB configuration (returned by INFO_STLB query).
///
/// Binary-compatible with C `info_stlb_type`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[repr(transparent)]
pub struct StlbInfo(pub u32);

impl StlbInfo {
    pub const fn max_sets_log2(self) -> u8 {
        (self.0 & 0xFF) as u8
    }
    pub const fn max_ways(self) -> u8 {
        ((self.0 >> 8) & 0xFF) as u8
    }
    pub const fn size(self) -> u8 {
        ((self.0 >> 16) & 0xFF) as u8
    }
    pub const fn enabled(self) -> bool {
        (self.0 >> 31) & 1 != 0
    }

    pub const fn new(sets_log2: u8, ways: u8, size: u8, enabled: bool) -> Self {
        Self(
            (sets_log2 as u32)
                | ((ways as u32) << 8)
                | ((size as u32) << 16)
                | (if enabled { 1u32 << 31 } else { 0 }),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_info_type_round_trip() {
        assert_eq!(InfoType::from_raw(0), Some(InfoType::BuildId));
        assert_eq!(InfoType::from_raw(45), Some(InfoType::NocSBase));
        assert_eq!(InfoType::from_raw(46), None);
    }

    #[test]
    fn test_boot_flags() {
        let flags = BootFlags(0)
            .with_use_tcm(true)
            .with_have_hvx(true)
            .with_ext_ok(true);
        assert!(flags.use_tcm());
        assert!(flags.have_hvx());
        assert!(!flags.have_sample());
        assert!(flags.ext_ok());
    }

    #[test]
    fn test_stlb_info() {
        let info = StlbInfo::new(11, 4, 2, true);
        assert_eq!(info.max_sets_log2(), 11);
        assert_eq!(info.max_ways(), 4);
        assert_eq!(info.size(), 2);
        assert!(info.enabled());
    }
}
