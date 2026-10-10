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

#define NUM_PROCESSORS			6
#define STACK_SIZE			1024

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
	r29 = ##(stack0 + STACK_SIZE - 8)
	r0 = #0xdeadbeef
	r1 = #0xcafebabe
	memd(r29) = r1:0

	// Get the physical address of guest_pt
	r25 = pc
	r24 = ##(guest_pt-_start)
	r24 = add(r24, r25)

	CREATE_PTE_ENTRIES(_start, _end_guest, GUEST_PTE_BITS)

	r0 = r24
	vmnewmap

	// Install our event vectors
	r0 = ##GUEST_event_vectors
	vmsetvec

	r1:0 = #0xff
	r3:2 = #0xff
	vmsettime
	vmgettime
	{ p0 = cmp.gtu(r1:0, r3:2); if (!p0.new) jump:nt _fail }

	// Start a new virtual processor
	r1 = ##(stack1 + STACK_SIZE - 8)
	r2 = #0xfafbfcfd
	r3 = #0xabcdef12
	memd(r1) = r3:2
	r0 = ##_proc1
	vmstart
	{ p0 = cmp.eq(r0, #-1); if (p0.new) jump:nt _fail }
	// Save the vpid of the new processor
	r11 = r0

	vmwait

	// Wait for other processor write its vpid
	r1 = ##(stack1 + STACK_SIZE - 8)
1:
	r2 = memw(r1)
	p0 = cmp.eq(r2, r11)
	if (!p0) jump 1b

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

_proc1:
	vmyield
	vmvpid
	memw(r29) = r0
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
	jump GUEST_event_abort           // 7: interrupt


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

	.align 8
stack0:
	.fill STACK_SIZE, 1, 0
stack1:
	.fill STACK_SIZE, 1, 0
stack2:
	.fill STACK_SIZE, 1, 0
stack3:
	.fill STACK_SIZE, 1, 0
stack4:
	.fill STACK_SIZE, 1, 0
stack5:
	.fill STACK_SIZE, 1, 0

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
