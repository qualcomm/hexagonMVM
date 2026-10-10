/*
 * Copyright (c) Qualcomm Technologies, Inc. and/or its subsidiaries.
 * SPDX-License-Identifier: BSD-3-Clause-Clear
 */

/*
 * Context save/restore, thread switch, and TLB insert.
 *
 * ThreadContext field offsets must match context.rs exactly.
 */

.equ CTX_r3130,    88
.equ CTX_r2928,    96
.equ CTX_r1918,   104
.equ CTX_r1716,   112
.equ CTX_usrp30,  120
.equ CTX_r0100,   128
.equ CTX_r1514,   136
.equ CTX_r1312,   144
.equ CTX_r1110,   152
.equ CTX_r0908,   160
.equ CTX_r0706,   168
.equ CTX_r0504,   176
.equ CTX_r0302,   184
.equ CTX_lc0sa0,  192
.equ CTX_gpugp,   200
.equ CTX_lc1sa1,  208
.equ CTX_m1m0,    216
.equ CTX_r2726,   224
.equ CTX_r2524,   232
.equ CTX_r2322,   240
.equ CTX_r2120,   248
.equ CTX_GELR,    256
.equ CTX_GSSR,    260
.equ CTX_GOSP,    264
.equ CTX_GBADVA,  268
.equ CTX_cs1cs0,  272
.equ CTX_SSR,      80
.equ CTX_CCR,      84
.equ CTX_ELR,      28

	.section .text, "ax", @progbits
	.globl minivm_context_save
	.type minivm_context_save, @function
	.p2align 2
minivm_context_save:
	memd(r0+#96) = r29:28
	memd(r0+#104) = r19:18
	memd(r0+#112) = r17:16
	memd(r0+#136) = r15:14
	memd(r0+#144) = r13:12
	memd(r0+#152) = r11:10
	memd(r0+#160) = r9:8
	memd(r0+#168) = r7:6
	memd(r0+#176) = r5:4
	memd(r0+#184) = r3:2
	memd(r0+#224) = r27:26
	memd(r0+#232) = r25:24
	memd(r0+#240) = r23:22
	memd(r0+#248) = r21:20
	r7 = r0
	crswap(r0,sgp0)
	memd(r7+#128) = r1:0
	r15 = usr
	r14 = p3:0
	r19 = lc0
	r18 = sa0
	r17 = lc1
	r16 = sa1
	r23 = gp
	r22 = ugp
	r13 = ssr
	r12 = elr
	r11 = ccr
	r21 = cs1
	r20 = cs0
	r9 = m1
	r8 = m0
	/* Preserve SSR.UM (bit 16) from old ctx — hardware clears UM on
	   exception entry, but the bit tracks the guest's virtual user mode.
	   r14 (guest p3:0) is already captured, so we can freely use predicates. */
	r15 = memw(r7+#80)
	p0 = tstbit(r15, #16)
	r15 = setbit(r13, #16)
	if (p0) r13 = r15
	r15 = usr
	memd(r7+#120) = r15:14
	memd(r7+#192) = r19:18
	memd(r7+#208) = r17:16
	memd(r7+#200) = r23:22
	memd(r7+#216) = r9:8
	memw(r7+#80) = r13
	memw(r7+#28) = r12
	memw(r7+#84) = r11
	memd(r7+#272) = r21:20
	r11:10 = g1:0
	r9:8 = g3:2
	memd(r7+#256) = r11:10
	memd(r7+#264) = r9:8
	r29 = ##__stack_end
	r28 = ##__global_pointer
	jumpr r31
	.size minivm_context_save, . - minivm_context_save

	.globl minivm_context_restore_rte
	.type minivm_context_restore_rte, @function
	.p2align 2
minivm_context_restore_rte:
	r7 = sgp0
	r11 = memw(r7+#84)
	r21:20 = memd(r7+#272)
	ccr = r11
	cs0 = r20
	cs1 = r21
	r12 = memw(r7+#80)
	r13 = memw(r7+#28)
	r19:18 = memd(r7+#192)
	r17:16 = memd(r7+#208)
	r23:22 = memd(r7+#200)
	r15:14 = memd(r7+#120)
	r9:8 = memd(r7+#216)
	ssr = r12
	elr = r13
	sa0 = r18
	lc0 = r19
	sa1 = r16
	lc1 = r17
	ugp = r22
	gp = r23
	p3:0 = r14
	usr = r15
	m0 = r8
	m1 = r9
	r11:10 = memd(r7+#256)
	r9:8 = memd(r7+#264)
	g1:0 = r11:10
	g3:2 = r9:8
	r1:0 = memd(r7+#128)
	crswap(r0,sgp0)
	r3:2 = memd(r0+#184)
	r5:4 = memd(r0+#176)
	r7:6 = memd(r0+#168)
	r9:8 = memd(r0+#160)
	r11:10 = memd(r0+#152)
	r13:12 = memd(r0+#144)
	r15:14 = memd(r0+#136)
	r17:16 = memd(r0+#112)
	r19:18 = memd(r0+#104)
	r21:20 = memd(r0+#248)
	r23:22 = memd(r0+#240)
	r25:24 = memd(r0+#232)
	r27:26 = memd(r0+#224)
	r29:28 = memd(r0+#96)
	r31:30 = memd(r0+#88)
	crswap(r0,sgp0)
	rte
	.size minivm_context_restore_rte, . - minivm_context_restore_rte

	.globl minivm_switch
	.type minivm_switch, @function
	.p2align 2
minivm_switch:
	p0 = cmp.eq(r0, #0)
	if (p0) jump .Lswitch_restore
	memd(r0+#88) = r31:30
	elr = r31
	sgp0 = r0
	r30 = r1
	call minivm_context_save
	r1 = r30
.Lswitch_restore:
	sgp0 = r1
	jump minivm_context_restore_rte
	.size minivm_switch, . - minivm_switch

	.globl minivm_tlb_insert
	.type minivm_tlb_insert, @function
	.p2align 2
minivm_tlb_insert:
	{{ tlbw(r1:0, r2) }}
	isync
	jumpr r31
	.size minivm_tlb_insert, . - minivm_tlb_insert
