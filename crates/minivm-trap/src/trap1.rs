/*
 * Copyright (c) Qualcomm Technologies, Inc. and/or its subsidiaries.
 * SPDX-License-Identifier: BSD-3-Clause-Clear
 */

//! trap1 handler (VM trap dispatch).
//!
//! trap1 is used by guest VMs to invoke hypervisor services.
//! The trap number from SSR selects which VM trap to invoke.
//! Dispatch logic is in minivm-vm; this module handles the entry/exit
//! framing and permission checks.

/// Check if a trap1 is permitted for the calling thread.
///
/// `trap_num`: the trap number (0-31).
/// `trapmask`: the thread's allowed trap bitmask.
///
/// Each bit in trapmask corresponds to a trap number.
/// Bit N set means trap N is allowed.
pub const fn is_trap_permitted(trap_num: u8, trapmask: u32) -> bool {
    if trap_num >= 32 {
        return false;
    }
    (trapmask >> trap_num) & 1 != 0
}

/// Extract the 5-bit trap1 number from SSR.
pub const fn extract_trap1_num(ssr: u32) -> u8 {
    (ssr & 0x1F) as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_trap_permitted() {
        let mask = 0xFFFF_FFFF; // all traps allowed
        assert!(is_trap_permitted(0, mask));
        assert!(is_trap_permitted(31, mask));
    }

    #[test]
    fn test_trap_not_permitted() {
        let mask = 0x0000_0001; // only trap 0 allowed
        assert!(is_trap_permitted(0, mask));
        assert!(!is_trap_permitted(1, mask));
        assert!(!is_trap_permitted(31, mask));
    }

    #[test]
    fn test_trap_out_of_range() {
        assert!(!is_trap_permitted(32, 0xFFFF_FFFF));
        assert!(!is_trap_permitted(255, 0xFFFF_FFFF));
    }

    #[test]
    fn test_extract_trap1_num() {
        assert_eq!(extract_trap1_num(0x00), 0);
        assert_eq!(extract_trap1_num(0x01), 1);
        assert_eq!(extract_trap1_num(0x1F), 31);
        assert_eq!(extract_trap1_num(0xFF), 31); // only low 5 bits
    }

    #[test]
    fn test_selective_mask() {
        // Only version (0), return (1), intop (5), wait (16) allowed
        let mask = (1 << 0) | (1 << 1) | (1 << 5) | (1 << 16);
        assert!(is_trap_permitted(0, mask));
        assert!(is_trap_permitted(1, mask));
        assert!(!is_trap_permitted(2, mask));
        assert!(is_trap_permitted(5, mask));
        assert!(!is_trap_permitted(6, mask));
        assert!(is_trap_permitted(16, mask));
    }
}
