/*
 * Copyright (c) Qualcomm Technologies, Inc. and/or its subsidiaries.
 * SPDX-License-Identifier: BSD-3-Clause-Clear
 */

//! Exception entry points: the Hexagon Virtual Machine `trap1` interface,
//! TLB fills, and forwarding of everything else to the guest.

use minivm_mem::translate::{varadix, TranslateCtx};
use minivm_types::asid::{AsidEntry, TranslationType};
use minivm_types::context::ThreadContext;
use minivm_types::regs::Ssr;
use minivm_types::translate::Translation;
use minivm_types::vm::{GSSR_IE, GSSR_UM, VM_VERSION};

use crate::arch;
use crate::hwint;
use crate::mon::{self, Mon, State, VcpuInit, FIRST_GUEST_TLB, MAX_HT};

// Hardware exception causes that are reported to the guest under a
// different code, or raised by the monitor itself.
const CAUSE_XPROT: u32 = 0x11;
const CAUSE_XUSER: u32 = 0x12;
const CAUSE_PRIVILEGE: u32 = 0x1b;
const CAUSE_RPROT: u32 = 0x22;
const CAUSE_WPROT: u32 = 0x23;
const CAUSE_TLBMISS_RW_WRITE: u32 = 0x71;

/// Run a handler for an event taken from a virtual CPU.
///
/// An event taken from anything else — the idle loop, or early boot — has
/// no guest to report to and no business happening.
fn guest_event(ctx: *mut ThreadContext, what: &[u8], f: impl FnOnce(&mut Mon, usize)) {
    let cpu = arch::htid();
    if !mon::is_vcpu_ctx(cpu, ctx) {
        let (ssr, elr) = unsafe { ((*ctx).ssr, (*ctx).elr) };
        crate::fatal(what, ssr, elr);
    }
    let mut m = mon::lock();
    f(&mut m, cpu);
    m.leave(cpu);
}

#[no_mangle]
pub extern "C" fn minivm_trap1(ctx: *mut ThreadContext) {
    guest_event(ctx, b"trap1 in the monitor", |m, cpu| {
        let trap = m.cpus[cpu].ctx.ssr & 0xff;
        if !m.cpus[cpu].guest_kernel() {
            // The virtual machine interface belongs to the guest kernel.
            m.cpus[cpu].error_event(0, CAUSE_PRIVILEGE);
            return;
        }
        trap1(m, cpu, trap);
    });
}

