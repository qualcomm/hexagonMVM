/*
 * Copyright (c) Qualcomm Technologies, Inc. and/or its subsidiaries.
 * SPDX-License-Identifier: BSD-3-Clause-Clear
 */

//! VM operations (boot, status, free).
//!
//! These operations manage the VM lifecycle through the trap0 VMOP interface.

use minivm_types::vm::VmId;

/// VM operation types.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum VmOp {
    /// Boot a VM: start its first thread.
    Boot = 0,
    /// Query VM exit status.
    Status = 1,
    /// Free a VM's resources.
    Free = 2,
}

impl VmOp {
    pub const fn from_raw(val: u8) -> Option<Self> {
        match val {
            0 => Some(Self::Boot),
            1 => Some(Self::Status),
            2 => Some(Self::Free),
            _ => None,
        }
    }
}

/// Result of a VM boot operation.
#[derive(Debug, PartialEq, Eq)]
pub enum BootResult {
    /// Boot succeeded, returns the new thread's VmId.
    Ok(VmId),
    /// Boot failed (invalid parameters, no free threads, etc.).
    Fail,
}

/// Validate VM boot parameters.
///
/// `pc`: entry point (must be 4-byte aligned).
/// `sp`: stack pointer (must be 8-byte aligned).
/// `max_cpus`: VM must have CPUs configured.
pub fn validate_boot(pc: u32, sp: u32, max_cpus: u32) -> Result<(), &'static str> {
    if pc & 0x3 != 0 {
        return Err("PC not aligned");
    }
    if sp & 0x7 != 0 {
        return Err("SP not aligned");
    }
    if max_cpus == 0 {
        return Err("No CPUs configured");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_vmop_from_raw() {
        assert_eq!(VmOp::from_raw(0), Some(VmOp::Boot));
        assert_eq!(VmOp::from_raw(1), Some(VmOp::Status));
        assert_eq!(VmOp::from_raw(2), Some(VmOp::Free));
        assert_eq!(VmOp::from_raw(3), None);
    }

    #[test]
    fn test_validate_boot_ok() {
        assert!(validate_boot(0x1000, 0x8000, 4).is_ok());
    }

    #[test]
    fn test_validate_boot_bad_pc() {
        assert!(validate_boot(0x1001, 0x8000, 4).is_err());
    }

    #[test]
    fn test_validate_boot_bad_sp() {
        assert!(validate_boot(0x1000, 0x8001, 4).is_err());
    }

    #[test]
    fn test_validate_boot_no_cpus() {
        assert!(validate_boot(0x1000, 0x8000, 0).is_err());
    }
}
