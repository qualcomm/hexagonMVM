/*
 * Copyright (c) Qualcomm Technologies, Inc. and/or its subsidiaries.
 * SPDX-License-Identifier: BSD-3-Clause-Clear
 */

/*
 * vmnewmap's variable-radix translation (type 3): the arguments it
 * refuses, then running on its tables, every leaf size from 4K to 16M
 * (odd ones too), vmclrmap, permissions, user access and the monitor's
 * own memory.
 */

#include "hvm.h"

#define TYPE_VARADIX 3
/* The whole 32-bit address space: a 256-entry root of 16M slots. */
#define VPN_BITS 20
#define ROOT_BITS 8

/* Leaf fields. */
#define X (1u << 31)
#define W (1u << 30)
#define R (1u << 29)
#define U (1u << 28)
#define CCC (7u << 24)

#define CAUSE_XPROT 0x11
#define CAUSE_RPROT 0x22
#define CAUSE_WPROT 0x23
#define CAUSE_URPROT 0x24
#define CAUSE_UWPROT 0x25

/* Mapped through the tables below. */
#define VA_BASE 0x40000000u
/* Logical memory the mappings point at, identity-mapped as well. */
#define PA_BASE 0xb0000000u

static u32 root[1 << ROOT_BITS] __attribute__((aligned(4 << ROOT_BITS)));
/* Lower tables, each aligned to its size. */
static u32 pool[4096] __attribute__((aligned(4096)));
static u32 pool_used;

static u32 *new_table(u32 bits)
{
	u32 n = 1u << bits;
	pool_used = (pool_used + n - 1) & ~(n - 1);
	if (pool_used + n > sizeof(pool) / sizeof(pool[0]))
		fail("table pool", pool_used, n);
	u32 *t = &pool[pool_used];
	pool_used += n;
	for (u32 i = 0; i < n; i++)
		t[i] = 0;
	return t;
}

/* A pointer to a table of 2^bits entries. */
static u32 table_entry(u32 *t, u32 bits) { return ((u32)t >> 2) | (1u << (bits - 1)); }

/* A leaf mapping 2^k 4K pages at pa. */
static u32 leaf(u32 k, u32 pa, u32 perm)
{
	return perm | CCC | (((pa >> 12) >> k) << (k + 1)) | (1u << k);
}

/*
 * Map 2^k pages at va to pa, making tables of 3 or 4 bits on the way
 * down as needed.
 */
