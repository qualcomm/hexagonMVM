/*
 * Copyright (c) Qualcomm Technologies, Inc. and/or its subsidiaries.
 * SPDX-License-Identifier: BSD-3-Clause-Clear
 */

//! Context switch.
//!
//! The actual context switch is architecture-specific (requires inline
//! assembly to save/restore all registers). This module provides the
//! accounting logic around the switch.
//!
//! On real hardware, the switch saves the current thread's registers into
//! its ThreadContext, loads the new thread's registers, and jumps to its ELR.
//!
//! For host testing, we just update the cycle counters and status fields.

use crate::context::ThreadContext;

/// Perform pre-switch accounting on the outgoing thread.
///
/// Records cycle counts and clears any stale state.
pub fn pre_switch_out(threads: &mut [ThreadContext], me: u32, cycles: u64) {
    if me != 0 {
        threads[me as usize].totalcycles = cycles;
    }
}

/// Perform post-switch accounting on the incoming thread.
pub fn post_switch_in(threads: &mut [ThreadContext], new: u32, cycles: u64) {
    if new != 0 {
        let prev_cycles = threads[new as usize].totalcycles;
        threads[new as usize].totalcycles = cycles;
        let _ = prev_cycles; // cycle delta can be used for accounting
    }
}
