/*
 * Copyright (c) Qualcomm Technologies, Inc. and/or its subsidiaries.
 * SPDX-License-Identifier: BSD-3-Clause-Clear
 */

//! Thread creation.
//!
//! Thread creation allocates a context from the VM's free pool,
//! initializes its registers and state, and enqueues it to the
//! ready list.

use minivm_types::consts::MAX_PRIO;
use minivm_types::vm::{ThreadStatus, VmId};

/// Parameters for creating a new thread.
pub struct CreateParams {
    /// Program counter (entry point). Must be 4-byte aligned.
    pub pc: u32,
    /// Stack pointer. Must be 8-byte aligned.
    pub sp: u32,
    /// First argument (placed in r0).
    pub arg1: u32,
    /// Thread priority (0 = highest). Values > 255 are rejected.
    pub prio: u32,
}

/// Result of thread creation validation.
#[derive(Debug, PartialEq, Eq)]
pub enum CreateError {
    /// Priority exceeds maximum.
    BadPriority,
    /// Priority exceeds VM's best allowed priority.
    PriorityTooHigh,
    /// Stack pointer not 8-byte aligned.
    BadSp,
    /// Program counter not 4-byte aligned.
    BadPc,
    /// No free thread contexts available.
    NoFreeThread,
}

/// Validate thread creation parameters.
///
/// `bestprio`: the VM's best allowed priority.
pub fn validate_create(params: &CreateParams, bestprio: u32) -> Result<(), CreateError> {
    if params.prio > MAX_PRIO {
        return Err(CreateError::BadPriority);
    }
    if params.prio < bestprio {
        return Err(CreateError::PriorityTooHigh);
    }
    if params.sp & 0x7 != 0 {
        return Err(CreateError::BadSp);
    }
    if params.pc & 0x3 != 0 {
        return Err(CreateError::BadPc);
    }
    Ok(())
}

/// Initialize a thread context for a new thread.
///
/// This sets up the minimum register state needed to start execution.
/// The caller is responsible for allocating from the free pool and
/// enqueuing to the ready list.
///
/// `ctx_prio`: receives the new priority
/// `ctx_base_prio`: receives the base priority
/// `ctx_elr`: receives the entry point
/// `ctx_sp`: receives the stack pointer (r29)
/// `ctx_arg`: receives the argument (r0)
/// `ctx_vmstatus`: cleared to 0
pub struct ThreadInit {
    pub prio: u8,
    pub base_prio: u8,
    pub elr: u32,
    pub sp: u32,
    pub arg: u32,
    pub status: ThreadStatus,
    pub vmstatus: u8,
}

impl ThreadInit {
    /// Create the initialization values for a new thread.
    pub fn from_params(params: &CreateParams) -> Self {
        Self {
            prio: params.prio as u8,
            base_prio: params.prio as u8,
            elr: params.pc,
            sp: params.sp,
            arg: params.arg1,
            status: ThreadStatus::Ready,
            vmstatus: 0,
        }
    }
}

/// Build a VmId for a thread given its VM index and CPU index.
pub fn make_thread_id(vmidx: u8, cpuidx: u16) -> VmId {
    VmId::new(vmidx, cpuidx, 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_ok() {
        let params = CreateParams {
            pc: 0x1000,
            sp: 0x8000,
            arg1: 0,
            prio: 10,
        };
        assert!(validate_create(&params, 0).is_ok());
    }

    #[test]
    fn test_validate_bad_sp() {
        let params = CreateParams {
            pc: 0x1000,
            sp: 0x8001,
            arg1: 0,
            prio: 10,
        };
        assert_eq!(validate_create(&params, 0), Err(CreateError::BadSp));
    }

    #[test]
    fn test_validate_bad_pc() {
        let params = CreateParams {
            pc: 0x1001,
            sp: 0x8000,
            arg1: 0,
            prio: 10,
        };
        assert_eq!(validate_create(&params, 0), Err(CreateError::BadPc));
    }

    #[test]
    fn test_validate_bad_prio() {
        let params = CreateParams {
            pc: 0x1000,
            sp: 0x8000,
            arg1: 0,
            prio: 5,
        };
        assert_eq!(
            validate_create(&params, 10),
            Err(CreateError::PriorityTooHigh)
        );
    }

    #[test]
    fn test_validate_prio_too_large() {
        let params = CreateParams {
            pc: 0x1000,
            sp: 0x8000,
            arg1: 0,
            prio: 256,
        };
        assert_eq!(validate_create(&params, 0), Err(CreateError::BadPriority));
    }

    #[test]
    fn test_thread_init() {
        let params = CreateParams {
            pc: 0x2000,
            sp: 0x4000,
            arg1: 42,
            prio: 7,
        };
        let init = ThreadInit::from_params(&params);
        assert_eq!(init.prio, 7);
        assert_eq!(init.base_prio, 7);
        assert_eq!(init.elr, 0x2000);
        assert_eq!(init.sp, 0x4000);
        assert_eq!(init.arg, 42);
        assert_eq!(init.status, ThreadStatus::Ready);
        assert_eq!(init.vmstatus, 0);
    }

    #[test]
    fn test_make_thread_id() {
        let id = make_thread_id(1, 3);
        assert_eq!(id.vmidx(), 1);
        assert_eq!(id.cpuidx(), 3);
        assert_eq!(id.vint(), 0);
    }
}
