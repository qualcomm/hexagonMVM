/*
 * Copyright (c) Qualcomm Technologies, Inc. and/or its subsidiaries.
 * SPDX-License-Identifier: BSD-3-Clause-Clear
 */

//! System-wide constants derived from `max.h` and `Makefile.inc.config`.

// Memory layout
pub const MINIVM_LINK_ADDR: u32 = 0xff000000;
pub const MINIVM_GUEST_START: u32 = 0x02000000;
pub const MINIVM_GUEST_END: u32 = 0xfe000000;

// TLB
pub const MAX_TLB_ENTRIES: u32 = 192;
pub const TLB_ENTRY_C_BITS: u32 = 24;
pub const TLB_ENTRY_GLOBAL_BIT: u32 = 62;
pub const TLB_ENTRY_VALID_BIT: u32 = 63;

// Scheduling
pub const MAX_PRIOS: u32 = 256;
pub const MAX_PRIO: u32 = MAX_PRIOS - 1;
pub const BEST_PRIO: u32 = 0;

// Hardware threads
pub const MAX_HTHREADS: u32 = 16;
pub const MAX_CORES: u32 = 8;

// Interrupts
pub const PERCPU_INTERRUPTS: u32 = 16;
pub const L1_INTERRUPTS: u32 = 8; // v65+
pub const L2_INTERRUPT_START: u32 = 32;
pub const MAX_L2_INTERRUPTS: u32 = 480;
pub const MAX_INTERRUPTS: u32 = 32 + MAX_L2_INTERRUPTS;

pub const RESCHED_INT: u32 = 1;
pub const VM_IPI_INT: u32 = 0;
pub const L2_CORE_INTERRUPT: u32 = 2;
pub const CLUSTER_RESCHED_INT: u32 = 6;
pub const RESCHED_INT_INTMASK: u32 = 1 << RESCHED_INT;
pub const VM_IPI_INTMASK: u32 = 1 << VM_IPI_INT;

// Guest interrupt numbering
pub const MINIVM_TIME_GUESTINT: u32 = 12;
pub const MINIVM_VM_CHILDINT: u32 = 14;

// Virtual address mappings
pub const TEMP_MAP_VA: u32 = 0xff800000;
pub const Q6_SS_BASE_VA: u32 = 0xffc00000;
pub const GPIO_VA: u32 = 0xff7f8000;

// ASID
pub const ASID_BITS: u32 = 7; // v60+
pub const MAX_ASIDS: u32 = 1 << ASID_BITS;

// STLB
pub const STLB_MAX_SETS_LOG2: u32 = 11;
pub const STLB_MAX_SETS: u32 = 1 << STLB_MAX_SETS_LOG2;
pub const STLB_MULT: u32 = 2;
pub const STLB_MAX_WAYS: u32 = 4;
pub const STLB_SHIFT: u32 = 16;
pub const STLB_ENTRIES: u32 = STLB_MAX_SETS * STLB_MAX_WAYS * STLB_MULT;

// SSR bits
pub const SSR_IE_BIT: u32 = 18;
pub const SSR_GUEST_BIT: u32 = 19;
pub const SSR_SS_BIT: u32 = 30;

// Stack
pub const KERNEL_STACK_SIZE: u32 = 8 * 64;

// Allocator
pub const ALLOC_UNIT: u32 = 8;
pub const ALLOC_NUNITS: u32 = 1024;
pub const DEFAULT_ALLOC_HEAP_SIZE: u32 = (ALLOC_NUNITS * ALLOC_UNIT) + ALLOC_UNIT;

// Boot contexts
pub const MAX_BOOT_CONTEXTS: u32 = 1;
pub const INTS_PER_BOOT_CONTEXT: u32 = 32;
pub const BOOT_STACK_SIZE: u32 = 128;

// Build defaults from Makefile.inc.config
pub const MINIVM_KERNEL_PGSIZE: u32 = 3; // SIZE_256K
pub const MINIVM_BOOT_MEM_SIZE: u32 = 0x40000000; // 1GB

// Coprocessor
pub const EXT_HVX_CONTEXTS: u32 = 4;
pub const EXT_HVX_MAX_VLENGTH: u32 = 128;
pub const EXT_HVX_VTCM_OFFSET: u32 = 0x200000;

// Page size limits
pub const PAGE_SIZE_MAX: u8 = 9; // Size1G for v68+

// SSR extension bits
pub const SSR_XA_BITS: u32 = 27;
pub const SSR_XA_NBITS: u32 = 3;
pub const SSR_XE_BIT: u32 = 31;
pub const SSR_XE2_BIT: u32 = 26;

// CCR extension bits
pub const CCR_XE3_BIT: u32 = 28;
pub const CCR_XA3_BITS: u32 = 21;
