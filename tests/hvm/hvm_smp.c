/*
 * Copyright (c) Qualcomm Technologies, Inc. and/or its subsidiaries.
 * SPDX-License-Identifier: BSD-3-Clause-Clear
 */

/*
 * Several virtual CPUs: vmstart/vmstop, interrupts posted between CPUs,
 * shared interrupt routing, and the timer, checked through the virtual
 * instructions.
 */

#include "hvm.h"

#define IPI 1
#define SHARED 50
#define TIMER 12
#define NCPU_MAX 8

static volatile u32 ids[NCPU_MAX];
static volatile u32 got[NCPU_MAX][64];
static volatile u32 running[NCPU_MAX];
static volatile u32 quit;
static volatile u32 next_slot;

static u32 slot_of(u32 id)
{
	for (u32 i = 0; i < NCPU_MAX; i++)
		if (ids[i] == id)
			return i;
	fail("unknown cpu id", id, 0);
}

/* Interrupts count against the CPU that took them; the IPI is acked by
   re-enabling it. */
static int hook(u32 kind, u32 gsr, u32 gelr, u32 gbadva)
{
	if (kind != EV_INTERRUPT)
		return 0;
	u32 n = GSR_CAUSE(gsr), me = slot_of(hvm_vpid());
	__atomic_fetch_add(&got[me][n & 63], 1, __ATOMIC_SEQ_CST);
	if (n == IPI || n == SHARED || n == TIMER)
		hvm_intop(INTOP_GLOBEN, n, 0);
	return 1;
}

/* A secondary: take interrupts while waiting, until told to quit. */
static void secondary(void)
{
	u32 me = __atomic_fetch_add(&next_slot, 1, __ATOMIC_SEQ_CST);
	ids[me] = hvm_vpid();
	hvm_intop(INTOP_GLOBEN, IPI, 0);
	/* Shared interrupts start locally disabled. */
	hvm_intop(INTOP_LOCEN, SHARED, 0);
	hvm_setie(1);
	running[me] = 1;
	while (!quit)
		hvm_wait();
	hvm_setie(0);
	running[me] = 0;
	hvm_stop(0);
}

static void wait_for(volatile u32 *p, u32 v, const char *what)
{
	for (u32 i = 0; i < 50000000; i++)
		if (*p == v)
			return;
	fail(what, *p, v);
}

static u32 start_all(u32 want)
{
	u32 n = 1;
	next_slot = 1;
	quit = 0;
	for (u32 i = 1; i < want; i++) {
		s32 id = hvm_start(secondary, STACK_TOP(i));
		if (id == -1)
			break;
		wait_for(&running[i], 1, "secondary did not start");
		CHECK("start returns the new cpu's id", (u32)id, ids[i]);
		n++;
	}
	return n;
}

static void stop_all(u32 n)
{
	quit = 1;
	for (u32 i = 1; i < n; i++) {
		hvm_intop(INTOP_POST, IPI, ids[i]);
		wait_for(&running[i], 0, "secondary did not stop");
	}
	/* Let the last vmstop land before the slot is reused. */
	for (volatile int d = 0; d < 100000; d++)
		;
}

