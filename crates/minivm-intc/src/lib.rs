/*
 * Copyright (c) Qualcomm Technologies, Inc. and/or its subsidiaries.
 * SPDX-License-Identifier: BSD-3-Clause-Clear
 */

//! Virtual interrupt controller.
//!
//! The interrupt controller has two layers:
//! - **Per-CPU interrupts** (first 16): stored in each thread's
//!   `cpuint_pending`/`cpuint_enabled` bitmasks
//! - **Shared interrupts** (up to thousands): stored in per-VM
//!   `pending[]`/`enable[]` bitmask arrays with per-CPU local masks
//!
//! Operations are dispatched through `IntOp` (nop, enable, disable, localen,
//! localdis, affinity, get, peek, status, post, clear).

#![no_std]

#[cfg(test)]
extern crate alloc;

pub mod deliver;
pub mod intop;
pub mod ipi;
pub mod percpu;
pub mod shared;
