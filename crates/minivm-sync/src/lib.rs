/*
 * Copyright (c) Qualcomm Technologies, Inc. and/or its subsidiaries.
 * SPDX-License-Identifier: BSD-3-Clause-Clear
 */

//! Synchronization primitives for the Hexagon VM.
//!
//! Provides spinlocks (via the `spin` crate),
//! a Big Kernel Lock (BKL), and atomic bitmask operations.

#![no_std]

pub mod atomic;
pub mod bkl;
pub mod spinlock;