fn trap1(m: &mut Mon, cpu: usize, trap: u32) {
    let (r0, r1, r2, r3) = {
        let c = &m.cpus[cpu];
        (c.r(0), c.r(1), c.r(2), c.r(3))
    };
    match trap {
        // vmversion
        0 => m.cpus[cpu].set_r0(VM_VERSION),
        // vmrte: return from an event
        1 => {
            let c = &mut m.cpus[cpu];
            let gssr = c.ctx.gssr;
            if gssr & GSSR_UM != 0 {
                let sp = (c.ctx.r2928 >> 32) as u32;
                c.ctx.r2928 = ((c.ctx.gosp as u64) << 32) | (c.ctx.r2928 & 0xFFFF_FFFF);
                c.ctx.gosp = sp;
                c.ctx.ssr &= !(1 << Ssr::GUEST_BIT);
            }
            c.ctx.elr = c.ctx.gelr;
            if gssr & GSSR_IE != 0 {
                // May turn straight into the next interrupt event.
                m.enable_ie(cpu);
            }
        }
        // vmsetvec
        2 => {
            m.cpus[cpu].ctx.gevb = r0;
            m.cpus[cpu].set_r0(0);
        }
        // vmsetie
        3 => {
            let prev = if r0 & 1 != 0 {
                m.enable_ie(cpu)
            } else {
                m.disable_ie(cpu)
            };
            // If enabling took an interrupt, the guest sees this return
            // value when its handler returns here.
            m.cpus[cpu].set_r0(prev as u32);
        }
        // vmgetie
        4 => {
            let ie = m.cpus[cpu].ie();
            m.cpus[cpu].set_r0(ie as u32);
        }
        // vmintop
        5 => {
            let ret = m.intop(cpu, r0, r1, r2);
            m.cpus[cpu].set_r0(ret as u32);
        }
        // vmclrmap
        10 => {
            clrmap(m, cpu, r0, r1);
            m.cpus[cpu].set_r0(0);
        }
        // vmnewmap
        11 => {
            let ret = newmap(m, cpu, r0, r1, r2 != 0, r3);
            m.cpus[cpu].set_r0(ret);
        }
        // vmcache: nothing to do for a cache that is not modelled
        13 => m.cpus[cpu].set_r0(0),
        // vmgettime
        14 => {
            m.cpus[cpu].ctx.r0100 = m.now().wrapping_add(m.cycle_offset);
            m.cpus[cpu].ctx.r0302 = m.pktcount;
        }
        // vmsettime
        15 => {
            m.cycle_offset = m.cpus[cpu].ctx.r0100.wrapping_sub(m.now());
            m.pktcount = m.cpus[cpu].ctx.r0302;
        }
        // vmwait
        16 => match m.do_work(cpu) {
            // Either it was just taken as an event or the guest's
            // interrupts are off; vmwait returns its number in both cases.
            Some(intno) => m.cpus[cpu].set_r0(intno),
            None => m.cpus[cpu].state = State::Waiting,
        },
        // vmyield: every virtual CPU has a hardware thread to itself
        17 => m.cpus[cpu].set_r0(0),
        // vmstart
        18 => {
            let ret = start(m, cpu, r0, r1);
            m.cpus[cpu].set_r0(ret);
        }
        // vmstop
        19 => stop(m, cpu, r0),
        // vmvpid
        20 => m.cpus[cpu].set_r0(Mon::vpid(cpu).0),
        // vmsetregs
        21 => {
            let c = &mut m.cpus[cpu].ctx;
            (c.gelr, c.gssr, c.gosp, c.gbadva) = (r0, r1, r2, r3);
        }
        // vmgetregs
        22 => {
            let c = &mut m.cpus[cpu].ctx;
            c.r0100 = ((c.gssr as u64) << 32) | c.gelr as u64;
            c.r0302 = ((c.gbadva as u64) << 32) | c.gosp as u64;
        }
        // vmtimerop
        24 => {
            let arg = m.cpus[cpu].ctx.r0302;
            m.cpus[cpu].ctx.r0100 = m.timerop(cpu, r0, arg);
        }
        // vmpmuconfig
        25 => m.cpus[cpu].set_r0(0),
        // Not a virtual instruction: an error event, as for one from user
        // mode.
        _ => m.cpus[cpu].error_event(0, CAUSE_PRIVILEGE),
    }
}

/// `vmstart`: a new virtual CPU at `pc`, sharing the caller's address space
/// and event vector. Returns its id, or -1.
fn start(m: &mut Mon, me: usize, pc: u32, sp: u32) -> u32 {
    if pc & 3 != 0 || sp & 7 != 0 {
        return !0;
    }
    let Some(cpu) = m.free_cpu() else {
        return !0;
    };
    let (ssr, usr, ccr, gevb) = {
        let c = &m.cpus[me].ctx;
        (c.ssr, (c.usrp30 >> 32) as u32, c.ccr, c.gevb)
    };
    // The new CPU holds its own reference to the address space.
    let asid = Ssr::new(ssr).asid() as u32;
    let e = *m.asids.get(asid);
    let Some(ty) = TranslationType::from_raw(e.trans_type()) else {
        return !0;
    };
    m.asids.inc(e.ptb, ty, false, e.extra(), e.vmid(), |_| {});
    // Its extension enables start clear, so that first use faults to the
    // guest.
    m.create_vcpu(
        cpu,
        VcpuInit {
            pc,
            sp,
            arg: 0,
            asid,
            usr,
            ccr,
            gevb,
        },
    );
    m.wake(cpu);
    Mon::vpid(cpu).0
}

/// `vmstop`: the calling virtual CPU is done. When the last one goes, so
/// does the machine.
fn stop(m: &mut Mon, cpu: usize, status: u32) {
    let asid = Ssr::new(m.cpus[cpu].ctx.ssr).asid() as u32;
    m.asids.dec(asid);
    m.cpus[cpu].state = State::Off;
    m.cancel_timeout(cpu);
    m.num_cpus -= 1;
    m.stop_reason = m.stop_reason.merge(status);
    if m.num_cpus == 0 {
        m.begin_shutdown(cpu, m.stop_reason.exit_code());
    }
}

