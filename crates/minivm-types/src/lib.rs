/*
 * Copyright (c) Qualcomm Technologies, Inc. and/or its subsidiaries.
 * SPDX-License-Identifier: BSD-3-Clause-Clear
 */

//! Core types and constants for the Hexagon VM (minivm).
//!
//! This crate provides shared type definitions, enumerations, constants,
//! and register bitfield types used across all minivm crates. All types
//! are `#[no_std]` compatible and many are `#[repr(C)]` for binary
//! compatibility with the C hypervisor implementation.

#![no_std]
#![allow(non_camel_case_types)]

pub mod arch;
pub mod asid;
pub mod config;
pub mod consts;
pub mod error;
pub mod info;
pub mod linear;
pub mod pmap;
pub mod regs;
pub mod timer;
pub mod translate;
pub mod trap;
pub mod vm;
pub mod vmint;
