/*
 * Copyright (c) Qualcomm Technologies, Inc. and/or its subsidiaries.
 * SPDX-License-Identifier: BSD-3-Clause-Clear
 */

//! Thread context struct.
//!
//! The thread context is a 32-byte-aligned structure that holds all per-thread
//! state: scheduling metadata (next/prev ring pointers, priority, status),
//! architectural registers, guest registers, and arch-conditional fields.
//!
//! The first two fields (next/prev) make the context compatible with the
//! intrusive ring buffer used by the ready list.
//!
//! Layout (v81, 296 bytes):
//! ```text
//!   0: next, prev        (ring pointers)
//!   8: tid, hthread, prio, status
//!  12: vmstatus, base_prio, tlbidxmask, _pad
//!  16: id, vmblock_ptr
//!  24: trapmask, elr
//!  32: tree (rightleft, timeout)
//!  48: cpuint_pending, cpuint_enabled, gevb
//!  56: totalcycles
//!  64: pktcount
//!  72: futex_continuation
//!  80: ssr, ccr
//!  88: r31:30 .. registers .. cs1:cs0, framelimit, framekey
//! 288: dm0 (v68+)
//! 292: vwctrl (v73+)
//! ```

use minivm_types::vm::VmId;

/// Thread context — layout must match assembly offsets in context switch code.
///
/// All pointer-like fields use `u32` to maintain the 296-byte layout
/// regardless of host pointer size (Hexagon is 32-bit).
#[derive(Clone, Copy)]
#[repr(C, align(32))]
pub struct ThreadContext {
    // Ring pointers (offset 0)
    pub next: u32, // *mut ThreadContext as u32
    pub prev: u32, // *mut ThreadContext as u32

    // Status fields (offset 8)
    pub tid: u8,
    pub hthread: u8,
    pub prio: u8,
    pub status: u8,

    // Atomic status word (offset 12)
    pub vmstatus: u8,
    pub base_prio: u8,
    pub tlbidxmask: u8,
    pub _pad0: u8,

    // VM identity (offset 16)
    pub id: VmId,
    pub vmblock: u32, // *mut VmBlock as u32

    // Trap info (offset 24)
    pub trapmask: u32,
    pub elr: u32,

    // Tree node for timeouts (offset 32)
    pub tree_rightleft: u64,
    pub tree_timeout: u64,

    // Interrupts + GEVB (offset 48)
    pub cpuint_pending: u16,
    pub cpuint_enabled: u16,
    pub gevb: u32, // void* as u32

    // Cycle counter (offset 56)
    pub totalcycles: u64,

    // Packet count (offset 64)
    pub pktcount: u64,

    // Futex/continuation (offset 72)
    pub futex_continuation: u64,

    // SSR + CCR (offset 80)
    pub ssr: u32,
    pub ccr: u32,

    // GP registers (offset 88)
    pub r3130: u64,
    pub r2928: u64,
    // offset 104
    pub r1918: u64,
    pub r1716: u64,
    pub usrp30: u64,
    pub r0100: u64,
    // offset 136
    pub r1514: u64,
    pub r1312: u64,
    pub r1110: u64,
    pub r0908: u64,
    // offset 168
    pub r0706: u64,
    pub r0504: u64,
    pub r0302: u64,
    pub lc0sa0: u64,
    // offset 200
    pub gpugp: u64,
    pub lc1sa1: u64,
    pub m1m0: u64,
    // offset 224
    pub r2726: u64,
    pub r2524: u64,
    pub r2322: u64,
    pub r2120: u64,
    // offset 256
    pub gelr: u32,
    pub gssr: u32,
    pub gosp: u32,
    pub gbadva: u32,
    // offset 272
    pub cs1cs0: u64,
    pub framelimit: u32,
    pub framekey: u32,
    // offset 288
    pub dm0: u32, // v68+
    // offset 292
    pub vwctrl: u32, // v73+
                     // offset 296
}

