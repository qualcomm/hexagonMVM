/*
 * Copyright (c) Qualcomm Technologies, Inc. and/or its subsidiaries.
 * SPDX-License-Identifier: BSD-3-Clause-Clear
 */

//! Monitor state: the virtual CPUs and the rule for what a hardware thread
//! runs when a handler returns.
//!
//! Every virtual CPU is pinned to one hardware thread for its whole life, so
//! there is no scheduler and nothing is ever migrated: registers the core
//! keeps per thread (HVX state, frame key, ...) simply stay where they are.
//!
//! All of this is guarded by one lock, taken on every entry to the monitor.
//! The monitor only ever runs inside an exception handler, with the core's
//! own interrupts blocked, so the lock is never contended by the thread that
//! holds it.

use minivm_mem::asid::AsidTable;
use minivm_types::context::ThreadContext;
use minivm_types::regs::Ssr;
use minivm_types::stop::StopReason;
use minivm_types::vm::{VmId, GSSR_IE, GSSR_UM, VMSTATUS_IE};

use crate::arch;
use crate::discovery::Platform;
use core::ops::{Deref, DerefMut};
use core::sync::atomic::{AtomicU32, Ordering};

/// Hardware threads the monitor has room for.
pub const MAX_HT: usize = 8;
/// VM index reported in virtual CPU ids. It must not be 0: `vmintop(post)`
/// takes a zero id to mean "the caller".
pub const VM_IDX: u8 = 2;

/// Hardware TLB indexes below this are wired by the monitor.
pub const FIRST_GUEST_TLB: u32 = 8;

const GEVB_ERROR: u32 = 8;
const GEVB_TRAP0: u32 = 20;

/// One hardware thread and the virtual CPU pinned to it.
#[repr(C)]
pub struct Cpu {
    /// The virtual CPU's registers while the monitor runs. SGP0 points here
    /// whenever the guest executes.
    pub ctx: ThreadContext,
}

impl Cpu {
    const fn new() -> Self {
        Self {
            ctx: ThreadContext::zeroed(),
        }
    }

    pub fn ie(&self) -> bool {
        self.ctx.vmstatus & VMSTATUS_IE != 0
    }

    pub fn r(&self, n: u32) -> u32 {
        let pair = match n / 2 {
            0 => self.ctx.r0100,
            1 => self.ctx.r0302,
            _ => self.ctx.r0504,
        };
        (pair >> (32 * (n & 1))) as u32
    }

    pub fn set_r0(&mut self, v: u32) {
        self.ctx.r0100 = (self.ctx.r0100 & !0xFFFF_FFFF) | v as u64;
    }

    fn r29(&self) -> u32 {
        (self.ctx.r2928 >> 32) as u32
    }

    fn set_r29(&mut self, v: u32) {
        self.ctx.r2928 = ((v as u64) << 32) | (self.ctx.r2928 & 0xFFFF_FFFF);
    }

    /// Whether the virtual CPU was in guest-kernel mode when it trapped.
    pub fn guest_kernel(&self) -> bool {
        self.ctx.ssr & (1 << Ssr::GUEST_BIT) != 0
    }

    /// Deliver an event to the guest: record where it was and what happened
    /// in its guest registers and continue at its event vector, in kernel
    /// mode with its interrupts off..
    pub fn vm_event(&mut self, gbadva: u32, cause: u32, vec_offset: u32) {
        if self.ctx.gevb == 0 {
            crate::fatal(
                b"event for a guest with no event vector",
                cause,
                self.ctx.elr,
            );
        }
        self.ctx.gelr = self.ctx.elr;
        self.ctx.elr = self.ctx.gevb + vec_offset;
        self.ctx.gbadva = gbadva;
        let mut gssr = cause;
        if !self.guest_kernel() {
            let sp = self.r29();
            self.set_r29(self.ctx.gosp);
            self.ctx.gosp = sp;
            self.ctx.ssr |= 1 << Ssr::GUEST_BIT;
            gssr |= GSSR_UM;
        }
        if self.ie() {
            gssr |= GSSR_IE;
        }
        self.ctx.gssr = gssr;
        self.ctx.vmstatus &= !VMSTATUS_IE;
    }

