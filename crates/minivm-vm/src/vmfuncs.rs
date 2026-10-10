/*
 * Copyright (c) Qualcomm Technologies, Inc. and/or its subsidiaries.
 * SPDX-License-Identifier: BSD-3-Clause-Clear
 */

//! VM trap handler implementations.
//!
//! Each VM trap (trap1) maps to a handler function. The pure logic
//! of each handler is extracted here; the actual context manipulation
//! and scheduling calls are performed by the caller.

use minivm_types::vm::{GSSR_IE, GSSR_SS, GSSR_UM, VM_VERSION};

/// Result of the vmtrap_return handler.
#[derive(Debug)]
pub struct ReturnResult {
    /// New ELR value (from GELR).
    pub new_elr: u32,
    /// Whether to swap r29 and GOSP (returning to user mode).
    pub swap_sp: bool,
    /// Whether to enable guest interrupts.
    pub enable_ie: bool,
    /// Whether to restore single-step mode.
    pub restore_ss: bool,
}

/// Process the return-from-event trap (trap1 0x01).
///
/// `gssr`: the guest SSR value saved during the event.
pub fn vmtrap_return(gssr: u32) -> ReturnResult {
    ReturnResult {
        new_elr: 0, // Caller sets from GELR
        swap_sp: gssr & GSSR_UM != 0,
        enable_ie: gssr & GSSR_IE != 0,
        restore_ss: gssr & GSSR_SS != 0,
    }
}

/// Result of setie trap.
#[derive(Debug)]
pub struct SetIeResult {
    /// Previous IE state (0 or 1).
    pub previous_ie: u32,
    /// Whether IE is now enabled.
    pub now_enabled: bool,
}

/// Process the set-interrupt-enable trap (trap1 0x03).
///
/// `enable`: whether to enable (r0 & 1).
/// `current_ie`: whether IE is currently enabled.
pub fn vmtrap_setie(enable: bool, current_ie: bool) -> SetIeResult {
    SetIeResult {
        previous_ie: if current_ie { 1 } else { 0 },
        now_enabled: enable,
    }
}

/// Process the get-interrupt-enable trap (trap1 0x04).
///
/// Returns: the IE state as u32 (0 or 1).
pub fn vmtrap_getie(ie_enabled: bool) -> u32 {
    if ie_enabled {
        1
    } else {
        0
    }
}

/// Process the version trap (trap1 0x00).
///
/// Returns: the supported VM version.
pub fn vmtrap_version() -> u32 {
    VM_VERSION
}

/// Process the setregs trap (trap1 0x15).
///
/// Sets GELR, GSSR, GOSP, GBADVA from r0-r3.
pub struct SetRegsInput {
    pub r0: u32,
    pub r1: u32,
    pub r2: u32,
    pub r3: u32,
}

/// Result of setregs: new guest register values.
pub struct SetRegsResult {
    pub gelr: u32,
    pub gssr: u32,
    pub gosp: u32,
    pub gbadva: u32,
}

pub fn vmtrap_setregs(input: &SetRegsInput) -> SetRegsResult {
    SetRegsResult {
        gelr: input.r0,
        gssr: input.r1,
        gosp: input.r2,
        gbadva: input.r3,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_version() {
        assert_eq!(vmtrap_version(), 0x00000800);
    }

    #[test]
    fn test_return_from_user_mode() {
        let r = vmtrap_return(GSSR_UM | GSSR_IE);
        assert!(r.swap_sp);
        assert!(r.enable_ie);
        assert!(!r.restore_ss);
    }

    #[test]
    fn test_return_from_guest_mode() {
        let r = vmtrap_return(0);
        assert!(!r.swap_sp);
        assert!(!r.enable_ie);
    }

    #[test]
    fn test_return_with_ss() {
        let r = vmtrap_return(GSSR_SS);
        assert!(r.restore_ss);
    }

    #[test]
    fn test_setie() {
        let r = vmtrap_setie(true, false);
        assert_eq!(r.previous_ie, 0);
        assert!(r.now_enabled);

        let r = vmtrap_setie(false, true);
        assert_eq!(r.previous_ie, 1);
        assert!(!r.now_enabled);
    }

    #[test]
    fn test_getie() {
        assert_eq!(vmtrap_getie(true), 1);
        assert_eq!(vmtrap_getie(false), 0);
    }

    #[test]
    fn test_setregs() {
        let input = SetRegsInput {
            r0: 0x1000,
            r1: 0x2000,
            r2: 0x3000,
            r3: 0x4000,
        };
        let r = vmtrap_setregs(&input);
        assert_eq!(r.gelr, 0x1000);
        assert_eq!(r.gssr, 0x2000);
        assert_eq!(r.gosp, 0x3000);
        assert_eq!(r.gbadva, 0x4000);
    }
}
