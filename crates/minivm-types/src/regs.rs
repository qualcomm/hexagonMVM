/*
 * Copyright (c) Qualcomm Technologies, Inc. and/or its subsidiaries.
 * SPDX-License-Identifier: BSD-3-Clause-Clear
 */

//! SSR, CCR, and SYSCFG register bitfield types.
//!

/// SSR (Supervisor Status Register) bitfield accessors.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[repr(transparent)]
pub struct Ssr(pub u32);

impl Ssr {
    // Bit positions
    pub const CAUSE_SHIFT: u32 = 0;
    pub const CAUSE_MASK: u32 = 0xFF;
    pub const ASID_SHIFT: u32 = 8;
    pub const ASID_MASK: u32 = 0x7F << 8;
    pub const UM_BIT: u32 = 16;
    pub const EX_BIT: u32 = 17;
    pub const IE_BIT: u32 = 18;
    pub const GUEST_BIT: u32 = 19;
    pub const BADVA_V0_BIT: u32 = 20;
    pub const BADVA_V1_BIT: u32 = 21;
    pub const BADVA_BVS_BIT: u32 = 22;
    pub const BADVA_CE_BIT: u32 = 23;
    pub const BADVA_PE_BIT: u32 = 24;
    pub const BP_BIT: u32 = 25;
    pub const XE2_BIT: u32 = 26;
    pub const XA_SHIFT: u32 = 27;
    pub const XA_MASK: u32 = 0x7 << 27;
    pub const SS_BIT: u32 = 30;
    pub const XE_BIT: u32 = 31;

    pub const fn new(raw: u32) -> Self {
        Self(raw)
    }
    pub const fn raw(self) -> u32 {
        self.0
    }
    pub const fn cause(self) -> u8 {
        (self.0 & Self::CAUSE_MASK) as u8
    }
    pub const fn asid(self) -> u8 {
        ((self.0 & Self::ASID_MASK) >> Self::ASID_SHIFT) as u8
    }
    pub const fn um(self) -> bool {
        self.0 & (1 << Self::UM_BIT) != 0
    }
    pub const fn ex(self) -> bool {
        self.0 & (1 << Self::EX_BIT) != 0
    }
    pub const fn ie(self) -> bool {
        self.0 & (1 << Self::IE_BIT) != 0
    }
    pub const fn guest(self) -> bool {
        self.0 & (1 << Self::GUEST_BIT) != 0
    }
    pub const fn ss(self) -> bool {
        self.0 & (1 << Self::SS_BIT) != 0
    }
    pub const fn xe(self) -> bool {
        self.0 & (1 << Self::XE_BIT) != 0
    }
    pub const fn xe2(self) -> bool {
        self.0 & (1 << Self::XE2_BIT) != 0
    }
    pub const fn xa(self) -> u8 {
        ((self.0 & Self::XA_MASK) >> Self::XA_SHIFT) as u8
    }

    pub const fn with_cause(self, cause: u8) -> Self {
        Self((self.0 & !Self::CAUSE_MASK) | (cause as u32))
    }

    pub const fn with_asid(self, asid: u8) -> Self {
        Self((self.0 & !Self::ASID_MASK) | (((asid as u32) & 0x7F) << Self::ASID_SHIFT))
    }

    pub const fn with_ie(self, ie: bool) -> Self {
        if ie {
            Self(self.0 | (1 << Self::IE_BIT))
        } else {
            Self(self.0 & !(1 << Self::IE_BIT))
        }
    }

    pub const fn with_guest(self, guest: bool) -> Self {
        if guest {
            Self(self.0 | (1 << Self::GUEST_BIT))
        } else {
            Self(self.0 & !(1 << Self::GUEST_BIT))
        }
    }
}

/// CCR (Cache Control Register) bitfield accessors.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[repr(transparent)]
pub struct Ccr(pub u32);

impl Ccr {
    pub const L1ICP_SHIFT: u32 = 0;
    pub const L1ICP_MASK: u32 = 0x3;
    pub const L1DCP_SHIFT: u32 = 3;
    pub const L1DCP_MASK: u32 = 0x3 << 3;
    pub const L2CP_SHIFT: u32 = 6;
    pub const L2CP_MASK: u32 = 0x3 << 6;
    pub const HFI_BIT: u32 = 16;
    pub const HFD_BIT: u32 = 17;
    pub const HFIL2_BIT: u32 = 18;
    pub const HFDL2_BIT: u32 = 19;
    pub const SFD_BIT: u32 = 20;
    pub const XA3_BIT: u32 = 21;
    pub const GIE_BIT: u32 = 24;
    pub const GTE_BIT: u32 = 25;
    pub const GEE_BIT: u32 = 26;
    pub const GRE_BIT: u32 = 27;
    pub const XE3_BIT: u32 = 28;