int main(void)
{
	event_hook = hook;
	ids[0] = hvm_vpid();

	puts_("vmstart: bad arguments\n");
	CHECK("misaligned pc", hvm_start((void (*)(void))((u32)secondary + 2), STACK_TOP(1)), -1);
	CHECK("misaligned sp", hvm_start(secondary, (char *)STACK_TOP(1) - 4), -1);

	puts_("vmstart: every thread\n");
	u32 n = start_all(NCPU_MAX + 1);
	say("cpus", n);
	if (n < 2)
		fail("no second cpu", n, 2);
	/* One more than there are threads for. */
	CHECK("start with none free", hvm_start(secondary, STACK_TOP(n)), -1);

	puts_("ipi ping-pong\n");
	for (u32 round = 0; round < 200; round++) {
		for (u32 i = 1; i < n; i++) {
			u32 before = got[i][IPI];
			CHECK("post ipi", hvm_intop(INTOP_POST, IPI, ids[i]), 0);
			wait_for(&got[i][IPI], before + 1, "ipi not taken");
		}
	}
	for (u32 i = 1; i < n; i++)
		if (got[i][IPI] != 200)
			fail("ipi count", got[i][IPI], 200);

	puts_("shared interrupt goes to one waiting cpu\n");
	hvm_intop(INTOP_GLOBEN, SHARED, 0);
	for (u32 k = 0; k < 50; k++) {
		u32 total = 0, t;
		for (u32 i = 0; i < n; i++)
			total += got[i][SHARED];
		hvm_intop(INTOP_POST, SHARED, 0);
		/* Interrupts are off here, so a secondary must take it. */
		for (u32 spin = 0;; spin++) {
			t = 0;
			for (u32 i = 0; i < n; i++)
				t += got[i][SHARED];
			if (t == total + 1)
				break;
			if (spin > 50000000)
				fail("shared interrupt not taken", t, total + 1);
		}
		CHECK("main did not take it", got[0][SHARED], 0);
	}

	puts_("affinity\n");
	for (u32 i = 1; i < n; i++) {
		u32 before = got[i][SHARED];
		CHECK("affinity", hvm_intop(INTOP_AFFINITY, SHARED, cpuidx_of(ids[i])), 0);
		hvm_intop(INTOP_POST, SHARED, 0);
		wait_for(&got[i][SHARED], before + 1, "affine cpu did not take it");
	}

	puts_("stop and start again\n");
	stop_all(n);
	u32 m = start_all(3);
	CHECK("restart", m, 3);
	for (u32 i = 1; i < m; i++) {
		u32 before = got[i][IPI];
		hvm_intop(INTOP_POST, IPI, ids[i]);
		wait_for(&got[i][IPI], before + 1, "ipi after restart");
	}
	stop_all(m);
	CHECK("post to a stopped cpu", hvm_intop(INTOP_POST, IPI, ids[1]), -1);

	puts_("timer\n");
	u64 freq = hvm_timerop(0, 0, 0), res = hvm_timerop(1, 0, 0);
	if (freq == 0 || res == 0)
		fail("timer freq/res", (u32)freq, (u32)res);
	u64 t0 = hvm_timerop(2, 0, 0);
	/* A per-CPU interrupt needs only the global enable. */
	hvm_intop(INTOP_GLOBEN, TIMER, 0);
	hvm_setie(1);
	hvm_timerop(5, 0, 1000000);	/* 1ms */
	wait_for(&got[0][TIMER], 1, "timer not taken");
	CHECK("timeout cleared", (u32)hvm_timerop(3, 0, 0), 0);

	/* A timeout while waiting ends the wait. */
	hvm_timerop(5, 0, 2000000);
	u32 before = got[0][TIMER];
	hvm_wait();
	CHECK("vmwait ended by timer", got[0][TIMER], before + 1);
	u64 t1 = hvm_timerop(2, 0, 0);
	t0 = hvm_timerop(2, 0, 0);
	(void)t1;

	/* An absolute timeout already past cancels. */
	before = got[0][TIMER];
	hvm_timerop(5, 0, 1000000);
	CHECK("past timeout", (u32)hvm_timerop(4, 0, t0 - 1000), 0);
	CHECK("past timeout: none set", (u32)hvm_timerop(3, 0, 0), 0);
	t0 = hvm_timerop(2, 0, 0);
	while (hvm_timerop(2, 0, 0) - t0 < 3000000)
		;
	CHECK("past timeout: not taken", got[0][TIMER], before);

	/* Cancelled timeout does not fire. */
	before = got[0][TIMER];
	hvm_timerop(5, 0, 1000000);
	hvm_timerop(5, 0, ~0ull);
	t0 = hvm_timerop(2, 0, 0);
	while (hvm_timerop(2, 0, 0) - t0 < 3000000)
		;
	CHECK("cancelled timeout", got[0][TIMER], before);
	hvm_setie(0);
	return 0;
}
