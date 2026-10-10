/*
 * Copyright (c) Qualcomm Technologies, Inc. and/or its subsidiaries.
 * SPDX-License-Identifier: BSD-3-Clause-Clear
 */

//! Memory management: ASID table, address translation, TLB fill, STLB, allocator.

#![no_std]
#![cfg_attr(target_arch = "hexagon", feature(asm_experimental_arch))]

#[cfg(test)]
extern crate alloc;

pub mod asid;
pub mod heap;
pub mod stlb;
pub mod tlb_fill;
pub mod translate;
