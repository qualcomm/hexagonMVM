/*
 * Copyright (c) Qualcomm Technologies, Inc. and/or its subsidiaries.
 * SPDX-License-Identifier: BSD-3-Clause-Clear
 */

/*
 * Exception vector table, startup code and the idle loop.
 *
 * The vector table is the first 32 words of the image: the core jumps to
 * EVB + 4*event, and events 16..31 are the sixteen core interrupts.
 *
 * Every hardware thread arrives at the reset vector:
 *
 *   - The boot thread is entered by the machine at the image's physical
 *     address with the MMU off and the FDT pointer in r1:r0. It wires the
 *     monitor's mapping, turns the MMU on and continues at the link address.
 *
 *   - Every other thread is started later by `start`, which resets it to
 *     EVB. By then EVB holds the link address and the MMU is on (SYSCFG is
 *     shared by all threads), which is how the two cases are told apart.
 *
 * Wired TLB entries (see `tlb` in mon.rs for the rest):
 *   [0] the monitor image, 1M at MINIVM_LINK_ADDR
 *   [1] the boot thread's identity mapping, removed once it runs at the
 *       link address
 */

	.equ BOOT_STACK_SIZE, 16384
	.equ SYSCFG_ID, 0x6		/* L1 instruction and data caches */
	.equ MAX_HTHREADS, 16

	.section .bss
	.p2align 3
	.globl minivm_boot_stacks
	.type minivm_boot_stacks, @object
minivm_boot_stacks:
	.space BOOT_STACK_SIZE * MAX_HTHREADS
	.size minivm_boot_stacks, BOOT_STACK_SIZE * MAX_HTHREADS

	/* Receives the registers of whatever traps before a thread has a
	   context of its own (semihosting during early boot). */
	.globl minivm_boot_ctx
	.type minivm_boot_ctx, @object
minivm_boot_ctx:
	.space 320 * MAX_HTHREADS
	.size minivm_boot_ctx, 320 * MAX_HTHREADS

