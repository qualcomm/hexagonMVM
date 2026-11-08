/*
 * Copyright (c) Qualcomm Technologies, Inc. and/or its subsidiaries.
 * SPDX-License-Identifier: BSD-3-Clause-Clear
 */

/*
 * Per-CPU timeouts on every virtual CPU at once: each CPU repeatedly sets
 * an absolute timeout a pseudo-random 50us-300us ahead and waits for it. A
 * wake-up before the timeout is a failure; a late one is measured.
 */

#include "hvm.h"

#define TIMER 12
#define SPINS 256
#define DONE 1
#define RANDOM_SHIFT 14
#define RANDOM_BASE 50000
/* 200us is plenty on hexagon-sim. QEMU's clock is the host's, so a
   loaded host (a CI runner with fewer cores than vCPUs) can be tens of ms
   later than that; only report it. A timeout that never fires hangs the
   test, which the runner's time limit catches. */
#define MAX_DELTA 200000
#define MAX_DELTA_HARD 1000000000

#define GETTIME 2
#define GETTIMEOUT 3
#define SETTIMEOUT 4

static volatile u32 done[NCPU_MAX];
static volatile u32 late[NCPU_MAX];
static volatile u32 missed[NCPU_MAX];
static volatile u32 worst[NCPU_MAX];
static volatile u32 next_slot;
static volatile u32 timer_events[NCPU_MAX];
static volatile u32 slot_by_cpuidx[64];

static int hook(u32 kind, u32 gsr, u32 gelr, u32 gbadva)
{
	if (kind == EV_INTERRUPT && GSR_CAUSE(gsr) == TIMER) {
		timer_events[slot_by_cpuidx[my_cpuidx() & 63]]++;
		return 1;
	}
	return 0;
}

static u64 random_delta(u64 seed)
{
	return (((u32)seed * 2654435769U) >> RANDOM_SHIFT) + RANDOM_BASE;
}

static void task(u32 me)
{
	slot_by_cpuidx[my_cpuidx() & 63] = me;
	for (u32 i = 0; i < SPINS; i++) {
		hvm_intop(INTOP_GLOBEN, TIMER, 0);
		u64 start = hvm_timerop(GETTIME, 0, 0);
		u64 wakeup = start + random_delta(start + me);
		if (hvm_timerop(SETTIMEOUT, 0, wakeup) == 0) {
			/* Under QEMU the other CPUs can run for longer than
			   the delta in between: it is in the past, and such a
			   timeout is cancelled. */
			if (hvm_timerop(GETTIME, 0, 0) <= wakeup)
				fail("future timeout refused", (u32)start, (u32)wakeup);
			CHECK("past timeout not set", (u32)hvm_timerop(GETTIMEOUT, 0, 0), 0);
			missed[me]++;
			continue;
		}
		s64 real = (s64)hvm_timerop(GETTIMEOUT, 0, 0);
		s64 d = real - (s64)wakeup;
		/* 0: it has fired already, which only a slow host explains. */
		if (real == 0 && hvm_timerop(GETTIME, 0, 0) < wakeup)
			fail("timeout gone before its time", (u32)start, (u32)wakeup);
		if (real != 0 && (d < -100 || d > 100))
			fail("get timeout", (u32)real, (u32)wakeup);
		u32 before = timer_events[me];
		/* Interrupts off: vmwait returns the timer's number without
		   taking it, so no event races the check below. */
		while (timer_events[me] == before) {
			if (hvm_wait() == -1)
				continue;
			/* What vmwait saw may not be what GET takes: a DONE
			   posted in between comes first. */
			s32 n = hvm_intop(INTOP_GET, 0, 0);
			if (n == TIMER) {
				timer_events[me]++;
			} else if (n == DONE) {
				/* A secondary finished; main counts on done[]. */
				hvm_intop(INTOP_GLOBEN, DONE, 0);
			} else if (n != -1) {
				fail("unexpected interrupt", n, TIMER);
			}
		}
		u64 end = hvm_timerop(GETTIME, 0, 0);
		if (end < wakeup)
			fail("woke before the timeout, ns early", (u32)(wakeup - end), 0);
		u64 delta = end - wakeup;
		if (delta > worst[me])
			worst[me] = delta;
		if (delta > MAX_DELTA)
			late[me]++;
		if (delta > MAX_DELTA_HARD) {
			say("cpu slot", me);
			say("wakeup ns", (u32)wakeup);
			fail("woke very late, ns", (u32)delta, MAX_DELTA_HARD);
		}
	}
	done[me] = 1;
}

static volatile u32 main_id;

static void secondary(void)
{
	task(__atomic_fetch_add(&next_slot, 1, __ATOMIC_SEQ_CST));
	/* Wake the main CPU, which waits rather than spins: under QEMU's
	   round-robin TCG a spinning CPU holds up the others. */
	hvm_intop(INTOP_POST, DONE, main_id);
	hvm_stop(0);
}

int main(void)
{
	event_hook = hook;
	main_id = hvm_vpid();
	hvm_intop(INTOP_GLOBEN, DONE, 0);
	next_slot = 1;
	u32 n = 1;
	for (u32 i = 1; i < NCPU_MAX; i++) {
		if (hvm_start(secondary, STACK_TOP(i)) == -1)
			break;
		n++;
	}
	say("cpus", n);
	task(0);
	for (u32 i = 0; i < n; i++)
		while (!done[i])
			if (hvm_wait() == DONE) {
				hvm_intop(INTOP_GET, 0, 0);
				hvm_intop(INTOP_GLOBEN, DONE, 0);
			}
	u32 total_late = 0, w = 0, total_missed = 0;
	for (u32 i = 0; i < n; i++) {
		total_late += late[i];
		total_missed += missed[i];
		if (worst[i] > w)
			w = worst[i];
	}
	say("wake-ups later than 200us", total_late);
	say("latest wake-up, ns", w);
	say("timeouts already past when set", total_missed);
	return 0;
}
