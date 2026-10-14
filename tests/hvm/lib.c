/*
 * Copyright (c) Qualcomm Technologies, Inc. and/or its subsidiaries.
 * SPDX-License-Identifier: BSD-3-Clause-Clear
 */

#include "hvm.h"

struct event last_event[8];
int (*event_hook)(u32 kind, u32 gsr, u32 gelr, u32 gbadva);
volatile u32 resume_at;

/* Output is serialised so that lines from different CPUs stay whole. */
static volatile u32 out_lock;

static void lock(void)
{
	while (__atomic_exchange_n(&out_lock, 1, __ATOMIC_ACQUIRE))
		while (out_lock)
			;
}

static void unlock(void)
{
	__atomic_store_n(&out_lock, 0, __ATOMIC_RELEASE);
}

void puts_(const char *s)
{
	lock();
	sh_write0(s);
	unlock();
}

static char *hex(char *p, u32 v)
{
	*p++ = '0';
	*p++ = 'x';
	for (int i = 28; i >= 0; i -= 4)
		*p++ = "0123456789abcdef"[(v >> i) & 0xf];
	return p;
}

static char *copy(char *p, const char *s)
{
	while (*s)
		*p++ = *s++;
	return p;
}

void puthex(u32 v)
{
	char buf[12], *p = hex(buf, v);
	*p = 0;
	puts_(buf);
}

void say(const char *s, u32 v)
{
	char buf[160], *p = copy(buf, s);
	*p++ = ' ';
	p = hex(p, v);
	*p++ = '\n';
	*p = 0;
	puts_(buf);
}

void fail(const char *what, u32 got, u32 want)
{
	char buf[200], *p = copy(buf, "FAIL: ");
	p = copy(p, what);
	p = copy(p, ": got ");
	p = hex(p, got);
	p = copy(p, ", want ");
	p = hex(p, want);
	*p++ = '\n';
	*p = 0;
	puts_(buf);
	sh_shutdown();
}

void hvm_user_resume(void);

/* Called from the event vectors in crt.S. */
void hvm_event(u32 kind, u32 gsr, u32 gelr, u32 gbadva)
{
	u32 sp;
	__asm__ __volatile__("%0 = r29" : "=r"(sp));
	struct event *e = &last_event[kind];
	e->kind = kind;
	e->gsr = gsr;
	e->gelr = gelr;
	e->gbadva = gbadva;
	e->sp = sp;
	e->count++;

	if (event_hook && event_hook(kind, gsr, gelr, gbadva))
		return;

	switch (kind) {
	case EV_TRAP0:
		if (GSR_CAUSE(gsr) == 0x7f && (gsr & GSR_UM)) {
			/* A user-mode call returned: back to the kernel. */
			hvm_wr_gelr((u32)hvm_user_resume);
			hvm_wr_gsr(0);
			return;
		}
		/* Semihosting (cause 0) was done already; others just
		   return to the following packet. */
		return;
	case EV_ERROR:
		if (!resume_at)
			fail("unexpected error event, gsr/gelr", gsr, gelr);
		hvm_wr_gelr(resume_at);
		/* Back to the mode it came from, except that giving up on a
		   user-mode call returns to the kernel. */
		if (resume_at == (u32)hvm_user_resume)
			hvm_wr_gsr(gsr & ~GSR_UM);
		resume_at = 0;
		return;
	case EV_INTERRUPT:
		return;
	default:
		fail("unexpected event", kind, gsr);
	}
}

/* main returned: report and stop. */
void hvm_finish(int rc)
{
	if (rc == 0)
		puts_("PASS\n");
	else
		fail("main returned", rc, 0);
	hvm_stop(0);
}
