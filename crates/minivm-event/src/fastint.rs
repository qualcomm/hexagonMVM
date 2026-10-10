/*
 * Copyright (c) Qualcomm Technologies, Inc. and/or its subsidiaries.
 * SPDX-License-Identifier: BSD-3-Clause-Clear
 */

//! Fast interrupt context management.
//!
//! Fast interrupts bypass the normal scheduling path and execute a
//! registered handler directly. The handler runs at monitor privilege
//! but with its own stack context.

/// L2 interrupt start index.
///
/// Interrupts at or above this index are L2 interrupts that
/// require special handling through the L2VIC.
pub const L2_INTERRUPT_START: u32 = 32;

/// Size of per-fastint context storage (bytes).
pub const FASTINT_CONTEXT_SIZE: u32 = 256;

/// Maximum number of fast interrupt registrations.
pub const MAX_FASTINTS: u32 = 32;

/// Check if an interrupt number is an L2 interrupt.
pub const fn is_l2_interrupt(intno: u32) -> bool {
    intno >= L2_INTERRUPT_START
}

/// Map an L2 VID register value to a global interrupt number.
///
/// L2 interrupts are numbered starting at 32, with the VID
/// register providing the offset within the L2VIC.
pub const fn l2_to_global_int(vid: u32) -> u32 {
    vid + L2_INTERRUPT_START
}

/// Extract interrupt number from SSR cause field.
///
/// The low 5 bits of SSR contain the L1 interrupt number.
pub const fn extract_int_from_ssr(ssr: u32) -> u32 {
    ssr & 0x1F
}

/// Calculate handler table offset for an interrupt.
///
/// Each handler entry is 8 bytes (function pointer + parameter).
pub const fn handler_table_offset(intno: u32) -> u32 {
    intno * 8
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_l2_interrupt() {
        assert!(!is_l2_interrupt(0));
        assert!(!is_l2_interrupt(15));
        assert!(!is_l2_interrupt(31));
        assert!(is_l2_interrupt(32));
        assert!(is_l2_interrupt(100));
    }

    #[test]
    fn test_l2_to_global() {
        assert_eq!(l2_to_global_int(0), 32);
        assert_eq!(l2_to_global_int(5), 37);
        assert_eq!(l2_to_global_int(255), 287);
    }

    #[test]
    fn test_extract_int_from_ssr() {
        assert_eq!(extract_int_from_ssr(0x00), 0);
        assert_eq!(extract_int_from_ssr(0x10), 16);
        assert_eq!(extract_int_from_ssr(0x1F), 31);
        assert_eq!(extract_int_from_ssr(0xFF), 31); // only low 5 bits
    }

    #[test]
    fn test_handler_table_offset() {
        assert_eq!(handler_table_offset(0), 0);
        assert_eq!(handler_table_offset(1), 8);
        assert_eq!(handler_table_offset(31), 248);
    }
}
