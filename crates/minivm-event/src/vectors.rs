/*
 * Copyright (c) Qualcomm Technologies, Inc. and/or its subsidiaries.
 * SPDX-License-Identifier: BSD-3-Clause-Clear
 */

//! Interrupt/exception vector table layout.
//!
//! The Hexagon exception vector table has 48 entries (4 bytes each):
//! - Entries 0-15: exception vectors (reset, NMI, error, TLB miss, traps, debug)
//! - Entries 16-47: L1 interrupt vectors
//!
//! Each entry is a jump instruction to the corresponding handler.

/// Exception vector indices.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum ExceptionVector {
    Reset = 0,
    Nmi = 1,
    Error = 2,
    /// Reserved (3).
    Rsvd3 = 3,
    TlbMissX = 4,
    /// Reserved (5).
    Rsvd5 = 5,
    TlbMissRW = 6,
    /// DMA TLB miss (v73+) or error.
    DmaTlbOrError = 7,
    Trap0 = 8,
    Trap1 = 9,
    /// Reserved (10).
    Rsvd10 = 10,
    /// Floating-point trap or error.
    FpTrap = 11,
    Debug = 12,
    /// Reserved (13).
    Rsvd13 = 13,
    /// Reserved (14).
    Rsvd14 = 14,
    /// Reserved (15).
    Rsvd15 = 15,
}

/// Total number of exception vector entries (0-15).
pub const NUM_EXCEPTION_VECTORS: u32 = 16;

/// Total number of L1 interrupt vectors (16-47).
pub const NUM_INTERRUPT_VECTORS: u32 = 32;

/// Total vector table size (exceptions + interrupts).
pub const VECTOR_TABLE_SIZE: u32 = 48;

/// Bytes per vector entry (one jump instruction).
pub const BYTES_PER_VECTOR: u32 = 4;

/// Total vector table size in bytes.
pub const VECTOR_TABLE_BYTES: u32 = VECTOR_TABLE_SIZE * BYTES_PER_VECTOR;

/// Action for each exception vector entry.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VectorAction {
    /// Handle as reset.
    Reset,
    /// Handle as NMI.
    Nmi,
    /// Handle as error exception.
    Error,
    /// Handle as TLB miss for execute.
    TlbMissX,
    /// Handle as TLB miss for read/write.
    TlbMissRW,
    /// Handle as trap0 (semihosting).
    Trap0,
    /// Handle as trap1 (VM trap).
    Trap1,
    /// Handle as debug exception.
    Debug,
    /// Handle as L1 interrupt.
    Interrupt,
    /// Reserved vector (generates error).
    Reserved,
}

/// Map an exception vector index to its handler action.
pub fn vector_action(index: u32) -> VectorAction {
    match index {
        0 => VectorAction::Reset,
        1 => VectorAction::Nmi,
        2 => VectorAction::Error,
        4 => VectorAction::TlbMissX,
        6 => VectorAction::TlbMissRW,
        7 => VectorAction::Error, // DMA TLB or FP error
        8 => VectorAction::Trap0,
        9 => VectorAction::Trap1,
        11 => VectorAction::Error, // FP trap
        12 => VectorAction::Debug,
        16..=47 => VectorAction::Interrupt,
        _ => VectorAction::Reserved,
    }
}

/// Calculate byte offset of a vector entry in the table.
pub const fn vector_offset(index: u32) -> u32 {
    index * BYTES_PER_VECTOR
}

/// Check if a vector index is an interrupt (vs exception).
pub const fn is_interrupt_vector(index: u32) -> bool {
    index >= NUM_EXCEPTION_VECTORS && index < VECTOR_TABLE_SIZE
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_vector_action_exceptions() {
        assert_eq!(vector_action(0), VectorAction::Reset);
        assert_eq!(vector_action(1), VectorAction::Nmi);
        assert_eq!(vector_action(2), VectorAction::Error);
        assert_eq!(vector_action(4), VectorAction::TlbMissX);
        assert_eq!(vector_action(6), VectorAction::TlbMissRW);
        assert_eq!(vector_action(8), VectorAction::Trap0);
        assert_eq!(vector_action(9), VectorAction::Trap1);
        assert_eq!(vector_action(12), VectorAction::Debug);
    }

    #[test]
    fn test_vector_action_reserved() {
        assert_eq!(vector_action(3), VectorAction::Reserved);
        assert_eq!(vector_action(5), VectorAction::Reserved);
        assert_eq!(vector_action(10), VectorAction::Reserved);
        assert_eq!(vector_action(13), VectorAction::Reserved);
        assert_eq!(vector_action(14), VectorAction::Reserved);
        assert_eq!(vector_action(15), VectorAction::Reserved);
    }

    #[test]
    fn test_vector_action_interrupts() {
        for i in 16..=47 {
            assert_eq!(vector_action(i), VectorAction::Interrupt);
        }
    }

    #[test]
    fn test_vector_action_out_of_range() {
        assert_eq!(vector_action(48), VectorAction::Reserved);
        assert_eq!(vector_action(255), VectorAction::Reserved);
    }

    #[test]
    fn test_vector_offset() {
        assert_eq!(vector_offset(0), 0);
        assert_eq!(vector_offset(8), 32); // trap0
        assert_eq!(vector_offset(16), 64); // first interrupt
    }

    #[test]
    fn test_is_interrupt_vector() {
        assert!(!is_interrupt_vector(0));
        assert!(!is_interrupt_vector(9));
        assert!(!is_interrupt_vector(15));
        assert!(is_interrupt_vector(16));
        assert!(is_interrupt_vector(31));
        assert!(is_interrupt_vector(47));
        assert!(!is_interrupt_vector(48));
    }

    #[test]
    fn test_table_size() {
        assert_eq!(VECTOR_TABLE_BYTES, 192);
    }
}
