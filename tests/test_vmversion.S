/*
 * Copyright (c) 2024 Taylor Simpson <ltaylorsimpson@gmail.com>
 * SPDX-License-Identifier: BSD-3-Clause
 */

#include "../hexagon_vm.h"

#define FD_STDOUT                1

.section .start,"awx",@progbits
.global _start
_start:
	r0 = ##GUEST_event_vectors
	vmsetvec

	// Request version 0xff, check that we get back 0x800
	r0 = #0xff
	vmversion
	{ p0 = cmp.eq(r0, ##0x800); if (!p0.new) jump:nt _fail }

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
	jump GUEST_event_abort           // 0: reserved`
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

GUEST_event_trap0:
	vmrte

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