/// `vmnewmap`: switch the calling virtual CPU to another address space.
/// For a varadix map, `extra` is the width of the virtual page number.
fn newmap(m: &mut Mon, cpu: usize, ptb: u32, ty: u32, invalidate: bool, extra: u32) -> u32 {
    let Some(ty) = u8::try_from(ty).ok().and_then(TranslationType::from_raw) else {
        return !0;
    };
    // Only varadix has a fourth argument; Linux leaves whatever was in r3,
    // which must not make otherwise identical maps different spaces. A
    // varadix map that cannot translate anything fails here, before the
    // guest loses the space it is running in.
    let extra = match ty {
        TranslationType::Varadix if varadix::valid_root(ptb, extra) => extra as u8,
        TranslationType::Varadix => return !0,
        _ => 0,
    };
    let old = Ssr::new(m.cpus[cpu].ctx.ssr).asid() as u32;
    let vmid = m.asids.get(old).vmid();
    // A recycled ASID, or one the guest says has changed, must not keep
    // translations from before.
    let Some(new) = m
        .asids
        .inc(ptb, ty, invalidate, extra, vmid, |asid| unsafe {
            arch::tlbinvasid(asid)
        })
    else {
        return !0;
    };
    m.asids.dec(old);
    let c = &mut m.cpus[cpu].ctx;
    c.ssr = Ssr::new(c.ssr).with_asid(new as u8).0;
    0
}

/// `vmclrmap`: the guest changed the translations of the `bytes` bytes
/// at `va`.
fn clrmap(m: &mut Mon, cpu: usize, va: u32, bytes: u32) {
    let pages = minivm_mem::tlb_fill::pages_spanned(va, bytes);
    if pages == 0 {
        return;
    }
    let asid = Ssr::new(m.cpus[cpu].ctx.ssr).asid() as u32;
    unsafe {
        if pages > 1 {
            arch::tlbinvasid(asid);
        } else {
            let idx = arch::tlbp((asid << 20) | (va >> 12));
            // A hit below FIRST_GUEST_TLB is one of the monitor's own.
            if idx & 0x8000_0000 == 0 && idx >= FIRST_GUEST_TLB {
                arch::tlbw(0, idx);
            }
        }
    }
}

#[no_mangle]
pub extern "C" fn minivm_trap0(ctx: *mut ThreadContext) {
    let cpu = arch::htid();
    if !mon::is_vcpu_ctx(cpu, ctx) {
        // Semihosting from the monitor's own boot code: the simulator has
        // already done the work.
        return;
    }
    let mut m = mon::lock();
    let trapno = m.cpus[cpu].ctx.ssr & 0xff;
    if trapno == 0 && m.cpus[cpu].ctx.gevb == 0 {
        // Semihosting from a guest too simple to have an event vector.
    } else {
        m.cpus[cpu].trap0_event(trapno);
    }
    m.leave(cpu);
}

#[no_mangle]
pub extern "C" fn minivm_error(ctx: *mut ThreadContext) {
    let badva = arch::badva();
    guest_event(ctx, b"exception in the monitor", |m, cpu| {
        let cause = m.cpus[cpu].ctx.ssr & 0xff;
        // The guest takes the faulting address from GBADVA, but an
        // execute-permission fault does not set BADVA (hexagon-sim leaves
        // it alone): the address is the packet's, in ELR.
        let addr = match cause {
            CAUSE_XPROT | CAUSE_XUSER => m.cpus[cpu].ctx.elr,
            _ => badva,
        };
        m.cpus[cpu].error_event(addr, cause);
    });
}

struct Fences {
    lo: u32,
    hi: u32,
}

impl Fences {
    /// Whether the physical byte range [pa, pa + len) is inside the fences.
    fn contains(&self, pa: u64, len: u64) -> bool {
        let lo = (self.lo as u64) << 12;
        let end = (self.hi as u64 + 1) << 12;
        pa >= lo && pa.checked_add(len).is_some_and(|e| e <= end)
    }
}

