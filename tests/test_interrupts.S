/*
 * Copyright (c) 2024 Taylor Simpson <ltaylorsimpson@gmail.com>
 * SPDX-License-Identifier: BSD-3-Clause
 */

#include "../hexagon_vm.h"

#define PERM_BITS		(0x7 << HVM_PTE_R_BIT)    // X:1 W:1 R:1
#define CACHE_BITS		(0x7 << HVM_PTE_CCC_OFF)
#define GUEST_PTE_BITS		(PERM_BITS | CACHE_BITS | HVM_PTE_PGSIZE_4M)

#define FOUR_MB_BITS		22
#define FOUR_MB			(1 << FOUR_MB_BITS)
#define FOUR_MB_MASK		(~(FOUR_MB - 1))

#define NUM_INTERRUPTS			16

#define CREATE_PTE_ENTRIES(START, END, BITS) \
	/* Number of entries between START and END */ \
	r26 = #(((END-START)+FOUR_MB-1)>>FOUR_MB_BITS); \
	r1 = #(START); \
	r1 = and(r1, #FOUR_MB_MASK); \
	r2 = lsr(r1, #FOUR_MB_BITS); \
	r0 = addasl(r24, r2, #2);	/* r0 = address of first PTE */ \
	r1 = add(r1, #(BITS));		/* r1 = 4MB PTE for the first entry */ \
	r2 = #FOUR_MB;			/* PTE increment */ \
	loop0(1f, r26); \
1: \
	memw(r0 ++ #4) = r1; \
	{ r1 = add(r1, r2) } :endloop0


#define FD_STDOUT                1

.section .start,"awx",@progbits
.global _start
_start:
	r29 = ##(stack + 32*4 - 8)
	r0 = #0xdeadbeef
	r1 = #0xcafebabe
	memd(r29) = r1:0

	// Get the physical address of guest_pt
	r25 = pc
	// Round down to the 4M-aligned image base: `pc` reads the address of
	// *this* packet, not _start, and a page-table base is only honoured to
	// page granularity.
	r25 = and(r25, ##0xffc00000)
	r24 = ##(guest_pt-_start)
	r24 = add(r24, r25)

	CREATE_PTE_ENTRIES(_start, _end_guest, GUEST_PTE_BITS)

	r0 = r24
	vmnewmap

	// Install our event vectors
	r0 = ##GUEST_event_vectors
	vmsetvec

	// Set interrupt enable and check that it's turned on
	r0 = #1
	vmsetie
	vmgetie
	{ p0 = cmp.eq(r0, #1); if (!p0.new) jump:nt _fail }

	// Post interrupt 13
	r0 = #HVM_INTOP_POST
	r2 = #0			// target: this virtual processor
	r1 = #13
	vmintop
	{ p0 = cmp.eq(r0, #0); if (!p0.new) jump:nt _fail }

	// Post interrupt 15
	r0 = #HVM_INTOP_POST
	r2 = #0			// target: this virtual processor
	r1 = #15
	vmintop
	{ p0 = cmp.eq(r0, #0); if (!p0.new) jump:nt _fail }

	// Posting only makes it pending: global:0/local:0/pend:1 (1). Per-CPU
	// interrupts have no local enable.
	r0 = #HVM_INTOP_STATUS
	r1 = #13
	vmintop
	{ p0 = cmp.eq(r0, #1); if (!p0.new) jump:nt _fail }

	// Nothing is enabled, so nothing is next
	r0 = #HVM_INTOP_PEEK
	vmintop
	{ p0 = cmp.eq(r0, #-1); if (!p0.new) jump:nt _fail }

	// Check not taken yet
	r0 = memw(##(interrupt_counts + 13*4))
	{ p0 = cmp.eq(r0, #0); if (!p0.new) jump:nt _fail }

	// Enable it, which delivers it
	r0 = #HVM_INTOP_GLOBEN
	r1 = #13
	vmintop
	{ p0 = cmp.eq(r0, #0); if (!p0.new) jump:nt _fail }

	// Check that it was taken
	r0 = memw(##(interrupt_counts + 13*4))
	{ p0 = cmp.eq(r0, #1); if (!p0.new) jump:nt _fail }

	// The handler enabled it again: global:1/local:0/pend:0 (4)
	r0 = #HVM_INTOP_STATUS
	r1 = #13
	vmintop
	{ p0 = cmp.eq(r0, #4); if (!p0.new) jump:nt _fail }

	// 15 is pending but not enabled, so still nothing is next
	r0 = #HVM_INTOP_PEEK
	vmintop
	{ p0 = cmp.eq(r0, #-1); if (!p0.new) jump:nt _fail }

	// Disable it
	r0 = #HVM_INTOP_GLOBDIS
	r1 = #13
	vmintop

	// Post interrupt 13
	r0 = #HVM_INTOP_POST
	r2 = #0			// target: this virtual processor
	r1 = #13
	vmintop
	{ p0 = cmp.eq(r0, #0); if (!p0.new) jump:nt _fail }

	// Check not taken yet
	r0 = memw(##(interrupt_counts + 13*4))
	{ p0 = cmp.eq(r0, #1); if (!p0.new) jump:nt _fail }

	// Status should be global:0/local:0/pend:1 (1)
	r0 = #HVM_INTOP_STATUS
	r1 = #13
	vmintop
	{ p0 = cmp.eq(r0, #1); if (!p0.new) jump:nt _fail }

	// Enable it again
	r0 = #HVM_INTOP_GLOBEN
	r1 = #13
	vmintop

	// Check that it was taken
	r0 = memw(##(interrupt_counts + 13*4))
	{ p0 = cmp.eq(r0, #2); if (!p0.new) jump:nt _fail }

	// Clear interrupt 15
	r0 = #HVM_INTOP_CLEAR
	r1 = #15
	vmintop
	{ p0 = cmp.eq(r0, #0); if (!p0.new) jump:nt _fail }

	// Status should be global:0/local:0/pend:0 (0)
	r0 = #HVM_INTOP_STATUS
	r1 = #15
	vmintop
	{ p0 = cmp.eq(r0, #0); if (!p0.new) jump:nt _fail }

	// Check that none are left
	r0 = #HVM_INTOP_PEEK
	vmintop
	{ p0 = cmp.eq(r0, #-1); if (!p0.new) jump:nt _fail }

	// Check not taken
	r0 = memw(##(interrupt_counts + 15*4))
	{ p0 = cmp.eq(r0, #0); if (!p0.new) jump:nt _fail }

	// Print the PASS message
	r0 = #5
	r1 = ##pass_args
	trap0(#0)

	// Successful test
	r2 = #0
	vmstop

_fail:
	// Print the FAIL message
	r0 = #5
	r1 = ##fail_args
	trap0(#0)

	r2 = #0xff		// return failure to shell
	vmstop

	.type	pass_str,@object
pass_str:
	.string	"PASS\n"
	.size	pass_str, 5
	.type	fail_str,@object
fail_str:
	.string	"FAIL\n"
	.size	fail_str, 5
	.type	pass_args,@object
pass_args:
	.word   FD_STDOUT
	.word   pass_str
	.word   5
	.type	fail_args,@object
fail_args:
	.word   FD_STDOUT
	.word   fail_str
	.word   5

.p2align 8
GUEST_event_vectors:
	jump GUEST_event_abort           // 0: reserved
	jump GUEST_event_abort           // 1: machine check
	jump GUEST_event_abort           // 2: general exception
	jump GUEST_event_abort           // 3: reserved
	jump GUEST_event_abort           // 4: reserved
	jump GUEST_event_trap0           // 5: trap0
	jump GUEST_event_abort           // 6: reserved
	jump GUEST_event_interrupt       // 7: interrupt


GUEST_event_abort:
	r2 = gsr		// return gsr value to shell
	vmstop

// Simple trap0 handler
//    Cause code 0 is an angel call - do nothing
//    Cause code 1 will exit (r0 has return code to shell)
GUEST_event_trap0:
	r29 = add(r29, #-8)
	memw(r29) = r10
	r10 = p3:0
	memw(r29+#4) = r10

	r10 = gsr
	r10 = zxth(r10)
	{
		p0 = cmp.eq(r10, #1)
		if (!p0.new) jump:nt 1f
	}
	r2 = r0
	vmstop
1:
	r10 = memw(r29+#4)
	p3:0 = r10
	r10 = memw(r29)
	r29 = add(r29, #8)
	vmrte

GUEST_event_interrupt:
	allocframe(#8)
	memd(r29) = r1:0
	r1 = gsr
	r1 = extractu(r1, #4, #0)
	r0 = ##interrupt_counts
	r0 = addasl(r0, r1, #2)
	memw(r0) += #1
	// Re-enable the interrupt (r1 has the number)
	r0 = #HVM_INTOP_GLOBEN
	vmintop
	r1:0 = memd(r29)
	deallocframe
	vmrte

	.align 4
interrupt_counts:
	.fill NUM_INTERRUPTS, 4, 0

	.align 8
stack:
	.fill 32, 4, 0

#define XLAT4M(VAL) \
	.word (VAL | HVM_PTE_PGSIZE_INVALID);

#define XLAT16M(VAL) \
	XLAT4M(VAL) \
	XLAT4M(VAL | 0x00400000) \
	XLAT4M(VAL | 0x00800000) \
	XLAT4M(VAL | 0x00c00000)

#define XLAT64M(VAL) \
	XLAT16M(VAL) \
	XLAT16M(VAL | 0x01000000) \
	XLAT16M(VAL | 0x02000000) \
	XLAT16M(VAL | 0x03000000)

#define XLAT256M(VAL) \
	XLAT64M(VAL) \
	XLAT64M(VAL | 0x04000000) \
	XLAT64M(VAL | 0x08000000) \
	XLAT64M(VAL | 0x0c000000)

	.p2align 12
guest_pt:
	XLAT256M(0x00000000)
	XLAT256M(0x10000000)
	XLAT256M(0x20000000)
	XLAT256M(0x30000000)
	XLAT256M(0x40000000)
	XLAT256M(0x50000000)
	XLAT256M(0x60000000)
	XLAT256M(0x70000000)
	XLAT256M(0x80000000)
	XLAT256M(0x90000000)
	XLAT256M(0xa0000000)
	XLAT256M(0xb0000000)
	XLAT256M(0xc0000000)
	XLAT256M(0xd0000000)
	XLAT256M(0xe0000000)
	XLAT256M(0xf0000000)
_end_guest:

.section ".note.ABI-tag", "a"
.align 4
.long 1f - 0f          /* name length */
.long 3f - 2f          /* data length */
.long  1               /* note type */

/*
 * vendor name seems like this should be MUSL but lldb doesn't agree.
 */
0:     .asciz "GNU"
1:     .align 4
2:     .long 0 /* linux */
       .long 3,0,0
3:     .align 4
