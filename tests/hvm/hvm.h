/*
 * Copyright (c) Qualcomm Technologies, Inc. and/or its subsidiaries.
 * SPDX-License-Identifier: BSD-3-Clause-Clear
 */

/*
 * C guest test support: the virtual instructions as functions (crt.S),
 * output, and checking.
 */

#ifndef HVM_H
#define HVM_H

typedef unsigned int u32;
typedef int s32;
typedef unsigned long long u64;
typedef long long s64;

/* Event vector offsets / kinds (vector index). */
#define EV_MACHINE_CHECK 1
#define EV_ERROR 2
#define EV_TRAP0 5
#define EV_INTERRUPT 7

#define GSR_UM (1u << 31)
#define GSR_IE (1u << 30)
#define GSR_CAUSE(gsr) ((gsr) & 0xffff)

#define INTOP_NOP 0
#define INTOP_GLOBEN 1
#define INTOP_GLOBDIS 2
#define INTOP_LOCEN 3
#define INTOP_LOCDIS 4
#define INTOP_AFFINITY 5
#define INTOP_GET 6
#define INTOP_PEEK 7
#define INTOP_STATUS 8
#define INTOP_POST 9
#define INTOP_CLEAR 10

#define PERCPU_INTS 16

u32 hvm_version(void);
u32 hvm_setvec(void *vec);
u32 hvm_setie(u32 ie);
u32 hvm_getie(void);
s32 hvm_intop(u32 op, u32 intno, u32 arg);
u32 hvm_clrmap(u32 va, u32 bytes);
s32 hvm_newmap(u32 ptb, u32 type, u32 flush, u32 extra);
s32 hvm_cache(u32 op, u32 va, u32 len);
u64 hvm_gettime(void);
void hvm_settime(u64 t);
s32 hvm_wait(void);
u32 hvm_yield(void);
s32 hvm_start(void (*pc)(void), void *sp);
void hvm_stop(u32 status);
u32 hvm_vpid(void);
void hvm_setregs(u32 gelr, u32 gsr, u32 gosp, u32 gbadva);
void hvm_getregs(u32 out[4]);
/* Timer ops: 0 freq, 1 resolution, 2 time, 3 timeout, 4 set absolute
   timeout, 5 set relative timeout. Times in ns; the argument in r3:2. */
u64 hvm_timerop(u32 op, u32 unused, u64 arg);
u32 hvm_getinfo(u32 code);
u32 hvm_trap1_n(u32 n);
u32 hvm_trap0_n(u32 n);

u32 hvm_rd_gelr(void);
u32 hvm_rd_gsr(void);
u32 hvm_rd_gosp(void);
u32 hvm_rd_gbadva(void);
void hvm_wr_gelr(u32);
void hvm_wr_gsr(u32);
void hvm_wr_gosp(u32);
void hvm_wr_gbadva(u32);

u32 hvm_run_user(u32 (*fn)(u32), u32 arg, void *usp);
extern volatile u32 hvm_user_result;
extern char hvm_stacks[];
/* Virtual CPUs the guests use, and stacks for them: one per CPU, matching
   NSTACKS in crt.S. minivm has room for this many hardware threads. */
#define NCPU_MAX 16
#define STACK_SIZE 16384
#define STACK_TOP(i) ((void *)(hvm_stacks + STACK_SIZE * ((i) + 1)))

void sh_write0(const char *s);
void sh_shutdown(void) __attribute__((noreturn));

/* lib.c */
void puts_(const char *s);
void puthex(u32 v);
void say(const char *s, u32 v);
void fail(const char *what, u32 got, u32 want) __attribute__((noreturn));
#define CHECK(what, got, want)                                     \
	do {                                                       \
		u32 g_ = (u32)(got), w_ = (u32)(want);             \
		if (g_ != w_)                                      \
			fail(what, g_, w_);                        \
	} while (0)

/* What the last event looked like. */
struct event {
	volatile u32 count;
	volatile u32 kind;
	volatile u32 gsr;
	volatile u32 gelr;
	volatile u32 gbadva;
	volatile u32 sp;
};
extern struct event last_event[8];

/*
 * Optional hook, run for every event before the default handling. Returns
 * nonzero if it handled the event completely.
 */
extern int (*event_hook)(u32 kind, u32 gsr, u32 gelr, u32 gbadva);

/*
 * Where an expected error or trap0 event resumes: 0 means "the packet after
 * the faulting one", which is GELR + 4 for the one-word instructions the
 * tests fault on.
 */
extern volatile u32 resume_at;

/* Virtual CPU id layout: vint:10, cpuidx:16, vmidx:6. */
static inline u32 cpuidx_of(u32 id)
{
	return (id >> 10) & 0xffff;
}

static inline u32 my_cpuidx(void)
{
	return cpuidx_of(hvm_vpid());
}

static inline void barrier(void)
{
	__asm__ __volatile__("" ::: "memory");
}

#endif
