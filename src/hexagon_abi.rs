/*
 * Copyright (c) Qualcomm Technologies, Inc. and/or its subsidiaries.
 * SPDX-License-Identifier: BSD-3-Clause-Clear
 */

//! Hexagon ABI callee-saved register save/restore stubs.
//!
//! The Hexagon calling convention uses helper functions for callee-saved
//! register save/restore sequences to reduce code size. These are normally
//! provided by `libclang_rt.builtins.a`, but for bare-metal builds we
//! provide them here.
//!
//! TODO: Remove this file once <https://github.com/rust-lang/compiler-builtins/pull/1180>
//! lands in a stable Rust release (provides these stubs via compiler-builtins).
//!
//! Reference: `common_entry_exit_abi1.S` and `common_entry_exit_abi2.S`
//! from the Hexagon toolchain runtime library.
//!
//! Note: All Hexagon packet delimiters `{ }` must be doubled `{{ }}` to
//! escape them through Rust's `global_asm!` formatting.

// ABI2 stubs: save/restore r16-r27 (callee-saved, ABI2 layout)
core::arch::global_asm!(
    r#"
/* ---- Save stubs (ABI2) ---- */

	.section .text.__save_r16_through_r27, "ax", @progbits
	.globl __save_r16_through_r27
	.type __save_r16_through_r27, @function
	.p2align 2
__save_r16_through_r27:
	{{
		memd(fp+#-48) = r27:26
		memd(fp+#-40) = r25:24
	}}
	{{
		memd(fp+#-32) = r23:22
		memd(fp+#-24) = r21:20
	}}
	{{
		memd(fp+#-16) = r19:18
		memd(fp+#-8) = r17:16
		jumpr lr
	}}
	.size __save_r16_through_r27, . - __save_r16_through_r27

	.section .text.__save_r16_through_r25, "ax", @progbits
	.globl __save_r16_through_r25
	.type __save_r16_through_r25, @function
	.p2align 2
__save_r16_through_r25:
	{{
		memd(fp+#-40) = r25:24
		memd(fp+#-32) = r23:22
	}}
	{{
		memd(fp+#-24) = r21:20
		memd(fp+#-16) = r19:18
	}}
	{{
		memd(fp+#-8) = r17:16
		jumpr lr
	}}
	.size __save_r16_through_r25, . - __save_r16_through_r25

	.section .text.__save_r16_through_r23, "ax", @progbits
	.globl __save_r16_through_r23
	.type __save_r16_through_r23, @function
	.p2align 2
__save_r16_through_r23:
	{{
		memd(fp+#-32) = r23:22
		memd(fp+#-24) = r21:20
	}}
	{{
		memd(fp+#-16) = r19:18
		memd(fp+#-8) = r17:16
		jumpr lr
	}}
	.size __save_r16_through_r23, . - __save_r16_through_r23

	.section .text.__save_r16_through_r21, "ax", @progbits
	.globl __save_r16_through_r21
	.type __save_r16_through_r21, @function
	.p2align 2
__save_r16_through_r21:
	{{
		memd(fp+#-24) = r21:20
		memd(fp+#-16) = r19:18
	}}
	{{
		memd(fp+#-8) = r17:16
		jumpr lr
	}}
	.size __save_r16_through_r21, . - __save_r16_through_r21

	.section .text.__save_r16_through_r19, "ax", @progbits
	.globl __save_r16_through_r19
	.type __save_r16_through_r19, @function
	.p2align 2
__save_r16_through_r19:
	{{
		memd(fp+#-16) = r19:18
		memd(fp+#-8) = r17:16
		jumpr lr
	}}
	.size __save_r16_through_r19, . - __save_r16_through_r19

	.section .text.__save_r16_through_r17, "ax", @progbits
	.globl __save_r16_through_r17
	.type __save_r16_through_r17, @function
	.p2align 2
__save_r16_through_r17:
	{{
		memd(fp+#-8) = r17:16
		jumpr lr
	}}
	.size __save_r16_through_r17, . - __save_r16_through_r17


/* ---- Restore stubs (ABI2) — normal return ---- */

	.section .text.__restore_r16_through_r27_and_deallocframe, "ax", @progbits
	.globl __restore_r16_through_r27_and_deallocframe
	.type __restore_r16_through_r27_and_deallocframe, @function
	.p2align 2
__restore_r16_through_r27_and_deallocframe:
	r27:26 = memd(fp+#-48)
	{{
		r25:24 = memd(fp+#-40)
		r23:22 = memd(fp+#-32)
	}}
	{{
		r21:20 = memd(fp+#-24)
		r19:18 = memd(fp+#-16)
	}}
	{{
		r17:16 = memd(fp+#-8)
		dealloc_return
	}}
	.size __restore_r16_through_r27_and_deallocframe, . - __restore_r16_through_r27_and_deallocframe

	.section .text.__restore_r16_through_r25_and_deallocframe, "ax", @progbits
	.globl __restore_r16_through_r25_and_deallocframe
	.type __restore_r16_through_r25_and_deallocframe, @function
	.p2align 2
__restore_r16_through_r25_and_deallocframe:
	{{
		r25:24 = memd(fp+#-40)
		r23:22 = memd(fp+#-32)
	}}
	{{
		r21:20 = memd(fp+#-24)
		r19:18 = memd(fp+#-16)
	}}
	{{
		r17:16 = memd(fp+#-8)
		dealloc_return
	}}
	.size __restore_r16_through_r25_and_deallocframe, . - __restore_r16_through_r25_and_deallocframe

	.section .text.__restore_r16_through_r23_and_deallocframe, "ax", @progbits
	.globl __restore_r16_through_r23_and_deallocframe
	.type __restore_r16_through_r23_and_deallocframe, @function
	.p2align 2
__restore_r16_through_r23_and_deallocframe:
	r23:22 = memd(fp+#-32)
	{{
		r21:20 = memd(fp+#-24)
		r19:18 = memd(fp+#-16)
	}}
	{{
		r17:16 = memd(fp+#-8)
		dealloc_return
	}}
	.size __restore_r16_through_r23_and_deallocframe, . - __restore_r16_through_r23_and_deallocframe

	.section .text.__restore_r16_through_r21_and_deallocframe, "ax", @progbits
	.globl __restore_r16_through_r21_and_deallocframe
	.type __restore_r16_through_r21_and_deallocframe, @function
	.p2align 2
__restore_r16_through_r21_and_deallocframe:
	{{
		r21:20 = memd(fp+#-24)
		r19:18 = memd(fp+#-16)
	}}
	{{
		r17:16 = memd(fp+#-8)
		dealloc_return
	}}
	.size __restore_r16_through_r21_and_deallocframe, . - __restore_r16_through_r21_and_deallocframe

	.section .text.__restore_r16_through_r19_and_deallocframe, "ax", @progbits
	.globl __restore_r16_through_r19_and_deallocframe
	.type __restore_r16_through_r19_and_deallocframe, @function
	.p2align 2
__restore_r16_through_r19_and_deallocframe:
	{{
		r19:18 = memd(fp+#-16)
		r17:16 = memd(fp+#-8)
	}}
	{{
		dealloc_return
	}}
	.size __restore_r16_through_r19_and_deallocframe, . - __restore_r16_through_r19_and_deallocframe

	.section .text.__restore_r16_through_r17_and_deallocframe, "ax", @progbits
	.globl __restore_r16_through_r17_and_deallocframe
	.type __restore_r16_through_r17_and_deallocframe, @function
	.p2align 2
__restore_r16_through_r17_and_deallocframe:
	{{
		r17:16 = memd(fp+#-8)
		dealloc_return
	}}
	.size __restore_r16_through_r17_and_deallocframe, . - __restore_r16_through_r17_and_deallocframe


/* ---- Restore stubs (ABI2) — before tail call ---- */

	.section .text.__restore_r16_through_r27_and_deallocframe_before_tailcall, "ax", @progbits
	.globl __restore_r16_through_r27_and_deallocframe_before_tailcall
	.type __restore_r16_through_r27_and_deallocframe_before_tailcall, @function
	.p2align 2
__restore_r16_through_r27_and_deallocframe_before_tailcall:
	r27:26 = memd(fp+#-48)
	{{
		r25:24 = memd(fp+#-40)
		r23:22 = memd(fp+#-32)
	}}
	{{
		r21:20 = memd(fp+#-24)
		r19:18 = memd(fp+#-16)
	}}
	{{
		r17:16 = memd(fp+#-8)
		deallocframe
		jumpr lr
	}}
	.size __restore_r16_through_r27_and_deallocframe_before_tailcall, . - __restore_r16_through_r27_and_deallocframe_before_tailcall

	.section .text.__restore_r16_through_r25_and_deallocframe_before_tailcall, "ax", @progbits
	.globl __restore_r16_through_r25_and_deallocframe_before_tailcall
	.type __restore_r16_through_r25_and_deallocframe_before_tailcall, @function
	.p2align 2
__restore_r16_through_r25_and_deallocframe_before_tailcall:
	{{
		r25:24 = memd(fp+#-40)
		r23:22 = memd(fp+#-32)
	}}
	{{
		r21:20 = memd(fp+#-24)
		r19:18 = memd(fp+#-16)
	}}
	{{
		r17:16 = memd(fp+#-8)
		deallocframe
		jumpr lr
	}}
	.size __restore_r16_through_r25_and_deallocframe_before_tailcall, . - __restore_r16_through_r25_and_deallocframe_before_tailcall

	.section .text.__restore_r16_through_r23_and_deallocframe_before_tailcall, "ax", @progbits
	.globl __restore_r16_through_r23_and_deallocframe_before_tailcall
	.type __restore_r16_through_r23_and_deallocframe_before_tailcall, @function
	.p2align 2
__restore_r16_through_r23_and_deallocframe_before_tailcall:
	{{
		r23:22 = memd(fp+#-32)
		r21:20 = memd(fp+#-24)
	}}
	r19:18 = memd(fp+#-16)
	{{
		r17:16 = memd(fp+#-8)
		deallocframe
		jumpr lr
	}}
	.size __restore_r16_through_r23_and_deallocframe_before_tailcall, . - __restore_r16_through_r23_and_deallocframe_before_tailcall

	.section .text.__restore_r16_through_r21_and_deallocframe_before_tailcall, "ax", @progbits
	.globl __restore_r16_through_r21_and_deallocframe_before_tailcall
	.type __restore_r16_through_r21_and_deallocframe_before_tailcall, @function
	.p2align 2
__restore_r16_through_r21_and_deallocframe_before_tailcall:
	{{
		r21:20 = memd(fp+#-24)
		r19:18 = memd(fp+#-16)
	}}
	{{
		r17:16 = memd(fp+#-8)
		deallocframe
		jumpr lr
	}}
	.size __restore_r16_through_r21_and_deallocframe_before_tailcall, . - __restore_r16_through_r21_and_deallocframe_before_tailcall

	.section .text.__restore_r16_through_r19_and_deallocframe_before_tailcall, "ax", @progbits
	.globl __restore_r16_through_r19_and_deallocframe_before_tailcall
	.type __restore_r16_through_r19_and_deallocframe_before_tailcall, @function
	.p2align 2
__restore_r16_through_r19_and_deallocframe_before_tailcall:
	r19:18 = memd(fp+#-16)
	{{
		r17:16 = memd(fp+#-8)
		deallocframe
		jumpr lr
	}}
	.size __restore_r16_through_r19_and_deallocframe_before_tailcall, . - __restore_r16_through_r19_and_deallocframe_before_tailcall

	.section .text.__restore_r16_through_r17_and_deallocframe_before_tailcall, "ax", @progbits
	.globl __restore_r16_through_r17_and_deallocframe_before_tailcall
	.type __restore_r16_through_r17_and_deallocframe_before_tailcall, @function
	.p2align 2
__restore_r16_through_r17_and_deallocframe_before_tailcall:
	{{
		r17:16 = memd(fp+#-8)
		deallocframe
		jumpr lr
	}}
	.size __restore_r16_through_r17_and_deallocframe_before_tailcall, . - __restore_r16_through_r17_and_deallocframe_before_tailcall


/* ---- ABI1 stubs: save/restore r24-r27 ---- */

	.section .text.__save_r24_through_r27, "ax", @progbits
	.globl __save_r24_through_r27
	.type __save_r24_through_r27, @function
	.p2align 2
__save_r24_through_r27:
	memd(fp+#-16) = r27:26
	.size __save_r24_through_r27, . - __save_r24_through_r27

	.globl __save_r24_through_r25
	.type __save_r24_through_r25, @function
	.p2align 2
__save_r24_through_r25:
	{{
		memd(fp+#-8) = r25:24
		jumpr lr
	}}
	.size __save_r24_through_r25, . - __save_r24_through_r25


	.section .text.__restore_r24_through_r27_and_deallocframe_before_tailcall, "ax", @progbits
	.globl __restore_r24_through_r27_and_deallocframe_before_tailcall
	.type __restore_r24_through_r27_and_deallocframe_before_tailcall, @function
	.p2align 2
__restore_r24_through_r27_and_deallocframe_before_tailcall:
	r27:26 = memd(fp+#-16)
	.size __restore_r24_through_r27_and_deallocframe_before_tailcall, . - __restore_r24_through_r27_and_deallocframe_before_tailcall

	.globl __restore_r24_through_r25_and_deallocframe_before_tailcall
	.type __restore_r24_through_r25_and_deallocframe_before_tailcall, @function
	.p2align 2
__restore_r24_through_r25_and_deallocframe_before_tailcall:
	{{
		r25:24 = memd(fp+#-8)
		deallocframe
		jumpr lr
	}}
	.size __restore_r24_through_r25_and_deallocframe_before_tailcall, . - __restore_r24_through_r25_and_deallocframe_before_tailcall


	.section .text.__restore_r24_through_r27_and_deallocframe, "ax", @progbits
	.globl __restore_r24_through_r27_and_deallocframe
	.type __restore_r24_through_r27_and_deallocframe, @function
	.p2align 2
__restore_r24_through_r27_and_deallocframe:
	{{
		lr = memw(fp+#4)
		r27:26 = memd(fp+#-16)
	}}
	{{
		r25:24 = memd(fp+#-8)
		deallocframe
		jumpr lr
	}}
	.size __restore_r24_through_r27_and_deallocframe, . - __restore_r24_through_r27_and_deallocframe

	.section .text.__restore_r24_through_r25_and_deallocframe, "ax", @progbits
	.globl __restore_r24_through_r25_and_deallocframe
	.type __restore_r24_through_r25_and_deallocframe, @function
	.p2align 2
__restore_r24_through_r25_and_deallocframe:
	{{
		r25:24 = memd(fp+#-8)
		deallocframe
	}}
	jumpr lr
	.size __restore_r24_through_r25_and_deallocframe, . - __restore_r24_through_r25_and_deallocframe

	.section .text.__deallocframe, "ax", @progbits
	.globl __deallocframe
	.type __deallocframe, @function
	.p2align 2
__deallocframe:
	dealloc_return
	.size __deallocframe, . - __deallocframe
"#
);
