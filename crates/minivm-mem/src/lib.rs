/*
 * Copyright (c) Qualcomm Technologies, Inc. and/or its subsidiaries.
 * SPDX-License-Identifier: BSD-3-Clause-Clear
 */

//! Memory management used by the monitor: ASIDs, address translation, and
//! hardware-TLB entry formatting.

#![no_std]

#[cfg(test)]
extern crate alloc;

pub mod asid;
pub mod tlb_fill;
pub mod translate;