    pub const fn new(raw: u32) -> Self {
        Self(raw)
    }
    pub const fn raw(self) -> u32 {
        self.0
    }
    pub const fn l2cp(self) -> u8 {
        ((self.0 & Self::L2CP_MASK) >> Self::L2CP_SHIFT) as u8
    }
    pub const fn xe3(self) -> bool {
        self.0 & (1 << Self::XE3_BIT) != 0
    }
    pub const fn xa3(self) -> bool {
        self.0 & (1 << Self::XA3_BIT) != 0
    }
}

/// SYSCFG register bitfield accessors.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[repr(transparent)]
pub struct Syscfg(pub u32);

impl Syscfg {
    pub const M_BIT: u32 = 0; // MMU enable
    pub const I_BIT: u32 = 1; // I-cache enable
    pub const D_BIT: u32 = 2; // D-cache enable
    pub const T_BIT: u32 = 3; // TLB sharing
    pub const G_BIT: u32 = 4; // Guest mode enable
    pub const R_BIT: u32 = 5; // Remap
    pub const C_BIT: u32 = 6; // L2 cache
    pub const V2X_BIT: u32 = 7; // V2X mode
    pub const IDA_BIT: u32 = 8; // IDA
    pub const PM_BIT: u32 = 9; // Performance monitor
    pub const TE_BIT: u32 = 10; // Trace enable
    pub const TL_BIT: u32 = 11; // Trace lock
    pub const KL_BIT: u32 = 12; // Kernel lock
    pub const BQ_BIT: u32 = 13; // Bus quality
    pub const PRIO_BIT: u32 = 14; // Priority
    pub const DMT_BIT: u32 = 15; // DMT enable

    pub const L2CFG_SHIFT: u32 = 16;
    pub const L2CFG_MASK: u32 = 0x7 << 16;
    pub const L2CFG_MAX: u32 = 6;

    pub const fn new(raw: u32) -> Self {
        Self(raw)
    }
    pub const fn raw(self) -> u32 {
        self.0
    }

    const fn bit(self, pos: u32) -> bool {
        self.0 & (1 << pos) != 0
    }
    const fn set_bit(self, pos: u32, val: bool) -> Self {
        if val {
            Self(self.0 | (1 << pos))
        } else {
            Self(self.0 & !(1 << pos))
        }
    }

    pub const fn mmu_enabled(self) -> bool {
        self.bit(Self::M_BIT)
    }
    pub const fn with_mmu(self, en: bool) -> Self {
        self.set_bit(Self::M_BIT, en)
    }
    pub const fn icache_enabled(self) -> bool {
        self.bit(Self::I_BIT)
    }
    pub const fn dcache_enabled(self) -> bool {
        self.bit(Self::D_BIT)
    }
    pub const fn guest_enabled(self) -> bool {
        self.bit(Self::G_BIT)
    }
    pub const fn with_guest(self, en: bool) -> Self {
        self.set_bit(Self::G_BIT, en)
    }
    pub const fn dmt_enabled(self) -> bool {
        self.bit(Self::DMT_BIT)
    }
    pub const fn with_dmt(self, en: bool) -> Self {
        self.set_bit(Self::DMT_BIT, en)
    }

    pub const fn l2cfg(self) -> u32 {
        (self.0 & Self::L2CFG_MASK) >> Self::L2CFG_SHIFT
    }
}

/// Boot defaults for thread registers.
pub mod boot_defaults {
    /// Default USR for boot thread (v65+).
    pub const THREAD_USR: u32 = 0x00057c00;
    /// Default SSR for boot thread (guest bit set).
    pub const THREAD_SSR: u32 = 0x01c60000 | (1 << super::Ssr::GUEST_BIT);
    /// Default CCR for boot thread.
    pub const THREAD_CCR: u32 = 0x00170000;
    /// Default GPUGP for boot thread (UGP = 0).
    pub const THREAD_GPUGP: u64 = 0;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ssr_bitfields() {
        let ssr = Ssr::new(0);
        assert!(!ssr.ie());
        assert!(!ssr.guest());

        let ssr = ssr
            .with_ie(true)
            .with_guest(true)
            .with_cause(0x42)
            .with_asid(0x3F);
        assert!(ssr.ie());
        assert!(ssr.guest());
        assert_eq!(ssr.cause(), 0x42);
        assert_eq!(ssr.asid(), 0x3F);
    }

    #[test]
    fn test_ssr_guest_bit_position() {
        let ssr = Ssr::new(1 << 19);
        assert!(ssr.guest());
        assert!(!ssr.ie());
    }

    #[test]
    fn test_syscfg_bits() {
        let cfg = Syscfg::new(0)
            .with_mmu(true)
            .with_guest(true)
            .with_dmt(true);
        assert!(cfg.mmu_enabled());
        assert!(cfg.guest_enabled());
        assert!(cfg.dmt_enabled());
        assert_eq!(cfg.raw(), (1 << 0) | (1 << 4) | (1 << 15));
    }

    #[test]
    fn test_boot_ssr_has_guest() {
        let ssr = Ssr::new(boot_defaults::THREAD_SSR);
        assert!(ssr.guest());
    }
}