    pub fn error_event(&mut self, gbadva: u32, cause: u32) {
        self.vm_event(gbadva, cause, GEVB_ERROR);
    }

    pub fn trap0_event(&mut self, trapno: u32) {
        self.vm_event(0, trapno, GEVB_TRAP0);
    }
}

/// What a new virtual CPU starts with.
pub struct VcpuInit {
    pub pc: u32,
    pub sp: u32,
    /// Initial r0.
    pub arg: u32,
    /// Address space; the caller has taken the reference.
    pub asid: u32,
    pub usr: u32,
    pub ccr: u32,
    /// Event vector, or 0 for the guest to set.
    pub gevb: u32,
}

pub struct Mon {
    pub cpus: [Cpu; MAX_HT],
    /// Live virtual CPUs.
    pub num_cpus: u32,
    pub asids: AsidTable,
    pub tlb_next: u32,
    pub tlb_size: u32,
    pub plat: Platform,
    /// Where the monitor image is loaded.
    pub phys_base: u32,
    /// Where it ends.
    pub phys_end: u32,
    /// Guest-physical page range the VM may map.
    pub fence_lo: u32,
    pub fence_hi: u32,
    /// Set once the VM is gone; every thread stops when it sees this.
    pub shutdown: bool,
    pub exit_code: u32,
    /// Strongest HVM stop reason reported by a vCPU so far.
    pub stop_reason: StopReason,
}

static mut MON: Mon = Mon::new();
/// Hardware threads currently owning the kernel lock. There can only be one;
/// the bitmap lets fatal shutdown distinguish locked and unlocked callers.
static LOCK_OWNER: AtomicU32 = AtomicU32::new(0);

/// SYSCFG.K0LOCK: some thread holds the kernel lock.
const SYSCFG_K0LOCK: u32 = 1 << 12;
/// How many times `lock` looks at it before stalling.
const LOCK_SPINS: u32 = 100;

/// Take the monitor lock and return the monitor state.
///
/// The lock is the core's kernel lock. A thread waiting for it
/// stalls rather than spinning, and it is handed on round-robin. It used
/// to be a spin lock (`memw_locked`), which on QEMU with fewer host cores
/// than hardware threads let the waiters take the host CPU from the thread
/// holding the lock: Linux made no progress on 4 host cores.
pub fn lock() -> MonGuard {
    // A short wait is cheaper spent watching for the lock to come free: on
    // QEMU a thread that stalls in `k0lock` is put to sleep and woken, which
    // made Linux boot twice as slowly with a host core per thread. Kept
    // short, so that waiters still give way on a host with few cores.
    for _ in 0..LOCK_SPINS {
        if arch::syscfg() & SYSCFG_K0LOCK == 0 {
            break;
        }
    }
    // Not `nomem`: this orders every access to the monitor's state.
    unsafe { core::arch::asm!("k0lock", options(nostack)) };
    let cpu = arch::htid();
    LOCK_OWNER.fetch_or(1 << cpu, Ordering::Release);
    // Safety: the lock is held, and handlers do not nest.
    MonGuard {
        mon: core::ptr::addr_of_mut!(MON),
        cpu,
        held: true,
    }
}

/// The monitor state, for as long as the guard holds the monitor lock.
pub struct MonGuard {
    mon: *mut Mon,
    cpu: usize,
    held: bool,
}

impl Deref for MonGuard {
    type Target = Mon;
    fn deref(&self) -> &Mon {
        // Safety: construction requires the global kernel lock, and the guard
        // holds it for the lifetime of the returned reference.
        unsafe { &*self.mon }
    }
}

impl DerefMut for MonGuard {
    fn deref_mut(&mut self) -> &mut Mon {
        // Safety: the guard is the unique owner of the global kernel lock.
        unsafe { &mut *self.mon }
    }
}

impl MonGuard {
    fn release(&mut self) {
        if self.held {
            LOCK_OWNER.fetch_and(!(1 << self.cpu), Ordering::Release);
            unsafe { core::arch::asm!("k0unlock", options(nostack)) };
            self.held = false;
        }
    }

