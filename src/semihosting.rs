/*
 * Copyright (c) Qualcomm Technologies, Inc. and/or its subsidiaries.
 * SPDX-License-Identifier: BSD-3-Clause-Clear
 */

//! Semihosting console output.
//!
//! The simulator intercepts `trap0(#0)` and handles it as a semihosting
//! request, with the operation in r0 and its argument in r1. This is the
//! monitor's console of last resort, for machines whose device tree names
//! no UART; see `console`.

core::arch::global_asm!(
    r#"
	.text
	.p2align 2

	.globl semihost_write0
	.type semihost_write0, @function
semihost_write0:
	r1 = r0
	r0 = #4
	r2 = #0
	trap0(#0)
	jumpr r31
	.size semihost_write0, . - semihost_write0
"#
);

extern "C" {
    fn semihost_write0(s: *const u8);
}

/// Write a NUL-terminated string.
pub fn write0(s: &[u8]) {
    assert!(
        s.last() == Some(&0),
        "semihosting write0 requires a terminated string"
    );
    unsafe { semihost_write0(s.as_ptr()) };
}
