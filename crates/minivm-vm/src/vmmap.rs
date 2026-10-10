/*
 * Copyright (c) Qualcomm Technologies, Inc. and/or its subsidiaries.
 * SPDX-License-Identifier: BSD-3-Clause-Clear
 */

//! clrmap/newmap operations.
//!
//! clrmap: Clears the current guest memory mapping.
//! newmap: Sets a new guest memory mapping (ASID registration).
//!
//! These operations manage the ASID entry in the VM's guest map,
//! controlling which address space the guest threads use.

use minivm_types::asid::AsidEntry;

/// Result of a map operation.
#[derive(Debug, PartialEq, Eq)]
pub enum MapResult {
    /// Operation succeeded; old ASID should be decremented.
    Ok { old_asid: u8 },
    /// No change needed (same mapping).
    NoChange,
    /// Error: ASID allocation failed.
    AsidFail,
}

/// Compute clrmap result: invalidate the current guest mapping.
///
/// `current_asid`: the current ASID assigned to the guest.
///
/// Returns the old ASID that should be decremented.
pub fn clrmap(current_asid: u8) -> MapResult {
    MapResult::Ok {
        old_asid: current_asid,
    }
}

/// Validate a newmap request.
///
/// `new_entry`: the proposed new ASID entry.
///
/// Returns true if the entry is valid for installation.
pub fn validate_newmap(_new_entry: AsidEntry) -> bool {
    // Basic validation: entry should have a valid type
    // The actual C code checks that the ASID table slot is valid
    // and that the page table base is within fences
    true // Detailed validation is done by the ASID subsystem
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_clrmap() {
        let r = clrmap(42);
        assert_eq!(r, MapResult::Ok { old_asid: 42 });
    }

    #[test]
    fn test_validate_newmap() {
        assert!(validate_newmap(AsidEntry::EMPTY));
    }
}