static void map(u32 k, u32 va, u32 pa, u32 perm)
{
	u32 vpn = va >> 12;
	u32 *table = root;
	u32 bits = ROOT_BITS;
	u32 startbit = VPN_BITS - ROOT_BITS;
	for (;;) {
		u32 *e = &table[(vpn >> startbit) & ((1u << bits) - 1)];
		if (startbit == k) {
			*e = leaf(k, pa, perm);
			return;
		}
		u32 s = *e ? (u32)__builtin_ctz(*e) : 32;
		if (s >= startbit) {
			/* Nothing here, or a leaf: put a table in its place. */
			u32 nb = startbit - k;
			if (nb > 3)
				nb = 3 + (startbit & 1);
			u32 *t = new_table(nb);
			*e = table_entry(t, nb);
			table = t;
			bits = nb;
		} else {
			if (startbit - (s + 1) < k)
				fail("table in the way", startbit, k);
			table = (u32 *)((*e & ~((2u << s) - 1)) << 2);
			bits = s + 1;
		}
		startbit -= bits;
	}
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

	/* The image, its stacks and the vectors stay where they are. */
	map(12, (u32)_start, (u32)_start, X | W | R | U);
	for (u32 i = 0; i < 8; i++)
		map(12, PA_BASE + (i << 24), PA_BASE + (i << 24), W | R);
	u32 rootp = table_entry(root, ROOT_BITS);

	puts_("refused\n");
	CHECK("width past 22 bits", hvm_newmap(rootp, TYPE_VARADIX, 0, 23), -1);
	CHECK("zero root", hvm_newmap(0, TYPE_VARADIX, 0, VPN_BITS), -1);
	CHECK("root leaf wider than the space",
	      hvm_newmap(leaf(12, 0, R), TYPE_VARADIX, 0, 11), -1);

	puts_("newmap\n");
	CHECK("newmap", hvm_newmap(rootp, TYPE_VARADIX, 1, VPN_BITS), 0);

	puts_("every leaf size\n");
	for (u32 k = 0; k <= 12; k++) {
		u32 sz = 4096u << k;
		/* One 16M slot each; up to 4M, two pages into it. */
		u32 va = VA_BASE + (k << 24) + (k <= 10 ? 2 * sz : 0);
		u32 pa = PA_BASE + (k <= 10 ? k << 22 : (k - 8) << 24);
		map(k, va, pa, R | W);
		u32 offs[3] = {0, sz / 2 + 4, sz - 4};
		for (u32 i = 0; i < 3; i++)
			*at(pa + offs[i]) = 0x1000 * k + i + 1;
		for (u32 i = 0; i < 3; i++) {
			u32 got = *at(va + offs[i]);
			if (got != 0x1000 * k + i + 1) {
				say("leaf bits", k);
				say("offset", offs[i]);
				fail("read through mapping", got, 0x1000 * k + i + 1);
			}
		}
		*at(va + 8) = 0xabc0 + k;
		CHECK("write through mapping", *at(pa + 8), 0xabc0 + k);
		/* The neighbouring leaf is not mapped. */
		if (k < 12)
			expect_load_fault(va + sz, CAUSE_RPROT, "unmapped neighbour");
	}

	puts_("clrmap\n");
	u32 va = VA_BASE + 2 * 4096;	/* the 4K page from above */
	u32 pa2 = PA_BASE + 0x00100000;
	*at(pa2) = 0x2222;
	CHECK("before", *at(va), 0x1);
	map(0, va, pa2, R | W);
	CHECK("clrmap", hvm_clrmap(va, 4096), 0);
	CHECK("after clrmap", *at(va), 0x2222);

	puts_("permissions\n");
	u32 rva = VA_BASE + 0x0d000000, rpa = PA_BASE + 0x05000000;
	map(0, rva, rpa, R);
	*at(rpa) = 0x77;
	CHECK("read-only read", *at(rva), 0x77);
	expect_store_fault(rva, CAUSE_WPROT, "store to read-only");
	CHECK("unchanged", *at(rpa), 0x77);
	map(0, rva + 4096, rpa + 4096, W);
	expect_load_fault(rva + 4096, CAUSE_RPROT, "load from write-only");
	hvm_store_w(rva + 4096, 0x99);
	CHECK("write-only store", *at(rpa + 4096), 0x99);
	map(0, rva + 8192, rpa + 8192, 0);
	expect_load_fault(rva + 8192, CAUSE_RPROT, "load, no permissions");
	/* A 32M leaf where 16M of page number is left to translate. */
	root[(VA_BASE >> 24) + 14] = leaf(13, PA_BASE, R | W);
	expect_load_fault(VA_BASE + 0x0e000000, CAUSE_RPROT, "leaf too big");

	puts_("execute\n");
	/* "jumpr r31" in a fresh page: no X faults, X runs it. */
	u32 cva = VA_BASE + 0x0d100000, cpa = PA_BASE + 0x05100000;
	*at(cpa) = 0x529fc000;
	CHECK("idsync", hvm_cache(5, cpa, 4), 0);
	map(0, cva, cpa, R);
	{
		u32 before = errors();
		resume_at = (u32)&&returned;
		((void (*)(void))cva)();
returned:
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
	u32 uva = VA_BASE + 0x0d200000, upa = PA_BASE + 0x05200000;
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
	u32 pva = VA_BASE + 0x0d300000;
	map(0, pva, hvm_getinfo(9), R | W); /* INFO_PHYS_ADDR */
	expect_load_fault(pva, CAUSE_RPROT, "read monitor image");
	expect_store_fault(pva, CAUSE_WPROT, "write monitor image");
	map(0, pva + 4096, hvm_getinfo(15), R | W); /* INFO_L2VIC_BASE */
	expect_load_fault(pva + 4096, CAUSE_RPROT, "read monitor device");
	return 0;
}
