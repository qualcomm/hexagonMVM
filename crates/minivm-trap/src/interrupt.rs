/*
 * Copyright (c) Qualcomm Technologies, Inc. and/or its subsidiaries.
 * SPDX-License-Identifier: BSD-3-Clause-Clear
 */

//! HW interrupt entry/exit logic.
//!
//! Hardware interrupts arrive via the L1 interrupt controller (vectors 16-31).
//! L2 interrupts are multiplexed through a single L1 interrupt and require
//! reading the L2VIC VID register to determine the actual source.

/// L1 interrupt that routes to L2VIC.
pub const L2_CORE_INTERRUPT: u32 = 2;

/// Extract L1 interrupt number from SSR.
///
/// The low 5 bits of the SSR cause field contain the interrupt index
/// when an interrupt exception occurs.
pub const fn extract_l1_int(ssr: u32) -> u32 {
    ssr & 0x1F
}

/// Check if an L1 interrupt is the L2VIC multiplexer.
pub const fn is_l2_core_interrupt(l1_int: u32) -> bool {
    l1_int == L2_CORE_INTERRUPT
}

/// Map an L2VIC VID value to a global interrupt number.
///
/// L2 interrupt numbers start at 32 in the global interrupt space.
pub const fn l2_vid_to_global(vid: u32) -> u32 {
    vid + 32
}

/// Resolve the global interrupt number from L1 interrupt info.
///
/// If the L1 interrupt is the L2 multiplexer, use the VID value.
/// Otherwise, the L1 interrupt number is the global number directly.
pub const fn resolve_interrupt(l1_int: u32, l2_vid: u32) -> u32 {
    if is_l2_core_interrupt(l1_int) {
        l2_vid_to_global(l2_vid)
    } else {
        l1_int
    }
}

/// Calculate the interrupt handler table offset.
///
/// Each handler entry is 8 bytes (function pointer + parameter).
pub const fn handler_offset(intno: u32) -> usize {
    (intno as usize) * 8
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_l1_int() {
        assert_eq!(extract_l1_int(0x00), 0);
        assert_eq!(extract_l1_int(0x02), 2); // L2 core interrupt
        assert_eq!(extract_l1_int(0x1F), 31);
        assert_eq!(extract_l1_int(0xFF), 31);
    }

    #[test]
    fn test_is_l2_core_interrupt() {
        assert!(is_l2_core_interrupt(2));
        assert!(!is_l2_core_interrupt(0));
        assert!(!is_l2_core_interrupt(1));
        assert!(!is_l2_core_interrupt(3));
    }

    #[test]
    fn test_l2_vid_to_global() {
        assert_eq!(l2_vid_to_global(0), 32);
        assert_eq!(l2_vid_to_global(10), 42);
        assert_eq!(l2_vid_to_global(255), 287);
    }

    #[test]
    fn test_resolve_l1_interrupt() {
        assert_eq!(resolve_interrupt(0, 0), 0);
        assert_eq!(resolve_interrupt(5, 0), 5);
        assert_eq!(resolve_interrupt(31, 0), 31);
    }

    #[test]
    fn test_resolve_l2_interrupt() {
        assert_eq!(resolve_interrupt(2, 0), 32);
        assert_eq!(resolve_interrupt(2, 10), 42);
        assert_eq!(resolve_interrupt(2, 255), 287);
    }

    #[test]
    fn test_handler_offset() {
        assert_eq!(handler_offset(0), 0);
        assert_eq!(handler_offset(1), 8);
        assert_eq!(handler_offset(32), 256);
    }
}
