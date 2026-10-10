/*
 * Copyright (c) Qualcomm Technologies, Inc. and/or its subsidiaries.
 * SPDX-License-Identifier: BSD-3-Clause-Clear
 */

/*
 * Exception vector table and startup code.
 *
 * The vector table occupies the first 64 bytes (16 x 4-byte jump slots)
 * starting at _start, which must be 4K-aligned.  After the table,
 * .Lpast_vectors sets up the stack, GP, EVB, and calls minivm_main.
 */

	.section .start, "ax", @progbits
	.globl _start
	.type _start, @function
	.p2align 12
_start:
	/* Skip past the exception vector table */
	jump .Lpast_vectors
	.p2align 2

	/* Minimal exception vector table */
	/* Vector 1 (NMI) at offset 0x04 */
	jump .Ltrap_hang
	/* Vector 2 (error) at offset 0x08 */
	jump .Lerror_handler
	/* Vector 3 (reserved) at offset 0x0C */
	jump .Ltrap_hang
	/* Vector 4 (TLB miss X) at offset 0x10 */
	jump .Ltlbmiss_handler
	/* Vector 5 (reserved) at offset 0x14 */
	jump .Ltrap_hang
	/* Vector 6 (TLB miss RW) at offset 0x18 */
	jump .Ltlbmiss_handler
	/* Vector 7 (INT - hardware interrupt) at offset 0x1C */
	jump .Lint_handler
	/* Vector 8 (trap0) at offset 0x20 */
	jump .Ltrap0_handler
	/* Vector 9 (trap1) at offset 0x24 */
	jump .Ltrap1_handler
	/* Padding vectors 10-15 */
	jump .Ltrap_hang
	jump .Ltrap_hang
	jump .Ltrap_hang
	jump .Ltrap_hang
	jump .Ltrap_hang
	jump .Ltrap_hang

.Ltrap0_handler:
	crswap(r0, sgp0)
	memd(r0+#88) = r31:30
	call minivm_context_save
	r0 = sgp0
	r8 = memw(r0+#80)
	r8 = and(r8, #0xFF)
	p0 = cmp.eq(r8, #28)
	if (p0) jump .Ltrap0_vmop
	p0 = cmp.eq(r8, #30)
	if (p0) jump .Ltrap0_config
	call minivm_trap0_semihost_handler
	jump minivm_context_restore_rte

.Ltrap0_vmop:
	call minivm_trap0_vmop_handler
	r7 = sgp0
	memw(r7+#128) = r0
	jump minivm_context_restore_rte

.Ltrap0_config:
	call minivm_trap0_config_handler
	r7 = sgp0
	memw(r7+#128) = r0
	jump minivm_context_restore_rte

.Ltrap1_handler:
	crswap(r0, sgp0)
	memd(r0+#88) = r31:30
	call minivm_context_save
	r0 = sgp0
	r8 = memw(r0+#80)
	r1 = memw(r0+#128)
	p0 = tstbit(r8, #19)
	if (p0) jump .Ltrap1_guest_dispatch
	r0 = r1
	call minivm_trap1_handler
	r7 = sgp0
	memw(r7+#128) = r0
	jump minivm_context_restore_rte
.Ltrap1_guest_dispatch:
	r1 = and(r8, #0x1f)
	call minivm_guest_trap1_handler
	jump minivm_context_restore_rte

.Lerror_handler:
	crswap(r0, sgp0)
	memd(r0+#88) = r31:30
	call minivm_context_save
	r0 = sgp0
	r1 = badva
	r2 = memw(r0+#80)
	r2 = and(r2, #0xFF)
	call minivm_error_handler
	jump minivm_context_restore_rte

.Ltlbmiss_handler:
	crswap(r0, sgp0)
	memd(r0+#88) = r31:30
	call minivm_context_save
	r0 = sgp0
	r1 = badva
	call minivm_tlb_miss_handler
	jump minivm_context_restore_rte

.Lint_handler:
	crswap(r0, sgp0)
	memd(r0+#88) = r31:30
	call minivm_context_save
	call minivm_int_handler
	jump minivm_context_restore_rte

.Ltrap_hang:
	nop
	jump .Ltrap_hang

	.p2align 4
.Lpast_vectors:
	r17:16 = combine(r1, r0)
	r29 = ##__stack_end
	r28 = ##__global_pointer
	r30 = #0
	r31 = #0
	r0 = ##_start
	evb = r0
	r0 = #0
	ssr = r0
	isync

	/* Zero the BSS section */
	r0 = ##__bss_start
	r1 = ##__bss_end
	r2 = #0
.Lbss_loop:
	p0 = cmp.gtu(r1, r0)
	if (!p0) jump .Lbss_done
	memw(r0++#4) = r2
	jump .Lbss_loop
.Lbss_done:

	r1:0 = combine(r17, r16)
	call minivm_main
	trap0(#0)
	.size _start, . - _start