impl ThreadContext {
    /// Create a zeroed thread context.
    pub const fn zeroed() -> Self {
        Self {
            next: 0,
            prev: 0,
            tid: 0,
            hthread: 0,
            prio: 0,
            status: 0,
            vmstatus: 0,
            base_prio: 0,
            tlbidxmask: 0,
            _pad0: 0,
            id: VmId(0),
            vmblock: 0,
            trapmask: 0,
            elr: 0,
            tree_rightleft: 0,
            tree_timeout: 0,
            cpuint_pending: 0,
            cpuint_enabled: 0,
            gevb: 0,
            totalcycles: 0,
            pktcount: 0,
            futex_continuation: 0,
            ssr: 0,
            ccr: 0,
            r3130: 0,
            r2928: 0,
            r1918: 0,
            r1716: 0,
            usrp30: 0,
            r0100: 0,
            r1514: 0,
            r1312: 0,
            r1110: 0,
            r0908: 0,
            r0706: 0,
            r0504: 0,
            r0302: 0,
            lc0sa0: 0,
            gpugp: 0,
            lc1sa1: 0,
            m1m0: 0,
            r2726: 0,
            r2524: 0,
            r2322: 0,
            r2120: 0,
            gelr: 0,
            gssr: 0,
            gosp: 0,
            gbadva: 0,
            cs1cs0: 0,
            framelimit: 0,
            framekey: 0,
            dm0: 0,
            vwctrl: 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::mem;

    #[test]
    fn test_context_size() {
        // v81 context is 296 bytes, but with 32-byte alignment we expect
        // size_of to be rounded up to the next multiple of 32 = 320
        let size = mem::size_of::<ThreadContext>();
        assert_eq!(size, 320);
    }

    #[test]
    fn test_context_alignment() {
        assert_eq!(mem::align_of::<ThreadContext>(), 32);
    }

    #[test]
    fn test_context_field_offsets() {
        let ctx = ThreadContext::zeroed();
        let base = &ctx as *const _ as usize;

        // Verify critical field offsets
        assert_eq!(&ctx.next as *const _ as usize - base, 0);
        assert_eq!(&ctx.prev as *const _ as usize - base, 4);
        assert_eq!(&ctx.tid as *const _ as usize - base, 8);
        assert_eq!(&ctx.hthread as *const _ as usize - base, 9);
        assert_eq!(&ctx.prio as *const _ as usize - base, 10);
        assert_eq!(&ctx.status as *const _ as usize - base, 11);
        assert_eq!(&ctx.vmstatus as *const _ as usize - base, 12);
        assert_eq!(&ctx.id as *const _ as usize - base, 16);
        assert_eq!(&ctx.vmblock as *const _ as usize - base, 20);
        assert_eq!(&ctx.trapmask as *const _ as usize - base, 24);
        assert_eq!(&ctx.elr as *const _ as usize - base, 28);
        assert_eq!(&ctx.tree_rightleft as *const _ as usize - base, 32);
        assert_eq!(&ctx.tree_timeout as *const _ as usize - base, 40);
        assert_eq!(&ctx.cpuint_pending as *const _ as usize - base, 48);
        assert_eq!(&ctx.cpuint_enabled as *const _ as usize - base, 50);
        assert_eq!(&ctx.gevb as *const _ as usize - base, 52);
        assert_eq!(&ctx.totalcycles as *const _ as usize - base, 56);
        assert_eq!(&ctx.pktcount as *const _ as usize - base, 64);
        assert_eq!(&ctx.futex_continuation as *const _ as usize - base, 72);
        assert_eq!(&ctx.ssr as *const _ as usize - base, 80);
        assert_eq!(&ctx.ccr as *const _ as usize - base, 84);
        assert_eq!(&ctx.r3130 as *const _ as usize - base, 88);
        assert_eq!(&ctx.gelr as *const _ as usize - base, 256);
        assert_eq!(&ctx.gssr as *const _ as usize - base, 260);
        assert_eq!(&ctx.gosp as *const _ as usize - base, 264);
        assert_eq!(&ctx.gbadva as *const _ as usize - base, 268);
        assert_eq!(&ctx.cs1cs0 as *const _ as usize - base, 272);
        assert_eq!(&ctx.framelimit as *const _ as usize - base, 280);
        assert_eq!(&ctx.framekey as *const _ as usize - base, 284);
        assert_eq!(&ctx.dm0 as *const _ as usize - base, 288);
        assert_eq!(&ctx.vwctrl as *const _ as usize - base, 292);
    }

    #[test]
    fn test_context_zeroed() {
        let ctx = ThreadContext::zeroed();
        assert_eq!(ctx.next, 0);
        assert_eq!(ctx.prio, 0);
        assert_eq!(ctx.status, 0);
        assert_eq!(ctx.ssr, 0);
    }
}
