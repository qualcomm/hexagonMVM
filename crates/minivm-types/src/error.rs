/*
 * Copyright (c) Qualcomm Technologies, Inc. and/or its subsidiaries.
 * SPDX-License-Identifier: BSD-3-Clause-Clear
 */

//! Kernel error codes.

/// VM error codes returned to guests.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(i32)]
pub enum VmError {
    Ok = 0,
    BadArg = -1,
    NoMem = -2,
    NotFound = -3,
    Busy = -4,
    NoPermission = -5,
    BadState = -6,
    NotSupported = -7,
}

impl VmError {
    pub const fn from_raw(val: i32) -> Option<Self> {
        match val {
            0 => Some(Self::Ok),
            -1 => Some(Self::BadArg),
            -2 => Some(Self::NoMem),
            -3 => Some(Self::NotFound),
            -4 => Some(Self::Busy),
            -5 => Some(Self::NoPermission),
            -6 => Some(Self::BadState),
            -7 => Some(Self::NotSupported),
            _ => None,
        }
    }

    pub const fn is_ok(self) -> bool {
        matches!(self, Self::Ok)
    }
    pub const fn is_err(self) -> bool {
        !self.is_ok()
    }
}
