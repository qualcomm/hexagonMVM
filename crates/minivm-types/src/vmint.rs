/*
 * Copyright (c) Qualcomm Technologies, Inc. and/or its subsidiaries.
 * SPDX-License-Identifier: BSD-3-Clause-Clear
 */

//! Virtual interrupt operation types.
//!

/// Interrupt operation type (intop dispatch).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum IntOp {
    Nop = 0,
    GlobEn = 1,
    GlobDis = 2,
    LocEn = 3,
    LocDis = 4,
    Affinity = 5,
    Get = 6,
    Peek = 7,
    Status = 8,
    Post = 9,
    Clear = 10,
}

impl IntOp {
    pub const FIRST_INVALID: u8 = 11;

    pub const fn from_raw(val: u8) -> Option<Self> {
        match val {
            0 => Some(Self::Nop),
            1 => Some(Self::GlobEn),
            2 => Some(Self::GlobDis),
            3 => Some(Self::LocEn),
            4 => Some(Self::LocDis),
            5 => Some(Self::Affinity),
            6 => Some(Self::Get),
            7 => Some(Self::Peek),
            8 => Some(Self::Status),
            9 => Some(Self::Post),
            10 => Some(Self::Clear),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_intop_values() {
        assert_eq!(IntOp::Nop as u8, 0);
        assert_eq!(IntOp::Clear as u8, 10);
        assert_eq!(IntOp::FIRST_INVALID, 11);
    }

    #[test]
    fn test_intop_round_trip() {
        for i in 0..=10 {
            let op = IntOp::from_raw(i).unwrap();
            assert_eq!(op as u8, i);
        }
        assert!(IntOp::from_raw(11).is_none());
    }
}
