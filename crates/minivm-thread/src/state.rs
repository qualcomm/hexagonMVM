/*
 * Copyright (c) Qualcomm Technologies, Inc. and/or its subsidiaries.
 * SPDX-License-Identifier: BSD-3-Clause-Clear
 */

//! Thread state query.
//!
//! Allows querying fields of a thread context by ID and byte offset.
//! The caller must ensure the target thread belongs to the same VM.

use minivm_types::vm::VmId;

/// Validate a thread state query.
///
/// `caller_vmidx`: the VM index of the calling thread
/// `target_id`: the VmId being queried
/// `offset`: byte offset into the context
/// `context_size`: total context struct size
pub fn validate_state_query(
    caller_vmidx: u8,
    target_id: VmId,
    offset: u32,
    context_size: u32,
    max_cpus: u16,
) -> Result<(), StateError> {
    if target_id.vmidx() != caller_vmidx {
        return Err(StateError::WrongVm);
    }
    if target_id.cpuidx() >= max_cpus {
        return Err(StateError::BadCpuIdx);
    }
    if offset > context_size.saturating_sub(8) {
        return Err(StateError::BadOffset);
    }
    Ok(())
}

/// Error from a thread state query.
#[derive(Debug, PartialEq, Eq)]
pub enum StateError {
    /// Target thread is in a different VM.
    WrongVm,
    /// Offset is out of bounds.
    BadOffset,
    /// CPU index is invalid.
    BadCpuIdx,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_ok() {
        let id = VmId::new(1, 0, 0);
        assert!(validate_state_query(1, id, 0, 320, 4).is_ok());
    }

    #[test]
    fn test_validate_wrong_vm() {
        let id = VmId::new(2, 0, 0);
        assert_eq!(
            validate_state_query(1, id, 0, 320, 4),
            Err(StateError::WrongVm)
        );
    }

    #[test]
    fn test_validate_bad_offset() {
        let id = VmId::new(1, 0, 0);
        // Offset 316 would read bytes 316-323 which is past size 320
        assert_eq!(
            validate_state_query(1, id, 316, 320, 4),
            Err(StateError::BadOffset)
        );
        // Offset 312 is ok (reads 312-319)
        assert!(validate_state_query(1, id, 312, 320, 4).is_ok());
    }

    #[test]
    fn test_validate_bad_cpuidx() {
        let id = VmId::new(1, 5, 0);
        assert_eq!(
            validate_state_query(1, id, 0, 320, 4),
            Err(StateError::BadCpuIdx)
        );
    }
}
