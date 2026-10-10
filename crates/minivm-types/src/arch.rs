/*
 * Copyright (c) Qualcomm Technologies, Inc. and/or its subsidiaries.
 * SPDX-License-Identifier: BSD-3-Clause-Clear
 */

//! Architecture version abstraction and feature detection.

/// Hexagon architecture version.
///
/// Each variant maps to the ARCHV build variable from the C implementation.
/// Intermediate versions are aliased to their base (e.g., v66/v67 → V65).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u8)]
pub enum ArchVersion {
    V65 = 65,
    V68 = 68,
    V73 = 73,
    V81 = 81,
}

impl ArchVersion {
    /// Map a raw core revision byte to the canonical architecture version.
    ///
    /// Returns `None` for unsupported versions.
    pub const fn from_rev(rev: u8) -> Option<Self> {
        match rev {
            0x65..=0x67 => Some(Self::V65),
            0x68..=0x72 => Some(Self::V68),
            0x73 | 0x75 | 0x77 | 0x79 => Some(Self::V73),
            0x81 | 0x83 | 0x85 | 0x87 | 0x89 | 0x91 => Some(Self::V81),
            _ => None,
        }
    }

    /// Whether this architecture version has HVX/coprocessor extensions.
    pub const fn has_extensions(self) -> bool {
        true // All v60+ have extensions
    }

    /// Whether the architecture has the DM0 register (v68+).
    pub const fn has_dm0(self) -> bool {
        matches!(self, Self::V68 | Self::V73 | Self::V81)
    }

    /// Whether the architecture has the VWCTRL register (v73+).
    pub const fn has_vwctrl(self) -> bool {
        matches!(self, Self::V73 | Self::V81)
    }

    /// Maximum page size supported.
    pub const fn max_page_size(self) -> crate::pmap::PageSize {
        match self {
            Self::V65 | Self::V68 => crate::pmap::PageSize::Size16M,
            Self::V73 | Self::V81 => crate::pmap::PageSize::Size1G,
        }
    }

    /// Maximum hardware threads.
    pub const fn max_hthreads(self) -> u32 {
        16 // All v60+ support up to 16
    }

    /// ASID bit width.
    pub const fn asid_bits(self) -> u32 {
        7 // All v60+ use 7-bit ASIDs
    }

    /// Number of ASID entries (2^asid_bits).
    pub const fn max_asids(self) -> u32 {
        1 << self.asid_bits()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_from_rev() {
        assert_eq!(ArchVersion::from_rev(0x65), Some(ArchVersion::V65));
        assert_eq!(ArchVersion::from_rev(0x66), Some(ArchVersion::V65));
        assert_eq!(ArchVersion::from_rev(0x67), Some(ArchVersion::V65));
        assert_eq!(ArchVersion::from_rev(0x68), Some(ArchVersion::V68));
        assert_eq!(ArchVersion::from_rev(0x71), Some(ArchVersion::V68));
        assert_eq!(ArchVersion::from_rev(0x72), Some(ArchVersion::V68));
        assert_eq!(ArchVersion::from_rev(0x73), Some(ArchVersion::V73));
        assert_eq!(ArchVersion::from_rev(0x79), Some(ArchVersion::V73));
        assert_eq!(ArchVersion::from_rev(0x81), Some(ArchVersion::V81));
        assert_eq!(ArchVersion::from_rev(0x83), Some(ArchVersion::V81));
        assert_eq!(ArchVersion::from_rev(0x91), Some(ArchVersion::V81));
        assert_eq!(ArchVersion::from_rev(0x04), None);
    }

    #[test]
    fn test_ordering() {
        assert!(ArchVersion::V65 < ArchVersion::V68);
        assert!(ArchVersion::V68 < ArchVersion::V73);
        assert!(ArchVersion::V73 < ArchVersion::V81);
    }

    #[test]
    fn test_features() {
        assert!(!ArchVersion::V65.has_dm0());
        assert!(ArchVersion::V68.has_dm0());
        assert!(!ArchVersion::V65.has_vwctrl());
        assert!(!ArchVersion::V68.has_vwctrl());
        assert!(ArchVersion::V73.has_vwctrl());
        assert!(ArchVersion::V81.has_vwctrl());
    }
}
