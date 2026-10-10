/*
 * Copyright (c) Qualcomm Technologies, Inc. and/or its subsidiaries.
 * SPDX-License-Identifier: BSD-3-Clause-Clear
 */

//! VM event generation.
//!
//! When a VM event (interrupt, error, trap0) needs to be delivered to
//! the guest, this module computes the new register state (GELR, GSSR,
//! GOSP, GBADVA) and the new ELR pointing into the GEVB.

use minivm_types::vm::{GSSR_IE, GSSR_SS, GSSR_UM};

#[cfg(test)]
use minivm_types::vm::{ERROR_GEVB_OFFSET, INTERRUPT_GEVB_OFFSET, TRAP0_GEVB_OFFSET};

/// Guest register state for event delivery.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct GuestRegs {
    pub gelr: u32,
    pub gssr: u32,
    pub gosp: u32,
    pub gbadva: u32,
}

/// Result of VM event computation.
///
/// The caller applies these values to the thread context.
#[derive(Clone, Copy, Debug)]
pub struct VmEventResult {
    /// New GELR (saved from current ELR).
    pub gelr: u32,
    /// New GSSR (cause + saved mode bits).
    pub gssr: u32,
    /// New GOSP (saved from r29 if entering from user mode).
    pub gosp: u32,
    /// New GBADVA (bad virtual address for the event).
    pub gbadva: u32,
    /// New ELR (GEVB + vec_offset).
    pub new_elr: u32,
    /// Whether we were in guest (user) mode before.
    pub was_user_mode: bool,
    /// New r29 value (old GOSP, only valid when was_user_mode is true).
    pub new_r29: u32,
}

/// Compute the register state for a VM event delivery.
///
/// `gbadva`: the bad virtual address for this event.
/// `cause`: the SSR cause code.
/// `vec_offset`: offset into GEVB for this event type.
/// `gevb`: current guest event vector base.
/// `current_elr`: current ELR value.
/// `current_r29`: current stack pointer (r29).
/// `ssr_guest`: whether currently in guest mode (ssr.guest bit).
/// `ie_enabled`: whether guest interrupts are currently enabled.
/// `ss_enabled`: whether single-step is active.
#[allow(clippy::too_many_arguments)]
pub fn compute_vm_event(
    gbadva: u32,
    cause: u32,
    vec_offset: u32,
    gevb: u32,
    current_elr: u32,
    current_r29: u32,
    ssr_guest: bool,
    ie_enabled: bool,
    ss_enabled: bool,
) -> Option<VmEventResult> {
    // GEVB must be set
    if gevb == 0 {
        return None; // Fatal: no event vector base
    }

    let mut gssr = cause & 0xFF; // Low 8 bits = cause

    // Handle guest/user mode transition
    // ssr_guest == false means we're in user mode (not guest kernel mode)
    let (gosp, new_r29, was_user_mode) = if !ssr_guest {
        // Entering from user mode: swap r29 and GOSP, set UM bit
        gssr |= GSSR_UM;
        // gosp = old r29, new_r29 = old gosp (caller must supply old gosp as current_r29's pair)
        (current_r29, 0, true) // new_r29 must be set by caller from old GOSP
    } else {
        // Already in guest kernel mode: GOSP/r29 unchanged
        (0, 0, false)
    };

    // Save IE state
    if ie_enabled {
        gssr |= GSSR_IE;
    }

    // Save SS state
    if ss_enabled {
        gssr |= GSSR_SS;
    }

    Some(VmEventResult {
        gelr: current_elr,
        gssr,
        gosp,
        gbadva,
        new_elr: gevb.wrapping_add(vec_offset),
        was_user_mode,
        new_r29,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_event_no_gevb() {
        assert!(compute_vm_event(
            0,
            0x1B,
            ERROR_GEVB_OFFSET,
            0,
            0x1000,
            0x8000,
            true,
            false,
            false
        )
        .is_none());
    }

    #[test]
    fn test_event_from_guest_mode() {
        let r = compute_vm_event(
            0xBAAD, // gbadva
            0x1B,   // cause
            ERROR_GEVB_OFFSET,
            0x10000, // gevb
            0x2000,  // current ELR
            0x8000,  // r29
            true,    // ssr_guest = true (already in guest)
            false,   // IE disabled
            false,   // SS disabled
        )
        .unwrap();

        assert_eq!(r.gelr, 0x2000);
        assert_eq!(r.gbadva, 0xBAAD);
        assert_eq!(r.new_elr, 0x10000 + ERROR_GEVB_OFFSET);
        assert_eq!(r.gssr & 0xFF, 0x1B); // cause
        assert_eq!(r.gssr & GSSR_UM, 0); // Not from user mode
        assert!(!r.was_user_mode);
    }

    #[test]
    fn test_event_from_user_mode() {
        let r = compute_vm_event(
            0,
            0x05,
            TRAP0_GEVB_OFFSET,
            0x10000,
            0x3000,
            0x9000, // r29 = stack pointer
            false,  // ssr_guest = false (user mode)
            true,   // IE enabled
            false,
        )
        .unwrap();

        assert_eq!(r.gosp, 0x9000); // r29 saved to GOSP
        assert!(r.was_user_mode);
        assert_ne!(r.gssr & GSSR_UM, 0); // UM bit set
        assert_ne!(r.gssr & GSSR_IE, 0); // IE bit saved
    }

    #[test]
    fn test_event_interrupt() {
        let r = compute_vm_event(
            0,
            0,
            INTERRUPT_GEVB_OFFSET,
            0x20000,
            0x4000,
            0x5000,
            true,
            true,
            false,
        )
        .unwrap();

        assert_eq!(r.new_elr, 0x20000 + INTERRUPT_GEVB_OFFSET);
        assert_ne!(r.gssr & GSSR_IE, 0); // IE was enabled
    }

    #[test]
    fn test_event_single_step() {
        let r = compute_vm_event(
            0,
            0,
            ERROR_GEVB_OFFSET,
            0x10000,
            0x1000,
            0x8000,
            true,
            false,
            true, // SS enabled
        )
        .unwrap();

        assert_ne!(r.gssr & GSSR_SS, 0); // SS bit saved
    }
}
