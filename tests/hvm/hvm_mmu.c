/*
 * Copyright (c) Qualcomm Technologies, Inc. and/or its subsidiaries.
 * SPDX-License-Identifier: BSD-3-Clause-Clear
 */

/*
 * vmnewmap/vmclrmap and translation through HVM page tables: every page
 * size, permissions, user access, and that vmclrmap makes changed entries
 * visible.
 */

#include "hvm.h"

#define TYPE_TABLE 1
#define X (1u << 11)
#define W (1u << 10)
#define R (1u << 9)
#define CCC (7u << 6)
#define U (1u << 5)
#define S_4M 5
#define S_16M 6
#define S_INVALID 7

#define CAUSE_XPROT 0x11
#define CAUSE_RPROT 0x22
#define CAUSE_WPROT 0x23
#define CAUSE_URPROT 0x24
#define CAUSE_UWPROT 0x25

/* Mapped through the tables below. */
#define VA_BASE 0x40000000u
/* Logical memory the mappings point at, identity-mapped as well. */
#define PA_BASE 0xb0000000u

static u32 l1[1024] __attribute__((aligned(4096)));
/* One L2 table per small page size; the 4K one is the largest. */
static u32 l2[5][1024] __attribute__((aligned(4096)));

static u32 page_size(u32 s) { return 4096u << (2 * s); }

static void identity_4m(u32 base, u32 len, u32 perm)
{
	for (u32 a = base & ~0x3fffffu; a < base + len; a += 0x400000)
		l1[a >> 22] = a | perm | CCC | S_4M;
}

/* Map one page of size code s at va to pa. */
static void map(u32 s, u32 va, u32 pa, u32 perm)
{
	if (s >= S_4M) {
		u32 n = s == S_16M ? 4 : 1;
		for (u32 i = 0; i < n; i++)
			l1[(va >> 22) + i] = pa | perm | CCC | s;
		return;
	}
	u32 *t = l2[s];
	l1[va >> 22] = (u32)t | s;
	u32 idx = (va & 0x3fffff) / page_size(s);
	t[idx] = pa | perm | CCC | s;
}

static volatile u32 *at(u32 a) { return (volatile u32 *)a; }

u32 hvm_load_w(u32 addr);
void hvm_load_w_resume(void);
void hvm_store_w(u32 addr, u32 v);
void hvm_store_w_resume(void);
void hvm_user_resume(void);

static u32 errors(void) { return last_event[EV_ERROR].count; }

static void expect_load_fault(u32 va, u32 cause, const char *what)
{
	u32 before = errors();
	resume_at = (u32)hvm_load_w_resume;
	hvm_load_w(va);
	if (errors() != before + 1) {
		resume_at = 0;
		fail(what, 0, cause);
	}
	CHECK(what, GSR_CAUSE(last_event[EV_ERROR].gsr), cause);
	CHECK("fault gbadva", last_event[EV_ERROR].gbadva, va);
}

static void expect_store_fault(u32 va, u32 cause, const char *what)
{
	u32 before = errors();
	resume_at = (u32)hvm_store_w_resume;
	hvm_store_w(va, 0x5a5a5a5a);
	if (errors() != before + 1) {
		resume_at = 0;
		fail(what, 0, cause);
	}
	CHECK(what, GSR_CAUSE(last_event[EV_ERROR].gsr), cause);
	CHECK("fault gbadva", last_event[EV_ERROR].gbadva, va);
}

static u32 user_load(u32 va) { return *(volatile u32 *)va; }
static u32 user_store(u32 va) { *(volatile u32 *)va = 1; return 0; }