impl TranslateCtx for Fences {
    fn translate(&self, input: Translation, info: AsidEntry) -> Translation {
        minivm_mem::translate::translate(self, input, info)
    }
    fn vmblock_guestmap(&self, _vmidx: u8) -> AsidEntry {
        AsidEntry::EMPTY
    }
    fn vmblock_fence_lo(&self, _vmidx: u8) -> u32 {
        self.lo
    }
    fn vmblock_fence_hi(&self, _vmidx: u8) -> u32 {
        self.hi
    }
    fn tcm_range(&self) -> (u32, u32) {
        (0, 0)
    }
    fn vtcm_range(&self) -> (u32, u32) {
        (0, 0)
    }
    fn physread_dword(&self, pa: u64) -> u64 {
        // A paging structure outside the fences reads as an invalid entry,
        // so a guest cannot make the monitor touch other physical memory.
        if !self.contains(pa, 8) {
            return 0;
        }
        // A page-table walk must not itself miss in the TLB.
        arch::phys_read32(pa) as u64 | ((arch::phys_read32(pa + 4) as u64) << 32)
    }
}

#[no_mangle]
pub extern "C" fn minivm_tlbmiss(ctx: *mut ThreadContext) {
    let badva = arch::badva();
    guest_event(ctx, b"TLB miss in the monitor", |m, cpu| {
        let ssr = m.cpus[cpu].ctx.ssr;
        // An instruction fetch miss reports its address through ELR, not
        // BADVA (hexagon-sim leaves BADVA alone; QEMU happens to set it):
        // the packet at ELR, or for cause 0x61 the next page, which a
        // packet starting at ELR runs into. Only icinva (0x62) and data
        // accesses use BADVA.
        let va = match ssr & 0xff {
            0x60 => m.cpus[cpu].ctx.elr,
            0x61 => m.cpus[cpu].ctx.elr.wrapping_add(16),
            _ => badva,
        };
        let asid = Ssr::new(ssr).asid() as u32;
        let info = *m.asids.get(asid);
        let fences = Fences {
            lo: m.fence_lo,
            hi: m.fence_hi,
        };
        let entry = if info.is_empty() {
            0
        } else {
            let mut trans =
                minivm_mem::translate::translate(&fences, Translation::default_for_va(va), info);
            let arch = m.plat.arch.expect("architecture validated at boot");
            let max_size = arch.max_page_size() as u8;
            if trans.size() > max_size {
                trans = trans.with_size(max_size);
            }
            let monitor = (m.phys_base as u64, m.phys_end as u64);
            let devices = (m.devices_base as u64, m.devices_base as u64 + 0x40_0000);
            // Reservations keep a cooperative OS allocator away from these
            // pages; this check is the actual guest access-control boundary.
            // It also holds every translation type to the fences. A large
            // page that reaches either is cut down to what is clear.
            trans = minivm_mem::tlb_fill::confine(
                trans,
                max_size,
                (fences.lo, fences.hi),
                &[monitor, devices],
            );
            // An entry is installed with whatever permissions the guest's
            // mapping has; an access those do not allow then takes a
            // protection exception, which goes to the guest as is.
            minivm_mem::tlb_fill::tlbfmt_from_translation(
                trans,
                va,
                asid,
                max_size,
                arch >= minivm_types::arch::ArchVersion::V73,
            )
        };
        if entry == 0 {
            // No mapping at all: a page fault, named after the access.
            let cause = match ssr & 0xff {
                0x60..=0x62 => CAUSE_XPROT,
                CAUSE_TLBMISS_RW_WRITE => CAUSE_WPROT,
                _ => CAUSE_RPROT,
            };
            m.cpus[cpu].error_event(va, cause);
            return;
        }
        // The TLB is shared by every hardware thread, so another virtual
        // CPU may have filled this page already; a second entry for it
        // would be a multiple match.
        if arch::tlbp((asid << 20) | (va >> 12)) & 0x8000_0000 != 0 {
            let idx = m.tlb_next;
            m.tlb_next = if idx + 1 >= m.tlb_size {
                FIRST_GUEST_TLB
            } else {
                idx + 1
            };
            unsafe { arch::tlbw(entry, idx) };
        }
    });
}

#[no_mangle]
pub extern "C" fn minivm_interrupt(ctx: *mut ThreadContext) {
    let cpu = arch::htid();
    let core_int = unsafe { (*ctx).ssr } & 0x1f;
    let mut m = mon::lock();
    hwint::interrupt(&mut m, cpu, core_int);
    m.leave(cpu);
}

#[no_mangle]
pub extern "C" fn minivm_unexpected(ctx: *mut ThreadContext) {
    let (ssr, elr) = unsafe { ((*ctx).ssr, (*ctx).elr) };
    crate::fatal(b"unexpected event", ssr, elr);
}

const _: () = assert!(MAX_HT <= 8);