/*
 * Save the interrupted context into the ThreadContext SGP0 points at,
 * switch to this thread's exception stack and call a Rust handler with
 * that context in r0. The handler picks what runs next by leaving SGP0
 * pointing at it.
 */
	.macro EVENT handler
	crswap(r0, sgp0)
	memd(r0+#88) = r31:30
	call minivm_context_save
	r0 = sgp0
	call \handler
	jump minivm_context_restore_rte
	.endm

	.section .start, "ax", @progbits
	.globl _start
	.type _start, @function
	.p2align 12
_start:
	jump .Lreset			/*  0: reset */
	jump .Lunexpected		/*  1: NMI */
	jump .Lerror			/*  2: error */
	jump .Lunexpected		/*  3 */
	jump .Ltlbmiss			/*  4: TLB miss, execute */
	jump .Lunexpected		/*  5 */
	jump .Ltlbmiss			/*  6: TLB miss, read/write */
	jump .Lunexpected		/*  7 */
	jump .Ltrap0			/*  8: trap0 */
	jump .Ltrap1			/*  9: trap1 */
	jump .Lunexpected		/* 10 */
	jump .Lunexpected		/* 11 */
	jump .Lunexpected		/* 12 */
	jump .Lunexpected		/* 13 */
	jump .Lunexpected		/* 14 */
	jump .Lunexpected		/* 15 */
	.rept 16
	jump .Linterrupt		/* 16..31: core interrupts */
	.endr

.Ltrap0:
	EVENT minivm_trap0
.Ltrap1:
	EVENT minivm_trap1
.Lerror:
	EVENT minivm_error
.Ltlbmiss:
	EVENT minivm_tlbmiss
.Linterrupt:
	EVENT minivm_interrupt
.Lunexpected:
	EVENT minivm_unexpected

.Lreset:
	r2 = syscfg
	p0 = tstbit(r2, #0)
	if (p0) jump .Lsecondary

	/*
	 * Boot thread: MMU off, running at the load address. Nothing below
	 * may use an absolute address until the jump to .Lvirtual.
	 */
	r17:16 = combine(r1, r0)
	r18 = add(pc, ##_start@PCREL)

	/* Out of exception mode: hexagon-sim, for one, enters with SSR.EX
	   set, and the boot path traps (semihosting). */
	r2 = #0
	ssr = r2
	isync

	/*
	 * Preserve each cache the loader left on, because it may hold the
	 * loader's writes. Invalidate each one it left off before enabling or
	 * sizing it; its contents are undefined. Handle the caches independently:
	 * loaders need not leave them in the same state.
	 */
	r2 = syscfg
	p0 = tstbit(r2, #1)		/* I */
	if (p0) jump .Licache_done
	jump .Licache_kill
	.p2align 5
.Licache_kill:
	ickill
	r2 = setbit(r2, #1)
.Licache_done:
	p0 = tstbit(r2, #2)		/* D */
	if (p0) jump .Ldcache_done
	jump .Ldcache_kill
	.p2align 5
.Ldcache_kill:
	dckill
	r2 = setbit(r2, #2)
.Ldcache_done:
	r3 = extractu(r2, #3, #16)	/* L2CFG */
	p0 = cmp.eq(r3, #0)
	if (!p0) jump .Ll2cache_done
	jump .Ll2cache_kill
	.p2align 5
.Ll2cache_kill:
	l2kill
.Ll2cache_done:
	syscfg = r2
	isync

	/* [0]: link address -> load address, 1M, RWX, global, cached. */
	r0 = lsr(r18, #11)
	r0 = or(r0, ##0xE7000010)
	r1 = ##0xC00FF000
	r2 = #0
	tlbw(r1:0, r2)

	/* [1]: the same page at its own address, to survive turning the
	   MMU on. */
	r1 = lsr(r18, #12)
	r1 = or(r1, ##0xC0000000)
	r2 = #1
	tlbw(r1:0, r2)
	isync

	r2 = syscfg
	r2 = setbit(r2, #0)
	syscfg = r2
	isync

	r2 = ##.Lvirtual
	jumpr r2

.Lvirtual:
	r1:0 = combine(#0, #0)
	r2 = #1
	tlbw(r1:0, r2)
	isync

	r0 = ##_start
	evb = r0

	r0 = ##__bss_start
	r1 = ##__bss_end
	r2 = #0
.Lbss_loop:
	p0 = cmp.gtu(r1, r0)
	if (!p0) jump .Lbss_done
	memw(r0++#4) = r2
	jump .Lbss_loop
.Lbss_done:

	call .Lthread_setup
	r1:0 = combine(r17, r16)
	r2 = r18
	call minivm_main
	jump minivm_context_restore_rte

.Lsecondary:
	call .Lthread_setup
	call minivm_secondary_main
	jump minivm_context_restore_rte

/*
 * Give this thread a boot stack and a context to trap into. Clobbers
 * r0-r3; returns to r31.
 */
.Lthread_setup:
	r0 = htid
	r1 = ##BOOT_STACK_SIZE
	r2 = add(r0, #1)
	r29 = ##minivm_boot_stacks
	r29 += mpyi(r2, r1)
	r1 = #320
	r3 = ##minivm_boot_ctx
	r3 += mpyi(r0, r1)
	sgp0 = r3
	r30 = #0
	jumpr r31
	.size _start, . - _start

/*
 * What a hardware thread runs when it has no virtual CPU to run: wait for
 * an interrupt. It runs in monitor mode with interrupts enabled, under a
 * context of its own, so the interrupt that ends the wait is taken through
 * the ordinary vector and saves nothing of value.
 *
 * QEMU also resumes a waiting thread when an interrupt is pending that the
 * thread cannot take, hence the loop.
 */
	.section .text, "ax", @progbits
	.globl minivm_idle_loop
	.type minivm_idle_loop, @function
	.p2align 2
minivm_idle_loop:
	r0 = #0
	wait(r0)
	jump minivm_idle_loop
	.size minivm_idle_loop, . - minivm_idle_loop

/* Take this hardware thread off line. Does not return. */
	.globl minivm_stop_thread
	.type minivm_stop_thread, @function
	.p2align 2
minivm_stop_thread:
	r2 = r0
	stop(r0)
	jump minivm_stop_thread
	.size minivm_stop_thread, . - minivm_stop_thread
