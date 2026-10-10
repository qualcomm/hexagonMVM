/*
 * Copyright (c) Qualcomm Technologies, Inc. and/or its subsidiaries.
 * SPDX-License-Identifier: BSD-3-Clause-Clear
 */

//! Semihosting interface for hexagon-sim.
//!
//! The simulator intercepts `trap0(#0)` and handles it as a
//! semihosting request. Register convention:
//!   - r0 = operation number
//!   - r1 = argument 1
//!   - r2 = argument 2

#[cfg(target_arch = "hexagon")]
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

	.globl semihost_exit
	.type semihost_exit, @function
semihost_exit:
	r1 = r0
	r2 = r0
	r0 = #24
	trap0(#0)
	jumpr r31
	.size semihost_exit, . - semihost_exit

"#
);

#[cfg(target_arch = "hexagon")]
extern "C" {
    fn semihost_write0(s: *const u8);
    fn semihost_exit(code: u32) -> !;
}

pub fn write0(s: &[u8]) {
    assert!(
        s.last() == Some(&0),
        "semihosting write0 requires null-terminated string"
    );
    #[cfg(target_arch = "hexagon")]
    unsafe {
        semihost_write0(s.as_ptr());
    }
    #[cfg(not(target_arch = "hexagon"))]
    let _ = s;
}

pub fn exit(code: u32) -> ! {
    #[cfg(target_arch = "hexagon")]
    unsafe {
        semihost_exit(code);
    }
    #[cfg(not(target_arch = "hexagon"))]
    {
        let _ = code;
        #[allow(clippy::empty_loop)]
        loop {}
    }
}
