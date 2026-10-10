/*
 * Copyright (c) Qualcomm Technologies, Inc. and/or its subsidiaries.
 * SPDX-License-Identifier: BSD-3-Clause-Clear
 */

//! ASID entry types and translation type enums.
//!

/// Translation type for ASID entries.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum TranslationType {
    Linear = 0,
    Table = 1,
    Offset = 2,
    Varadix = 3,
}

impl TranslationType {
    pub const fn from_raw(val: u8) -> Option<Self> {
        match val {
            0 => Some(Self::Linear),
            1 => Some(Self::Table),
            2 => Some(Self::Offset),
            3 => Some(Self::Varadix),
            _ => None,
        }
    }
}

/// ASID entry fields (lower 32 bits of the 64-bit ASID entry).
///
/// Layout: `count:12 | vmid:6 | type:3 | log_maxhops:3 | extra:8`
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[repr(transparent)]
pub struct AsidEntryFields(pub u32);

impl AsidEntryFields {
    // Bit widths
    pub const COUNT_BITS: u32 = 12;
    pub const VMID_BITS: u32 = 6;
    pub const TYPE_BITS: u32 = 3;
    pub const LOG_MAXHOPS_BITS: u32 = 3;
    pub const EXTRA_BITS: u32 = 8;

    // Bit positions
    pub const COUNT_POS: u32 = 0;
    pub const VMID_POS: u32 = Self::COUNT_POS + Self::COUNT_BITS;
    pub const TYPE_POS: u32 = Self::VMID_POS + Self::VMID_BITS;
    pub const LOG_MAXHOPS_POS: u32 = Self::TYPE_POS + Self::TYPE_BITS;
    pub const EXTRA_POS: u32 = Self::LOG_MAXHOPS_POS + Self::LOG_MAXHOPS_BITS;

    // Masks
    pub const COUNT_MASK: u32 = ((1 << Self::COUNT_BITS) - 1) << Self::COUNT_POS;
    pub const VMID_MASK: u32 = ((1 << Self::VMID_BITS) - 1) << Self::VMID_POS;
    pub const TYPE_MASK: u32 = ((1 << Self::TYPE_BITS) - 1) << Self::TYPE_POS;
    pub const LOG_MAXHOPS_MASK: u32 = ((1 << Self::LOG_MAXHOPS_BITS) - 1) << Self::LOG_MAXHOPS_POS;
    pub const EXTRA_MASK: u32 = ((1 << Self::EXTRA_BITS) - 1) << Self::EXTRA_POS;

    pub const fn count(self) -> u16 {
        ((self.0 & Self::COUNT_MASK) >> Self::COUNT_POS) as u16
    }
    pub const fn vmid(self) -> u8 {
        ((self.0 & Self::VMID_MASK) >> Self::VMID_POS) as u8
    }
    pub const fn trans_type(self) -> u8 {
        ((self.0 & Self::TYPE_MASK) >> Self::TYPE_POS) as u8
    }
    pub const fn log_maxhops(self) -> u8 {
        ((self.0 & Self::LOG_MAXHOPS_MASK) >> Self::LOG_MAXHOPS_POS) as u8
    }
    pub const fn extra(self) -> u8 {
        ((self.0 & Self::EXTRA_MASK) >> Self::EXTRA_POS) as u8
    }

    pub const fn with_count(self, count: u16) -> Self {
        Self(
            (self.0 & !Self::COUNT_MASK) | (((count as u32) << Self::COUNT_POS) & Self::COUNT_MASK),
        )
    }

    pub const fn with_vmid(self, vmid: u8) -> Self {
        Self((self.0 & !Self::VMID_MASK) | (((vmid as u32) << Self::VMID_POS) & Self::VMID_MASK))
    }

    pub const fn with_type(self, ty: TranslationType) -> Self {
        Self((self.0 & !Self::TYPE_MASK) | (((ty as u32) << Self::TYPE_POS) & Self::TYPE_MASK))
    }

    pub const fn with_log_maxhops(self, lmh: u8) -> Self {
        Self(
            (self.0 & !Self::LOG_MAXHOPS_MASK)
                | (((lmh as u32) << Self::LOG_MAXHOPS_POS) & Self::LOG_MAXHOPS_MASK),
        )
    }

    pub const fn with_extra(self, extra: u8) -> Self {
        Self(
            (self.0 & !Self::EXTRA_MASK) | (((extra as u32) << Self::EXTRA_POS) & Self::EXTRA_MASK),
        )
    }
}

/// Full 64-bit ASID entry.
///
/// Layout: `ptb:32 | fields:32`
///
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct AsidEntry {
    pub ptb: u32,
    pub fields: AsidEntryFields,
}

impl AsidEntry {
    pub const EMPTY: Self = Self {
        ptb: 0,
        fields: AsidEntryFields(0),
    };

    pub const fn raw(self) -> u64 {
        (self.ptb as u64) | ((self.fields.0 as u64) << 32)
    }

    pub const fn from_raw(raw: u64) -> Self {
        Self {
            ptb: raw as u32,
            fields: AsidEntryFields((raw >> 32) as u32),
        }
    }

    pub const fn count(self) -> u16 {
        self.fields.count()
    }
    pub const fn vmid(self) -> u8 {
        self.fields.vmid()
    }
    pub const fn trans_type(self) -> u8 {
        self.fields.trans_type()
    }
    pub const fn log_maxhops(self) -> u8 {
        self.fields.log_maxhops()
    }
    pub const fn extra(self) -> u8 {
        self.fields.extra()
    }

    /// Check if this entry is empty (zero).
    pub const fn is_empty(self) -> bool {
        self.ptb == 0 && self.fields.0 == 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fields_bit_layout() {
        // Verify the bits add up to 32
        assert_eq!(
            AsidEntryFields::COUNT_BITS
                + AsidEntryFields::VMID_BITS
                + AsidEntryFields::TYPE_BITS
                + AsidEntryFields::LOG_MAXHOPS_BITS
                + AsidEntryFields::EXTRA_BITS,
            32
        );
    }

    #[test]
    fn test_fields_round_trip() {
        let f = AsidEntryFields(0)
            .with_count(0xABC)
            .with_vmid(0x3F)
            .with_type(TranslationType::Offset)
            .with_log_maxhops(5)
            .with_extra(0xDE);

        assert_eq!(f.count(), 0xABC);
        assert_eq!(f.vmid(), 0x3F);
        assert_eq!(f.trans_type(), TranslationType::Offset as u8);
        assert_eq!(f.log_maxhops(), 5);
        assert_eq!(f.extra(), 0xDE);
    }

    #[test]
    fn test_asid_entry_raw_round_trip() {
        let entry = AsidEntry {
            ptb: 0xDEADBEEF,
            fields: AsidEntryFields(0x12345678),
        };
        let raw = entry.raw();
        let back = AsidEntry::from_raw(raw);
        assert_eq!(back.ptb, entry.ptb);
        assert_eq!(back.fields.0, entry.fields.0);
    }

    #[test]
    fn test_translation_type_values() {
        assert_eq!(TranslationType::Linear as u8, 0);
        assert_eq!(TranslationType::Table as u8, 1);
        assert_eq!(TranslationType::Offset as u8, 2);
        assert_eq!(TranslationType::Varadix as u8, 3);
    }
}
