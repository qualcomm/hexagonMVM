/*
 * Copyright (c) Qualcomm Technologies, Inc. and/or its subsidiaries.
 * SPDX-License-Identifier: BSD-3-Clause-Clear
 */

/*
 * The simple virtual instructions and events, checked through the virtual
 * instructions.
 */

#include "hvm.h"

#define CAUSE_PRIV 0x1b
#define CAUSE_MISALIGNED_LOAD 0x20
#define CAUSE_MISALIGNED_STORE 0x21

extern char hvm_trap1_table[];
void hvm_user_resume(void);
u32 hvm_load_w(u32 addr);
void hvm_load_w_resume(void);
void hvm_store_w(u32 addr, u32 v);
void hvm_store_w_resume(void);
void hvm_trap0_9(void);
void hvm_trap0_9_next(void);

static int in_stack(u32 sp, int i)
{
	u32 top = (u32)STACK_TOP(i);
	return sp <= top && sp > top - STACK_SIZE;
}

/* What the trap0 handler saw. */
static volatile u32 ie_in_handler, gosp_in_handler;
static int hook(u32 kind, u32 gsr, u32 gelr, u32 gbadva)
{
	if (kind == EV_TRAP0 && GSR_CAUSE(gsr) == 9) {
		ie_in_handler = hvm_getie();
		gosp_in_handler = hvm_rd_gosp();
	}
	return 0;
}

static u32 user_trap1(u32 n)
{
	hvm_trap1_n(n);
	return 0x5555;
}

static u32 user_trap0(u32 arg)
{
	u32 sp;
	hvm_trap0_9();
	__asm__ __volatile__("%0 = r29" : "=r"(sp));
	return in_stack(sp, 1) ? arg : 0xbad;
}

static u32 user_misaligned(u32 addr)
{
	/* Faults, and resumes here in user mode with r0 = addr. */
	return hvm_load_w(addr);
}

static u32 user_greg(u32 unused)
{
	return hvm_rd_gelr();
}

static u32 events(int kind)
{
	return last_event[kind].count;
}

