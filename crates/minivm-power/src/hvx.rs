/*
 * Copyright (c) Qualcomm Technologies, Inc. and/or its subsidiaries.
 * SPDX-License-Identifier: BSD-3-Clause-Clear
 */

//! HVX power sequencing.
//!
//! HVX (Hexagon Vector eXtension) power management involves
//! clock gating, reset control, IO clamping, and BHS (Battery-backed
//! Handoff SRAM) management on v65+.

use minivm_types::arch::ArchVersion;

/// HVX power states.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[repr(u8)]
pub enum HvxState {
    #[default]
    Off = 0,
    On = 1,
}

/// HVX power sequencing steps for poweron.
///
/// These describe the hardware register operations needed.
/// The actual register writes are done by the caller.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HvxPowerStep {
    /// Wait for BHS off status (v65+).
    WaitBhsOff,
    /// Enable BHS and trigger update (v65+).
    EnableBhs,
    /// Wait for BHS on status (v65+).
    WaitBhsOn,
    /// Unclamp IO and QMC memory.
    UnclampIo,
    /// Configure VTCM power on (v65+).
    VtcmPowerOn,
    /// Wait for VTCM sleep-not-retention cleared (v65+).
    WaitVtcmSnret,
    /// Wait for VTCM sleep-retention cleared (v65+).
    WaitVtcmSret,
    /// Deassert reset.
    DeassertReset,
    /// Enable clock.
    EnableClock,
}

/// Get the poweron sequence (v65+).
pub fn poweron_steps(_arch: ArchVersion) -> &'static [HvxPowerStep] {
    &[
        HvxPowerStep::WaitBhsOff,
        HvxPowerStep::EnableBhs,
        HvxPowerStep::WaitBhsOn,
        HvxPowerStep::UnclampIo,
        HvxPowerStep::VtcmPowerOn,
        HvxPowerStep::WaitVtcmSnret,
        HvxPowerStep::WaitVtcmSret,
        HvxPowerStep::DeassertReset,
        HvxPowerStep::EnableClock,
    ]
}

/// Compute the HVX state transition.
///
/// Returns the new state, or None if already in the requested state.
pub fn transition(current: HvxState, target: HvxState) -> Option<HvxState> {
    if current == target {
        None
    } else {
        Some(target)
    }
}

/// HVX timeout for hardware polling (cycles).
pub const HVX_POLL_TIMEOUT: u32 = 1000;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_state() {
        assert_eq!(HvxState::default(), HvxState::Off);
    }

    #[test]
    fn test_transition_on() {
        assert_eq!(transition(HvxState::Off, HvxState::On), Some(HvxState::On));
    }

    #[test]
    fn test_transition_off() {
        assert_eq!(transition(HvxState::On, HvxState::Off), Some(HvxState::Off));
    }

    #[test]
    fn test_transition_noop() {
        assert_eq!(transition(HvxState::On, HvxState::On), None);
        assert_eq!(transition(HvxState::Off, HvxState::Off), None);
    }

    #[test]
    fn test_poweron_steps_v65() {
        let steps = poweron_steps(ArchVersion::V65);
        assert_eq!(steps.len(), 9);
        assert_eq!(steps[0], HvxPowerStep::WaitBhsOff);
        assert_eq!(steps[8], HvxPowerStep::EnableClock);
    }

    #[test]
    fn test_poweron_steps_v68() {
        let steps = poweron_steps(ArchVersion::V68);
        assert_eq!(steps.len(), 9);
    }
}
