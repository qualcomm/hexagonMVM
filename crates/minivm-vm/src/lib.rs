/*
 * Copyright (c) Qualcomm Technologies, Inc. and/or its subsidiaries.
 * SPDX-License-Identifier: BSD-3-Clause-Clear
 */

#![no_std]

#[cfg(test)]
extern crate alloc;

pub mod vmblock;
pub mod vmconfig;
pub mod vmevent;
pub mod vmfuncs;
pub mod vmmap;
pub mod vmop;
pub mod vmtrap;
pub mod vmwork;
