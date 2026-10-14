/*
 * Copyright (c) Qualcomm Technologies, Inc. and/or its subsidiaries.
 * SPDX-License-Identifier: BSD-3-Clause-Clear
 */

/*
 * Runtime for the C guest tests: entry, event vectors that call into C,
 * one wrapper function per virtual instruction, and semihosting.
 *
 * Every wrapper is an ordinary function, so the C ABI's caller-saved
 * registers cover whatever the monitor returns in r0-r3.
 */

	.equ STACK_SIZE, 16384
	.equ NSTACKS, 8

	.section .bss
	.p2align 3
	.globl hvm_stacks
hvm_stacks:
	.space STACK_SIZE * NSTACKS

	.section .start, "ax", @progbits
	.globl _start
_start:
	/* Memory starts out undefined (hexagon-sim does not clear it). */
	r0 = ##__bss_start
	r1 = ##__bss_end
	r2 = #0
1:	p0 = cmp.gtu(r1, r0)
	if (!p0) jump 2f
	memw(r0++#4) = r2
	jump 1b
2:	r29 = ##hvm_stacks + STACK_SIZE
	r30 = #0
	r0 = ##hvm_vectors
	trap1(#2)			/* vmsetvec */
	call main
	call hvm_finish
1:	jump 1b

/*
 * Event vectors. Each saves the caller-saved state on the current stack
 * (the kernel stack: an event from user mode has already switched to it),
 * calls hvm_event(kind, gsr, gelr, gbadva), restores and returns with
 * vmrte. hvm_event may redirect the return by writing GELR.
 */
	.p2align 6
	.globl hvm_vectors
hvm_vectors:
	jump ev_0
	jump ev_1
	jump ev_2
	jump ev_3
	jump ev_4
	jump ev_5
	jump ev_6
	jump ev_7

	.equ FRAME, 112

	.macro EV n
ev_\n:
	allocframe(#FRAME)
	memd(r29+#0) = r1:0
	r0 = #\n
	jump ev_common
	.endm

	EV 0
	EV 1
	EV 2
	EV 3
	EV 4
	EV 5
	EV 6
	EV 7

ev_common:
	memd(r29+#8) = r3:2
	memd(r29+#16) = r5:4
	memd(r29+#24) = r7:6
	memd(r29+#32) = r9:8
	memd(r29+#40) = r11:10
	memd(r29+#48) = r13:12
	memd(r29+#56) = r15:14
	r2 = p3:0
	r3 = r28
	memd(r29+#64) = r3:2
	r3:2 = c1:0			/* lc0:sa0 */
	memd(r29+#72) = r3:2
	r3:2 = c3:2			/* lc1:sa1 */
	memd(r29+#80) = r3:2
	r3:2 = c7:6			/* m1:m0 */
	memd(r29+#88) = r3:2
	r2 = usr
	memw(r29+#96) = r2

	r1 = gsr
	r2 = gelr
	r3 = gbadva
	call hvm_event

	r2 = memw(r29+#96)
	usr = r2
	r3:2 = memd(r29+#88)
	c7:6 = r3:2
	r3:2 = memd(r29+#80)
	c3:2 = r3:2
	r3:2 = memd(r29+#72)
	c1:0 = r3:2
	r3:2 = memd(r29+#64)
	p3:0 = r2
	r28 = r3
	r15:14 = memd(r29+#56)
	r13:12 = memd(r29+#48)
	r11:10 = memd(r29+#40)
	r9:8 = memd(r29+#32)
	r7:6 = memd(r29+#24)
	r5:4 = memd(r29+#16)
	r3:2 = memd(r29+#8)
	r1:0 = memd(r29+#0)
	deallocframe
	trap1(#1)			/* vmrte */

/* Guest registers, readable and writable from guest kernel mode. */
	.text
	.macro GETREG name, reg
	.globl \name
\name:
	r0 = \reg
	jumpr r31
	.endm
	.macro SETREG name, reg
	.globl \name
\name:
	\reg = r0
	jumpr r31
	.endm

	GETREG hvm_rd_gelr, gelr
	GETREG hvm_rd_gsr, gsr
	GETREG hvm_rd_gosp, gosp
	GETREG hvm_rd_gbadva, gbadva
	SETREG hvm_wr_gelr, gelr
	SETREG hvm_wr_gsr, gsr
	SETREG hvm_wr_gosp, gosp
	SETREG hvm_wr_gbadva, gbadva

/* One wrapper per virtual instruction: arguments in r0-r3, results in
   r0 (or r1:0). */
	.macro VMCALL name, num
	.globl \name
\name:
	trap1(#\num)
	jumpr r31
	.endm

	VMCALL hvm_version, 0
	VMCALL hvm_setvec, 2
	VMCALL hvm_setie, 3
	VMCALL hvm_getie, 4
	VMCALL hvm_intop, 5
	VMCALL hvm_clrmap, 10
	VMCALL hvm_newmap, 11
	VMCALL hvm_cache, 13
	VMCALL hvm_gettime, 14
	VMCALL hvm_settime, 15
	VMCALL hvm_wait, 16
	VMCALL hvm_yield, 17
	VMCALL hvm_start, 18
	VMCALL hvm_stop, 19
	VMCALL hvm_vpid, 20
	VMCALL hvm_setregs, 21
	VMCALL hvm_timerop, 24
	VMCALL hvm_pmuconfig, 25
	VMCALL hvm_getinfo, 26

/* vmgetregs: r0-r3 are the results, stored through the pointer. */
	.globl hvm_getregs
hvm_getregs:
	r4 = r0
	trap1(#22)
	memw(r4+#0) = r0
	memw(r4+#4) = r1
	memw(r4+#8) = r2
	memw(r4+#12) = r3
	jumpr r31

/*
 * Call trap1 #n, for n from 0 to 31, with no arguments set up. Only for
 * numbers that are not valid virtual instructions, or are harmless.
 */
	.globl hvm_trap1_n
hvm_trap1_n:
	r1 = ##hvm_trap1_table
	r1 = addasl(r1, r0, #3)
	jumpr r1
	.p2align 3
	.globl hvm_trap1_table
hvm_trap1_table:
	.irp n, 0,1,2,3,4,5,6,7,8,9,10,11,12,13,14,15,16,17,18,19,20,21,22,23,24,25,26,27,28,29,30,31
	{ trap1(#\n) }
	jumpr r31
	.endr

/* Same for trap0. */
	.globl hvm_trap0_n
hvm_trap0_n:
	r1 = ##trap0_table
	r1 = addasl(r1, r0, #3)
	jumpr r1
	.p2align 3
trap0_table:
	.irp n, 1,2,3,4,5,6,7,8
	{ trap0(#\n) }
	jumpr r31
	.endr

/* A load and a store whose fault can be resumed at the matching
   *_resume label, with r0 unchanged. */
	.globl hvm_load_w, hvm_load_w_resume
hvm_load_w:
	r0 = memw(r0)
hvm_load_w_resume:
	jumpr r31

	.globl hvm_store_w, hvm_store_w_resume
hvm_store_w:
	memw(r0) = r1
hvm_store_w_resume:
	jumpr r31

/* trap1 #6, an unassigned number, and where it resumes. */
	.globl hvm_bad_trap1, hvm_bad_trap1_resume
hvm_bad_trap1:
	trap1(#6)
hvm_bad_trap1_resume:
	jumpr r31

/* trap0 #9 with a known address for the following packet. */
	.globl hvm_trap0_9, hvm_trap0_9_next
hvm_trap0_9:
	trap0(#9)
hvm_trap0_9_next:
	jumpr r31

/* Semihosting: write a string; shut the machine down. */
	/* SYS_WRITE(1, s, strlen(s)): hexagon-sim has no SYS_WRITE0. */
	.globl sh_write0
sh_write0:
	r2 = r0
1:	r3 = memub(r2)
	{ p0 = cmp.eq(r3, #0); if (p0.new) jump:nt 2f }
	{ r2 = add(r2, #1); jump 1b }
2:	r2 = sub(r2, r0)
	allocframe(#16)
	r1 = #1
	memw(r29+#0) = r1
	memw(r29+#4) = r0
	memw(r29+#8) = r2
	r1 = r29
	r0 = #5
	trap0(#0)
	dealloc_return

	.globl sh_shutdown
sh_shutdown:
	r0 = #0x18
	r1 = #0
	trap0(#0)
1:	jump 1b

/*
 * u32 hvm_run_user(fn, arg, usp): run fn(arg) in user mode on stack usp.
 * When fn returns, user_done hands its result back with trap0(#0x7f); the
 * event handler sees that and resumes at hvm_user_resume in kernel mode.
 */
	.globl hvm_run_user
hvm_run_user:
	allocframe(#16)
	memd(r29+#0) = r17:16
	r16 = r0			/* fn */
	r17 = r2			/* user sp */
	r0 = r1				/* arg, live in r0 at user entry */
	r31 = ##user_done
	gelr = r16
	gosp = r17
	r2 = ##0x80000000		/* GSR.UM, interrupts off */
	gsr = r2
	trap1(#1)			/* vmrte into user mode */
	.globl hvm_user_resume
hvm_user_resume:
	/* The event frame restored the user's r30; rebuild ours. */
	r30 = add(r29, #16)
	r17:16 = memd(r29+#0)
	r0 = memw(##hvm_user_result)
	dealloc_return

user_done:
	r1 = ##hvm_user_result
	memw(r1) = r0
	trap0(#0x7f)
1:	jump 1b

	.section .bss
	.p2align 2
	.globl hvm_user_result
hvm_user_result:
	.word 0
