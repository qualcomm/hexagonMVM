/*
 * Copyright (c) Qualcomm Technologies, Inc. and/or its subsidiaries.
 * SPDX-License-Identifier: BSD-3-Clause-Clear
 */

//! Timer operation types.
//!

/// Timer trap operation types.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum TimerOp {
    GetFreq = 0,
    GetResolution = 1,
    GetTime = 2,
    GetTimeout = 3,
    SetTimeout = 4,
    DeltaTimeout = 5,
}

impl TimerOp {
    pub const fn from_raw(val: u8) -> Option<Self> {
        match val {
            0 => Some(Self::GetFreq),
            1 => Some(Self::GetResolution),
            2 => Some(Self::GetTime),
            3 => Some(Self::GetTimeout),
            4 => Some(Self::SetTimeout),
            5 => Some(Self::DeltaTimeout),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_timer_op_round_trip() {
        for i in 0..=5 {
            let op = TimerOp::from_raw(i).unwrap();
            assert_eq!(op as u8, i);
        }
        assert!(TimerOp::from_raw(6).is_none());
    }
}
