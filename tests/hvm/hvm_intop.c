/*
 * Copyright (c) Qualcomm Technologies, Inc. and/or its subsidiaries.
 * SPDX-License-Identifier: BSD-3-Clause-Clear
 */

/*
 * vmintop: every operation, and delivery as an event. Per-CPU interrupts
 * are 0..15, shared ones 16 and up.
 */

#include "hvm.h"

#define BAD_INT 1026

static s32 op(u32 o, u32 n) { return hvm_intop(o, n, 0); }
static u32 status(u32 n) { return (u32)op(INTOP_STATUS, n); }

#define ST_PENDING 1
#define ST_LOCAL 2
#define ST_GLOBAL 4

static volatile u32 seen[64];
static int hook(u32 kind, u32 gsr, u32 gelr, u32 gbadva)
{
	if (kind == EV_INTERRUPT)
		seen[GSR_CAUSE(gsr) & 63]++;
	return 0;
}

int main(void)
{
	u32 me = hvm_vpid();
	event_hook = hook;

	puts_("A: nop\n");
	CHECK("nop", op(INTOP_NOP, 0), 0);

	puts_("B: globen\n");
	CHECK("globen 0", op(INTOP_GLOBEN, 0), 0);
	CHECK("globen 0 status", status(0) & ST_GLOBAL, ST_GLOBAL);
	CHECK("globen 1", op(INTOP_GLOBEN, 1), 0);
	CHECK("globen 1 status", status(1) & ST_GLOBAL, ST_GLOBAL);
	CHECK("globen 16", op(INTOP_GLOBEN, 16), 0);
	CHECK("globen 16 status", status(16) & ST_GLOBAL, ST_GLOBAL);
	CHECK("globen 17", op(INTOP_GLOBEN, 17), 0);
	CHECK("globen oob", op(INTOP_GLOBEN, BAD_INT), -1);

	puts_("C: globdis\n");
	CHECK("globdis 0", op(INTOP_GLOBDIS, 0), 0);
	CHECK("globdis 0 status", status(0) & ST_GLOBAL, 0);
	CHECK("globdis 1", op(INTOP_GLOBDIS, 1), 0);
	CHECK("globdis 16", op(INTOP_GLOBDIS, 16), 0);
	CHECK("globdis 16 status", status(16) & ST_GLOBAL, 0);
	CHECK("globdis 17", op(INTOP_GLOBDIS, 17), 0);
	CHECK("globdis oob", op(INTOP_GLOBDIS, BAD_INT), -1);

	puts_("D: locen\n");
	CHECK("locen on a per-CPU interrupt", op(INTOP_LOCEN, 0), -1);
	CHECK("locen 16", op(INTOP_LOCEN, 16), 0);
	CHECK("locen 16 status", status(16) & ST_LOCAL, ST_LOCAL);
	CHECK("locen 17", op(INTOP_LOCEN, 17), 0);
	CHECK("locen oob", op(INTOP_LOCEN, BAD_INT), -1);

	puts_("E: locdis\n");
	CHECK("locdis on a per-CPU interrupt", op(INTOP_LOCDIS, 0), -1);
	CHECK("locdis 16", op(INTOP_LOCDIS, 16), 0);
	CHECK("locdis 16 status", status(16) & ST_LOCAL, 0);
	CHECK("locdis 17", op(INTOP_LOCDIS, 17), 0);
	CHECK("locdis oob", op(INTOP_LOCDIS, BAD_INT), -1);

	puts_("F: affinity\n");
	CHECK("affinity per-CPU", hvm_intop(INTOP_AFFINITY, 0, 0), -1);
	CHECK("affinity 16 -> me", hvm_intop(INTOP_AFFINITY, 16, my_cpuidx()), 0);
	CHECK("affinity 16 -> me status", status(16) & ST_LOCAL, ST_LOCAL);
	CHECK("affinity 16 -> 3", hvm_intop(INTOP_AFFINITY, 16, my_cpuidx() + 3), 0);
	CHECK("affinity 16 -> 3 status", status(16) & ST_LOCAL, 0);
	CHECK("affinity oob int", hvm_intop(INTOP_AFFINITY, BAD_INT, 0), -1);
	CHECK("affinity oob cpu", hvm_intop(INTOP_AFFINITY, 16, 1000), -1);
	CHECK("affinity 16 -> me again", hvm_intop(INTOP_AFFINITY, 16, my_cpuidx()), 0);
	CHECK("locdis 16", op(INTOP_LOCDIS, 16), 0);

	/* Interrupts are off from here to "K": nothing is taken. */
	puts_("G: get/peek\n");
	CHECK("ie off", hvm_getie(), 0);
	CHECK("peek none", op(INTOP_PEEK, 0), -1);
	CHECK("get none", op(INTOP_GET, 0), -1);
	CHECK("post 0", hvm_intop(INTOP_POST, 0, 0), 0);
	CHECK("post 0 pending", status(0) & ST_PENDING, ST_PENDING);
	CHECK("post 0: not enabled", status(0) & ST_GLOBAL, 0);
	CHECK("peek: 0 not enabled", op(INTOP_PEEK, 0), -1);
	op(INTOP_GLOBEN, 0);
	CHECK("peek 0", op(INTOP_PEEK, 0), 0);
	CHECK("get 0", op(INTOP_GET, 0), 0);
	CHECK("get 0 cleared pending+global", status(0) & (ST_PENDING | ST_GLOBAL), 0);
	CHECK("post 0", hvm_intop(INTOP_POST, 0, me), 0);
	CHECK("post 1", hvm_intop(INTOP_POST, 1, me), 0);
	op(INTOP_GLOBEN, 0);
	op(INTOP_GLOBEN, 1);
	CHECK("peek 0 of 0,1", op(INTOP_PEEK, 0), 0);
	CHECK("get 0 of 0,1", op(INTOP_GET, 0), 0);
	CHECK("peek 1", op(INTOP_PEEK, 0), 1);
	CHECK("get 1", op(INTOP_GET, 0), 1);
	CHECK("peek none", op(INTOP_PEEK, 0), -1);

	CHECK("post 16", hvm_intop(INTOP_POST, 16, 0), 0);
	CHECK("peek: 16 not enabled", op(INTOP_PEEK, 0), -1);
	op(INTOP_GLOBEN, 16);
	CHECK("peek: 16 not local", op(INTOP_PEEK, 0), -1);
	op(INTOP_LOCEN, 16);
	CHECK("peek 16", op(INTOP_PEEK, 0), 16);
	CHECK("post 17", hvm_intop(INTOP_POST, 17, 0), 0);
	op(INTOP_GLOBEN, 17);
	op(INTOP_LOCEN, 17);
	CHECK("get 16", op(INTOP_GET, 0), 16);
	CHECK("get 16 status", status(16), ST_LOCAL);
	CHECK("peek 17", op(INTOP_PEEK, 0), 17);
	CHECK("get 17", op(INTOP_GET, 0), 17);
	CHECK("peek none", op(INTOP_PEEK, 0), -1);
	CHECK("ie still off", hvm_getie(), 0);

	puts_("H: status\n");
	CHECK("shared interrupt starts locally disabled", status(20) & ST_LOCAL, 0);
	for (u32 i = 0; i < 4; i++) {
		u32 n = 20 + i;
		op(INTOP_LOCDIS, n);
		if (i & 1)
			hvm_intop(INTOP_POST, n, 0);
		if (i & 2)
			op(INTOP_LOCEN, n);
		CHECK("status shared", status(n), i);
	}
	op(INTOP_LOCDIS, 24);
	op(INTOP_GLOBEN, 24);
	CHECK("status shared global", status(24), ST_GLOBAL);
	CHECK("status oob", op(INTOP_STATUS, BAD_INT), -1);
	for (u32 n = 20; n < 25; n++) {
		op(INTOP_CLEAR, n);
		op(INTOP_LOCDIS, n);
		op(INTOP_GLOBDIS, n);
	}

	puts_("I: post/clear\n");
	CHECK("post 2", hvm_intop(INTOP_POST, 2, 0), 0);
	CHECK("post 2 again", hvm_intop(INTOP_POST, 2, 0), 0);
	CHECK("post 2 pending", status(2) & ST_PENDING, ST_PENDING);
	CHECK("clear 2 (pending)", op(INTOP_CLEAR, 2), 0);
	CHECK("clear 2 cleared", status(2) & ST_PENDING, 0);
	CHECK("clear 2 again", op(INTOP_CLEAR, 2), 0);
	CHECK("post 30", hvm_intop(INTOP_POST, 30, 0), 0);
	CHECK("post 30 again", hvm_intop(INTOP_POST, 30, 0), 0);
	CHECK("clear 30 (pending)", op(INTOP_CLEAR, 30), 0);
	CHECK("clear 30 cleared", status(30) & ST_PENDING, 0);
	CHECK("clear 30 again", op(INTOP_CLEAR, 30), 0);
	CHECK("post oob int", hvm_intop(INTOP_POST, BAD_INT, 0), -1);
	CHECK("clear oob int", op(INTOP_CLEAR, BAD_INT), -1);
	/* A virtual CPU id from a VM that does not exist. */
	CHECK("post to bad id", hvm_intop(INTOP_POST, 3, (40u << 26) | (0 << 10)), -1);

	puts_("J: bad ops\n");
	CHECK("op 99", op(99, 0), -1);
	CHECK("op 11", op(11, 0), -1);
	CHECK("op -1", op(~0u, 0), -1);

	/* With interrupts on, a deliverable interrupt is taken as an event:
	   GSR.CAUSE is its number, GSR.IE the enable before the event, and
	   taking it disables it (vmint "VM enable/disable/check" I-N). */
	puts_("K: delivery\n");
	op(INTOP_GLOBEN, 5);
	hvm_intop(INTOP_POST, 5, 0);
	CHECK("not taken with ie off", seen[5], 0);
	CHECK("vmsetie(1) returns old", hvm_setie(1), 0);
	CHECK("taken on vmsetie", seen[5], 1);
	CHECK("event gsr", last_event[EV_INTERRUPT].gsr, GSR_IE | 5);
	CHECK("ie back on after vmrte", hvm_getie(), 1);
	CHECK("taking disabled it", status(5) & (ST_GLOBAL | ST_PENDING), 0);
	hvm_intop(INTOP_POST, 5, 0);
	CHECK("re-posted before globen: held", seen[5], 1);
	op(INTOP_GLOBEN, 5);
	CHECK("globen takes it", seen[5], 2);

	/* Shared, through the same path. */
	op(INTOP_GLOBEN, 40);
	op(INTOP_LOCEN, 40);
	hvm_intop(INTOP_POST, 40, 0);
	CHECK("shared taken", seen[40], 1);
	CHECK("shared gsr", last_event[EV_INTERRUPT].gsr, GSR_IE | 40);
	CHECK("shared disabled", status(40) & (ST_GLOBAL | ST_PENDING), 0);

	/* Local disable holds a shared interrupt back. */
	op(INTOP_LOCDIS, 40);
	hvm_intop(INTOP_POST, 40, 0);
	op(INTOP_GLOBEN, 40);
	CHECK("locdis holds it", seen[40], 1);
	op(INTOP_LOCEN, 40);
	CHECK("locen releases it", seen[40], 2);

	/* vmwait with interrupts off returns the pending interrupt without
	   taking it. */
	hvm_setie(0);
	op(INTOP_GLOBEN, 6);
	hvm_intop(INTOP_POST, 6, 0);
	CHECK("vmwait ie off", hvm_wait(), 6);
	CHECK("not taken", seen[6], 0);
	CHECK("still pending", status(6) & ST_PENDING, ST_PENDING);
	CHECK("vmsetie(1) takes it", hvm_setie(1), 0);
	CHECK("taken", seen[6], 1);

	/* vmwait with interrupts on: the event, then vmwait returns. */
	op(INTOP_GLOBEN, 7);
	hvm_intop(INTOP_POST, 7, 0);
	CHECK("posted with ie on: taken at once", seen[7], 1);
	hvm_setie(0);
	return 0;
}