int main(void)
{
	extern char _start[];

	/* Unused entries are invalid; a zero entry would be a directory of
	   4K pages at address 0. */
	for (u32 i = 0; i < 1024; i++)
		l1[i] = S_INVALID;
	/* The image, its stacks and the vectors stay where they are. */
	identity_4m((u32)_start, 0x01000000, X | W | R | U);
	identity_4m(PA_BASE, 0x08000000, W | R);

	puts_("newmap\n");
	CHECK("bad type", hvm_newmap((u32)l1, 0x12345, 0, 0), -1);
	/* Only a varadix map reads r3; Linux leaves anything there. */
	CHECK("newmap", hvm_newmap((u32)l1, TYPE_TABLE, 1, 0xdeadbeef), 0);
	CHECK("still running", *at((u32)&l1[(u32)_start >> 22]) & 7, S_4M);

	puts_("every page size\n");
	for (u32 s = 0; s <= S_16M; s++) {
		u32 sz = page_size(s);
		u32 va = VA_BASE + s * 0x01000000u + (s < S_4M ? 2 * sz : 0);
		u32 pa = PA_BASE + s * 0x01000000u;
		map(s, va, pa, R | W);
		u32 offs[3] = {0, sz / 2 + 4, sz - 4};
		for (u32 i = 0; i < 3; i++)
			*at(pa + offs[i]) = 0x1000 * s + i + 1;
		for (u32 i = 0; i < 3; i++) {
			u32 got = *at(va + offs[i]);
			if (got != 0x1000 * s + i + 1) {
				say("page size code", s);
				say("offset", offs[i]);
				fail("read through mapping", got, 0x1000 * s + i + 1);
			}
		}
		*at(va + 8) = 0xabc0 + s;
		CHECK("write through mapping", *at(pa + 8), 0xabc0 + s);
		/* The neighbouring page of a small size is not mapped. */
		if (s < S_4M) {
			expect_load_fault(va + sz, CAUSE_RPROT, "unmapped neighbour");
		}
	}

	puts_("clrmap one page\n");
	u32 va = VA_BASE + 2 * 4096;	/* the 4K page from above */
	u32 pa2 = PA_BASE + 0x00100000;
	*at(pa2) = 0x2222;
	CHECK("before", *at(va), 0x1);
	l2[0][2] = pa2 | R | W | CCC;
	CHECK("clrmap", hvm_clrmap(va, 4096), 0);
	CHECK("after clrmap", *at(va), 0x2222);

	puts_("clrmap several pages\n");
	map(0, va + 4096, pa2 + 4096, R | W);
	*at(pa2 + 4096) = 0x3333;
	CHECK("second page", *at(va + 4096), 0x3333);
	*at(pa2 + 0x10000) = 0x4444;
	*at(pa2 + 0x11000) = 0x5555;
	l2[0][2] = (pa2 + 0x10000) | R | W | CCC;
	l2[0][3] = (pa2 + 0x11000) | R | W | CCC;
	CHECK("clrmap 2", hvm_clrmap(va, 8192), 0);
	CHECK("after clrmap 2: first", *at(va), 0x4444);
	CHECK("after clrmap 2: second", *at(va + 4096), 0x5555);
	CHECK("clrmap 0", hvm_clrmap(va, 0), 0);

	puts_("clrmap by bytes\n");
	/* A range is in bytes: a few bytes inside one page flush just it. */
	*at(pa2 + 0x20000) = 0x6666;
	l2[0][3] = (pa2 + 0x20000) | R | W | CCC;
	CHECK("clrmap inside a page", hvm_clrmap(va + 4096 + 0x800, 0x10), 0);
	CHECK("after clrmap inside", *at(va + 4096), 0x6666);
	/* An unaligned range of 4096 bytes touches two pages. */
	*at(pa2 + 0x30000) = 0x7777;
	*at(pa2 + 0x31000) = 0x8888;
	l2[0][2] = (pa2 + 0x30000) | R | W | CCC;
	l2[0][3] = (pa2 + 0x31000) | R | W | CCC;
	CHECK("clrmap unaligned", hvm_clrmap(va + 0x800, 4096), 0);
	CHECK("unaligned: first", *at(va), 0x7777);
	CHECK("unaligned: second", *at(va + 4096), 0x8888);

	puts_("permissions\n");
	u32 rva = VA_BASE + 0x00800000, rpa = PA_BASE + 0x00800000;
	map(0, rva, rpa, R);
	*at(rpa) = 0x77;
	CHECK("read-only read", *at(rva), 0x77);
	expect_store_fault(rva, CAUSE_WPROT, "store to read-only");
	CHECK("unchanged", *at(rpa), 0x77);
	map(0, rva + 4096, rpa + 4096, W);
	expect_load_fault(rva + 4096, CAUSE_RPROT, "load from write-only");
	hvm_store_w(rva + 4096, 0x99);
	CHECK("write-only store", *at(rpa + 4096), 0x99);
	map(0, rva + 8192, rpa + 8192, X);
	expect_load_fault(rva + 8192, CAUSE_RPROT, "load from execute-only");
	/* An entry with none of R, W, X is a page fault. */
	map(0, rva + 12288, rpa + 12288, 0);
	expect_load_fault(rva + 12288, CAUSE_RPROT, "load, no permissions");
	l1[(VA_BASE + 0x02800000) >> 22] = S_INVALID;
	expect_load_fault(VA_BASE + 0x02800000, CAUSE_RPROT, "load, invalid L1");

	puts_("execute\n");
	/* "jumpr r31" in a fresh page: no X faults, X runs it. */
	u32 cva = VA_BASE + 0x00900000, cpa = PA_BASE + 0x00900000;
	*at(cpa) = 0x529fc000;
	CHECK("idsync", hvm_cache(5, cpa, 4), 0);
	map(0, cva, cpa, R);
	{
		u32 before = errors();
		static volatile u32 back;
		back = 0;
		/* The fault resumes at the return address. */
		void (*fn)(void) = (void (*)(void))cva;
		resume_at = (u32)&&returned;
		fn();
returned:
		back = 1;
		CHECK("execute without X", errors(), before + 1);
		CHECK("execute fault cause", GSR_CAUSE(last_event[EV_ERROR].gsr), CAUSE_XPROT);
		CHECK("execute fault gelr", last_event[EV_ERROR].gelr, cva);
	}
	map(0, cva, cpa, R | X);
	hvm_clrmap(cva, 4096);
	{
		u32 before = errors();
		((void (*)(void))cva)();
		CHECK("execute with X", errors(), before);
	}

	puts_("user access\n");
	u32 uva = VA_BASE + 0x00a00000, upa = PA_BASE + 0x00a00000;
	map(0, uva, upa, R | W);
	map(0, uva + 4096, upa + 4096, R | W | U);
	*at(upa) = 0x123;
	*at(upa + 4096) = 0x456;
	CHECK("user, U page", hvm_run_user(user_load, uva + 4096, STACK_TOP(1)), 0x456);
	resume_at = (u32)hvm_user_resume;
	hvm_run_user(user_load, uva, STACK_TOP(1));
	CHECK("user load, no U", last_event[EV_ERROR].gsr, GSR_UM | CAUSE_URPROT);
	CHECK("user load gbadva", last_event[EV_ERROR].gbadva, uva);
	resume_at = (u32)hvm_user_resume;
	hvm_run_user(user_store, uva, STACK_TOP(1));
	CHECK("user store, no U", last_event[EV_ERROR].gsr, GSR_UM | CAUSE_UWPROT);
	CHECK("unchanged", *at(upa), 0x123);

	puts_("monitor protection\n");
	u32 pva = VA_BASE + 0x00b00000;
	map(0, pva, hvm_getinfo(9), R | W); /* INFO_PHYS_ADDR */
	hvm_clrmap(pva, 4096);
	expect_load_fault(pva, CAUSE_RPROT, "read monitor image");
	expect_store_fault(pva, CAUSE_WPROT, "write monitor image");
	map(0, pva, hvm_getinfo(15), R | W); /* INFO_L2VIC_BASE */
	hvm_clrmap(pva, 4096);
	expect_load_fault(pva, CAUSE_RPROT, "read monitor device");

	/* The monitor keeps guest mappings inside its physical fences:
	   pages 0 through 0xfe000, i.e. below 0xfe001000. */
	puts_("fences\n");
	map(0, pva, 0xfe000000u, R | W);
	hvm_clrmap(pva, 4096);
	(void)*at(pva); /* last page inside the fence: mapped, may read 0 */
	map(0, pva, 0xfe001000u, R | W);
	hvm_clrmap(pva, 4096);
	expect_load_fault(pva, CAUSE_RPROT, "read just above fence");
	expect_store_fault(pva, CAUSE_WPROT, "write just above fence");
	map(0, pva, 0xff000000u, R | W);
	hvm_clrmap(pva, 4096);
	expect_load_fault(pva, CAUSE_RPROT, "read far above fence");
	/* A 16M page straddling the fence is refused as a whole. */
	map(S_16M, VA_BASE + 0x03000000u, 0xfe000000u, R | W);
	hvm_clrmap(VA_BASE + 0x03000000u, 4096);
	expect_load_fault(VA_BASE + 0x03000000u + 0x00f00000u, CAUSE_RPROT,
			  "read page straddling fence");

	/* A page table above the fence reads as all-invalid. */
	puts_("table above fence\n");
	u32 fva = VA_BASE + 0x00c00000u;
	l1[fva >> 22] = 0xff000000u | 0; /* directory of 4K pages */
	hvm_clrmap(fva, 4096);
	expect_load_fault(fva, CAUSE_RPROT, "walk a table above the fence");
	l1[fva >> 22] = S_INVALID;
	hvm_clrmap(fva, 4096);
	return 0;
}
