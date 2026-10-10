/*
 * Copyright (c) Qualcomm Technologies, Inc. and/or its subsidiaries.
 * SPDX-License-Identifier: BSD-3-Clause-Clear
 */

//! Architecture abstraction layer for Hexagon processors.
//!
//! Provides Hexagon-specific register access via inline assembly,
//! TLB operations, cache operations, interrupt control, and
//! hardware thread management.

#![no_std]
#![cfg_attr(target_arch = "hexagon", feature(asm_experimental_arch))]
// Hexagon register-pair syntax (e.g. `r7:6`) triggers a false positive.
#![cfg_attr(target_arch = "hexagon", allow(named_asm_labels))]

/// Read a Hexagon system register. Returns `$default` on non-Hexagon hosts.
#[macro_export]
macro_rules! hexagon_read_reg {
	($reg:literal, $ty:ty, $default:expr) => {{
		#[cfg(target_arch = "hexagon")]
		{
			let val: $ty;
			unsafe { core::arch::asm!(concat!("{val} = ", $reg), val = out(reg) val, options(nomem, nostack)) };
			val
		}
		#[cfg(not(target_arch = "hexagon"))]
		{ $default }
	}};
}

/// Write a Hexagon system register. No-op on non-Hexagon hosts.
#[macro_export]
macro_rules! hexagon_write_reg {
	($reg:literal, $val:expr) => {{
		#[cfg(target_arch = "hexagon")]
		{ let val = $val; unsafe { core::arch::asm!(concat!($reg, " = {val}"), val = in(reg) val, options(nomem, nostack)) }; }
		#[cfg(not(target_arch = "hexagon"))]
		{ let _ = $val; }
	}};
}

/// Execute a Hexagon instruction with no inputs/outputs. No-op on non-Hexagon.
#[macro_export]
macro_rules! hexagon_insn {
    ($insn:literal) => {{
        #[cfg(target_arch = "hexagon")]
        unsafe {
            core::arch::asm!($insn, options(nomem, nostack))
        };
    }};
}

/// Execute a Hexagon instruction with one input register. No-op on non-Hexagon.
#[macro_export]
macro_rules! hexagon_insn_r {
	($insn:literal, $val:expr) => {{
		#[cfg(target_arch = "hexagon")]
		{ let val = $val; unsafe { core::arch::asm!(concat!($insn, "({val})"), val = in(reg) val, options(nostack)) }; }
		#[cfg(not(target_arch = "hexagon"))]
		{ let _ = $val; }
	}};
}

pub mod cache;
pub mod hexagon;
pub mod interrupts;
pub mod syscfg;
pub mod thread_hw;
pub mod tlb;

/// Coprocessor type (from cfg_table COPROC_TYPE).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CoprocType(pub u32);

impl CoprocType {
    pub const HVX_MASK: u32 = 0x1;

    pub const fn has_hvx(self) -> bool {
        self.0 & Self::HVX_MASK != 0
    }
}
