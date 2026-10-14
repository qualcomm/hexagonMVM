/*
 * Copyright (c) Qualcomm Technologies, Inc. and/or its subsidiaries.
 * SPDX-License-Identifier: BSD-3-Clause-Clear
 */

//! Kernel error codes.

use strum::FromRepr;

/// VM error codes returned to guests.
#[derive(Clone, Copy, Debug, PartialEq, Eq, FromRepr)]
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
    pub const fn is_ok(self) -> bool {
        matches!(self, Self::Ok)
    }
    pub const fn is_err(self) -> bool {
        !self.is_ok()
    }
}
