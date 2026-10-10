/*
 * Copyright (c) Qualcomm Technologies, Inc. and/or its subsidiaries.
 * SPDX-License-Identifier: BSD-3-Clause-Clear
 */

//! Scheduler: priority-based scheduling with context switch, ready list,
//! run list, yield, wait, and reschedule support.

#![no_std]

#[cfg(test)]
extern crate alloc;

pub mod context;
pub mod dosched;
pub mod lowprio;
pub mod readylist;
pub mod resched;
pub mod runlist;
pub mod switch_ctx;
pub mod wait;
pub mod yield_op;
