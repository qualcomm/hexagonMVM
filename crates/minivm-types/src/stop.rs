/*
 * Copyright (c) Qualcomm Technologies, Inc. and/or its subsidiaries.
 * SPDX-License-Identifier: BSD-3-Clause-Clear
 */

//! Guest stop reasons and the policy for combining SMP shutdown requests.

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
#[repr(u32)]
pub enum StopReason {
    #[default]
    None = 0,
    Poweroff = 1,
    Halt = 2,
    Restart = 3,
    MachineCheck = 4,
}

impl StopReason {
    pub const fn from_raw(raw: u32) -> Option<Self> {
        match raw {
            0 => Some(Self::None),
            1 => Some(Self::Poweroff),
            2 => Some(Self::Halt),
            3 => Some(Self::Restart),
            4 => Some(Self::MachineCheck),
            _ => None,
        }
    }

    /// Retain the strongest reason reported by any stopping vCPU.
    pub const fn merge(self, raw: u32) -> Self {
        let next = match Self::from_raw(raw) {
            Some(reason) => reason,
            None => Self::MachineCheck,
        };
        if next as u32 > self as u32 {
            next
        } else {
            self
        }
    }

    /// Convert an HVM shutdown reason to the process-style status consumed by
    /// the simulator's `stop` instruction. Halt and poweroff are successful,
    /// restart remains distinguishable to an external launcher, and machine
    /// checks represent failures.
    pub const fn exit_code(self) -> u32 {
        match self {
            Self::MachineCheck => 1,
            Self::Restart => 3,
            Self::None | Self::Poweroff | Self::Halt => 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stop_reason_values_match_the_hvm_abi() {
        for raw in 0..=4 {
            assert_eq!(StopReason::from_raw(raw).unwrap() as u32, raw);
        }
        assert_eq!(StopReason::from_raw(5), None);
    }

    #[test]
    fn merge_keeps_the_strongest_reason_and_treats_unknown_as_failure() {
        let reason = StopReason::None
            .merge(StopReason::Poweroff as u32)
            .merge(StopReason::Halt as u32)
            .merge(StopReason::None as u32);
        assert_eq!(reason, StopReason::Halt);
        assert_eq!(reason.merge(99), StopReason::MachineCheck);
    }

    #[test]
    fn exit_code_distinguishes_restart_and_failure_from_clean_shutdown() {
        assert_eq!(StopReason::None.exit_code(), 0);
        assert_eq!(StopReason::Poweroff.exit_code(), 0);
        assert_eq!(StopReason::Halt.exit_code(), 0);
        assert_eq!(StopReason::Restart.exit_code(), 3);
        assert_eq!(StopReason::MachineCheck.exit_code(), 1);
    }
}