    /// Choose what hardware thread `cpu` runs when the current handler
    /// returns, and release the lock. Every handler ends here.
    pub fn leave(mut self, cpu: usize) {
        if self.shutdown {
            let code = self.exit_code;
            self.release();
            unsafe { arch::minivm_stop_thread(code) };
        }
        let next = &mut self.cpus[cpu].ctx;
        unsafe { arch::set_sgp0(next as *mut ThreadContext as u32) };
        self.release();
    }
}

impl Drop for MonGuard {
    fn drop(&mut self) {
        self.release();
    }
}

/// Whether `ctx` is the context of the virtual CPU pinned to hardware
/// thread `cpu`, as opposed to an idle or boot context.
pub fn is_vcpu_ctx(cpu: usize, ctx: *const ThreadContext) -> bool {
    core::ptr::eq(ctx, unsafe { core::ptr::addr_of!(MON.cpus[cpu].ctx) })
}

/// Bring the machine down from a thread that cannot go on, whether or not
/// it holds the lock.
pub fn abandon(code: u32) -> ! {
    let cpu = arch::htid();
    if LOCK_OWNER.load(Ordering::Acquire) & (1 << cpu) != 0 {
        let m = unsafe { &mut *core::ptr::addr_of_mut!(MON) };
        m.begin_shutdown(cpu, code);
        LOCK_OWNER.fetch_and(!(1 << cpu), Ordering::Release);
        unsafe { core::arch::asm!("k0unlock", options(nostack)) };
    } else {
        let mut m = lock();
        m.begin_shutdown(cpu, code);
        m.release();
    }
    unsafe { arch::minivm_stop_thread(code) };
}

impl Mon {
    const fn new() -> Self {
        Self {
            cpus: [const { Cpu::new() }; MAX_HT],
            num_cpus: 0,
            asids: AsidTable::new(),
            tlb_next: FIRST_GUEST_TLB,
            tlb_size: 0,
            plat: Platform::empty(),
            phys_base: 0,
            phys_end: 0,
            fence_lo: 0,
            fence_hi: 0xFE000,
            shutdown: false,
            exit_code: 0,
            stop_reason: StopReason::None,
        }
    }

    /// Id of the virtual CPU on hardware thread `cpu`.
    pub fn vpid(cpu: usize) -> VmId {
        VmId::new(VM_IDX, cpu as u16, 0)
    }

    // ---- virtual interrupt controller ----

    /// Turn the guest's interrupts on. Returns whether they were on
    /// already.
    pub fn enable_ie(&mut self, cpu: usize) -> bool {
        let prev = self.cpus[cpu].ie();
        self.cpus[cpu].ctx.vmstatus |= VMSTATUS_IE;
        prev
    }

    pub fn disable_ie(&mut self, cpu: usize) -> bool {
        let prev = self.cpus[cpu].ie();
        self.cpus[cpu].ctx.vmstatus &= !VMSTATUS_IE;
        prev
    }

    // ---- virtual CPU lifecycle ----

    /// Give hardware thread `cpu` a virtual CPU. It always starts in
    /// guest-kernel mode with its own interrupts off.
    pub fn create_vcpu(&mut self, cpu: usize, init: VcpuInit) {
        let c = &mut self.cpus[cpu];
        c.ctx = ThreadContext::zeroed();
        c.ctx.elr = init.pc;
        c.ctx.r2928 = (init.sp as u64) << 32;
        c.ctx.r0100 = init.arg as u64;
        // EX keeps the restore path privileged until its `rte`. The core's
        // own interrupt enable stays on for as long as the guest runs: the
        // guest's interrupt enable is virtual (VMSTATUS_IE).
        c.ctx.ssr = ((init.asid << Ssr::ASID_SHIFT) & Ssr::ASID_MASK)
            | (1 << Ssr::EX_BIT)
            | (1 << Ssr::UM_BIT)
            | (1 << Ssr::GUEST_BIT)
            | (1 << Ssr::IE_BIT);
        c.ctx.usrp30 = (init.usr as u64) << 32;
        c.ctx.ccr = init.ccr;
        c.ctx.gevb = init.gevb;
        c.ctx.id = Self::vpid(cpu);
        self.num_cpus += 1;
    }

    /// Start bringing the machine down: every hardware thread stops as soon
    /// as it next leaves the monitor.
    pub fn begin_shutdown(&mut self, _me: usize, code: u32) {
        self.shutdown = true;
        self.exit_code = code;
    }
}