int main(void)
{
	u32 n, before;
	event_hook = hook;

	puts_("version\n");
	CHECK("vmversion", hvm_version(), 0x800);

	puts_("setvec\n");
	extern char hvm_vectors[];
	CHECK("vmsetvec returns 0", hvm_setvec(hvm_vectors), 0);

	puts_("setie/getie\n");
	CHECK("starts off", hvm_getie(), 0);
	CHECK("setie 0/0", hvm_setie(0), 0);
	CHECK("getie", hvm_getie(), 0);
	CHECK("setie 1/0", hvm_setie(1), 0);
	CHECK("getie", hvm_getie(), 1);
	CHECK("setie 1/1", hvm_setie(1), 1);
	CHECK("setie 0/1", hvm_setie(0), 1);
	CHECK("getie", hvm_getie(), 0);
	CHECK("setie 2 means 0", hvm_setie(2), 0);
	CHECK("getie", hvm_getie(), 0);

	puts_("setregs/getregs\n");
	u32 r[4];
	hvm_setregs(0x89abcdef, 0x01234567, 0x76543210, 0xfedcba98);
	CHECK("set/gelr", hvm_rd_gelr(), 0x89abcdef);
	CHECK("set/gsr", hvm_rd_gsr(), 0x01234567);
	CHECK("set/gosp", hvm_rd_gosp(), 0x76543210);
	CHECK("set/gbadva", hvm_rd_gbadva(), 0xfedcba98);
	hvm_wr_gelr(0x11111111);
	hvm_wr_gsr(0x22222222);
	hvm_wr_gosp(0x33333333);
	hvm_wr_gbadva(0x44444444);
	hvm_getregs(r);
	CHECK("get/gelr", r[0], 0x11111111);
	CHECK("get/gsr", r[1], 0x22222222);
	CHECK("get/gosp", r[2], 0x33333333);
	CHECK("get/gbadva", r[3], 0x44444444);

	puts_("vpid/yield\n");
	n = hvm_vpid();
	CHECK("vpid vm index", n >> 26, 2);
	CHECK("vpid stable", hvm_vpid(), n);
	CHECK("vmyield", hvm_yield(), 0);

	puts_("gettime/settime\n");
	u64 t0 = hvm_gettime(), t1 = hvm_gettime();
	if (t1 < t0)
		fail("time went backwards", (u32)t1, (u32)t0);
	u64 big = (u64)0x12345 << 32;
	hvm_settime(big);
	t1 = hvm_gettime();
	if (t1 < big || t1 - big > 0xffffffffull)
		fail("settime: gettime high word", (u32)(t1 >> 32), 0x12345);

	puts_("vmcache\n");
	static u32 buf[64];
	for (n = 0; n <= 5; n++)
		CHECK("vmcache op", hvm_cache(n, (u32)buf, sizeof(buf)), 0);
	CHECK("vmcache op 6", hvm_cache(6, (u32)buf, sizeof(buf)), -1);
	CHECK("vmcache op 7", hvm_cache(7, (u32)buf, sizeof(buf)), -1);

	/* vmevent: trap0 from kernel mode. */
	puts_("trap0, kernel, ie off\n");
	before = events(EV_TRAP0);
	hvm_trap0_9();
	CHECK("trap0 event", events(EV_TRAP0), before + 1);
	CHECK("trap0 gsr", last_event[EV_TRAP0].gsr, 9);
	CHECK("trap0 gelr", last_event[EV_TRAP0].gelr, (u32)hvm_trap0_9_next);
	CHECK("trap0 stack", in_stack(last_event[EV_TRAP0].sp, 0), 1);

	puts_("trap0, kernel, ie on\n");
	hvm_setie(1);
	hvm_trap0_9();
	CHECK("trap0 gsr ie", last_event[EV_TRAP0].gsr, GSR_IE | 9);
	CHECK("ie off in handler", ie_in_handler, 0);
	CHECK("ie on after vmrte", hvm_getie(), 1);
	hvm_setie(0);

	puts_("trap0, user\n");
	before = events(EV_TRAP0);
	CHECK("user trap0 returns", hvm_run_user(user_trap0, 0x77, STACK_TOP(1)), 0x77);
	/* trap0 #0x7f brought it back. */
	CHECK("user trap0 events", events(EV_TRAP0), before + 2);
	CHECK("user trap0 gsr", last_event[EV_TRAP0].gsr, GSR_UM | 0x7f);
	CHECK("user handler on kernel stack", in_stack(last_event[EV_TRAP0].sp, 0), 1);
	CHECK("user sp in gosp", in_stack(gosp_in_handler, 1), 1);

	/* Misaligned accesses: GBADVA is the address, GELR the packet. */
	puts_("error, kernel\n");
	before = events(EV_ERROR);
	resume_at = (u32)hvm_load_w_resume;
	CHECK("misaligned load resumes", hvm_load_w((u32)buf + 2), (u32)buf + 2);
	CHECK("load event", events(EV_ERROR), before + 1);
	CHECK("load gsr", last_event[EV_ERROR].gsr, CAUSE_MISALIGNED_LOAD);
	CHECK("load gbadva", last_event[EV_ERROR].gbadva, (u32)buf + 2);
	CHECK("load gelr", last_event[EV_ERROR].gelr, (u32)hvm_load_w);
	resume_at = (u32)hvm_store_w_resume;
	hvm_store_w((u32)buf + 1, 0);
	CHECK("store gsr", last_event[EV_ERROR].gsr, CAUSE_MISALIGNED_STORE);
	CHECK("store gbadva", last_event[EV_ERROR].gbadva, (u32)buf + 1);
	hvm_setie(1);
	resume_at = (u32)hvm_load_w_resume;
	hvm_load_w((u32)buf + 3);
	CHECK("load gsr ie", last_event[EV_ERROR].gsr, GSR_IE | CAUSE_MISALIGNED_LOAD);
	CHECK("ie on after", hvm_getie(), 1);
	hvm_setie(0);

	puts_("error, user\n");
	resume_at = (u32)hvm_load_w_resume;
	CHECK("user misaligned resumes in user mode",
	      hvm_run_user(user_misaligned, (u32)buf + 2, STACK_TOP(1)), (u32)buf + 2);
	CHECK("user load gsr", last_event[EV_ERROR].gsr, GSR_UM | CAUSE_MISALIGNED_LOAD);
	CHECK("user load gbadva", last_event[EV_ERROR].gbadva, (u32)buf + 2);
	CHECK("user error on kernel stack", in_stack(last_event[EV_ERROR].sp, 0), 1);

	puts_("guest register from user mode\n");
	resume_at = (u32)hvm_user_resume;
	before = events(EV_ERROR);
	hvm_run_user(user_greg, 0, STACK_TOP(1));
	CHECK("greg from user: event", events(EV_ERROR), before + 1);
	/* The hardware's cause for a guest instruction in user mode, passed
	   on unchanged (the HVM spec has only 0x1b for both). */
	CHECK("greg from user: gsr", last_event[EV_ERROR].gsr, GSR_UM | 0x1a);

	/* vmtrap: every trap1 from user mode is a privilege error. */
	puts_("trap1 from user mode\n");
	for (n = 0; n < 32; n++) {
		before = events(EV_ERROR);
		resume_at = (u32)hvm_user_resume;
		hvm_user_result = 0;
		CHECK("user trap1 did not return", hvm_run_user(user_trap1, n, STACK_TOP(1)), 0);
		if (events(EV_ERROR) != before + 1)
			fail("user trap1: no error event", n, 0);
		CHECK("user trap1 gsr", last_event[EV_ERROR].gsr, GSR_UM | CAUSE_PRIV);
		CHECK("user trap1 on kernel stack", in_stack(last_event[EV_ERROR].sp, 0), 1);
	}

	/* vmtrap testvals: these numbers are not virtual instructions. */
	puts_("unassigned trap1 numbers\n");
	static const u32 bad[] = {6, 7, 8, 9, 12, 23, 27, 28, 29, 30, 31};
	for (u32 i = 0; i < sizeof(bad) / sizeof(bad[0]); i++) {
		n = bad[i];
		before = events(EV_ERROR);
		resume_at = (u32)hvm_trap1_table + n * 8 + 4;
		hvm_trap1_n(n);
		if (events(EV_ERROR) != before + 1) {
			resume_at = 0;
			fail("unassigned trap1: no error event", n, 0);
		}
		CHECK("unassigned trap1 gsr", last_event[EV_ERROR].gsr, CAUSE_PRIV);
	}
	return 0;
}
