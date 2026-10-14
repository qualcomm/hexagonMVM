/*
 * Copyright (c) Qualcomm Technologies, Inc. and/or its subsidiaries.
 * SPDX-License-Identifier: BSD-3-Clause-Clear
 */

//! Monitor state: the virtual CPUs, the VM's virtual interrupt controller,
//! and the rule for what a hardware thread runs when a handler returns.
//!
//! Every virtual CPU is pinned to one hardware thread for its whole life, so
//! there is no scheduler and nothing is ever migrated: registers the core
//! keeps per thread (HVX state, frame key, ...) simply stay where they are.
//! A hardware thread without a runnable virtual CPU sits in the idle loop.
//!
//! All of this is guarded by one lock, taken on every entry to the monitor.
//! The monitor only ever runs inside an exception handler, with the core's
//! own interrupts blocked, so the lock is never contended by the thread that
//! holds it.
//!
//! The virtual interrupt controller has the semantics the Linux port was
//! written against. A
//! per-CPU interrupt is taken when it is pending and enabled; it has no
//! local enable. A shared one must also be locally enabled on the CPU that
//! takes it, and starts locally disabled everywhere. Taking an interrupt
//! clears its enable, and the guest sets that again when it has handled
//! it. Posting only makes an interrupt pending.

use minivm_mem::asid::AsidTable;
use minivm_types::context::ThreadContext;
use minivm_types::regs::Ssr;
use minivm_types::stop::StopReason;
use minivm_types::vm::{VmId, GSSR_IE, GSSR_UM, VMSTATUS_IE, VMSTATUS_VMWORK};
use minivm_types::vmint::{clear_rearms_physical, IntOp};

use crate::arch;
use crate::discovery::Platform;
use core::ops::{Deref, DerefMut};
use core::sync::atomic::{AtomicU32, Ordering};

/// Hardware threads the monitor has room for.
pub const MAX_HT: usize = 8;
/// Interrupts 0..16 are private to each virtual CPU; the rest are shared.
pub const PERCPU_INTS: u32 = 16;
/// Shared interrupts: 288 device lines and 32 more.
pub const SHARED_INTS: u32 = 320;
const SH_WORDS: usize = (SHARED_INTS as usize) / 32;
/// The per-CPU interrupt a `vmtimerop` timeout arrives on.
pub const TIME_GUESTINT: u32 = 12;
/// VM index reported in virtual CPU ids. It must not be 0: `vmintop(post)`
/// takes a zero id to mean "the caller".
pub const VM_IDX: u8 = 2;

/// Core interrupt that makes a hardware thread enter the monitor. There is
/// one for all threads, steered with `iassignw` to the threads that have
/// been sent it.
pub const IPI_INT: u32 = 0;
/// Core interrupt the L2VIC funnels its lines into.
pub const L2_CORE_INT: u32 = 2;

/// Hardware TLB indexes below this are wired by the monitor.
pub const FIRST_GUEST_TLB: u32 = 8;
pub mod tlb {
    //! Wired TLB entries. Entry 0 is the monitor image, written by entry.S.
    pub const DEVICES: u32 = 1;
    pub const UART: u32 = 2;
    pub const FDT0: u32 = 3;
    pub const FDT1: u32 = 4;
}

/// Monitor virtual addresses. The image takes the first megabyte at
/// 0xff000000; everything else the monitor touches is wired above it.
pub mod va {
    pub const UART: u32 = 0xff10_0000;
    pub const DEVICES: u32 = 0xff40_0000;
    pub const FDT: u32 = 0xff80_0000;
}

const GEVB_ERROR: u32 = 8;
const GEVB_TRAP0: u32 = 20;
const GEVB_INTERRUPT: u32 = 28;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum State {
    /// No virtual CPU on this hardware thread.
    Off,
    /// The virtual CPU is executing (or is about to).
    Running,
    /// The virtual CPU is in `vmwait`.
    Waiting,
}

/// One hardware thread and the virtual CPU pinned to it.
#[repr(C)]
pub struct Cpu {
    /// The virtual CPU's registers while the monitor runs. SGP0 points here
    /// whenever the guest executes.
    pub ctx: ThreadContext,
    /// Context of the idle loop.
    pub idle: ThreadContext,
    pub state: State,
    /// The hardware thread is up and has been through `secondary_main`.
    pub started: bool,
    /// `vmtimerop` timeout in timer ticks; 0 is none.
    pub timeout: u64,
}

impl Cpu {
    const fn new() -> Self {
        Self {
            ctx: ThreadContext::zeroed(),
            idle: ThreadContext::zeroed(),
            state: State::Off,
            started: false,
            timeout: 0,
        }
    }

    /// Per-CPU interrupts that can be taken now. They have no local
    /// enable: they belong to one CPU already.
    fn cpuint_ready(&self) -> u16 {
        self.ctx.cpuint_pending & self.ctx.cpuint_enabled
    }

    pub fn ie(&self) -> bool {
        self.ctx.vmstatus & VMSTATUS_IE != 0
    }

    fn work(&self) -> bool {
        self.ctx.vmstatus & VMSTATUS_VMWORK != 0
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

    /// Point the idle context at the top of the idle loop: monitor mode,
    /// interrupts on. EX is set so the restore path stays privileged until
    /// its `rte`.
    fn reset_idle(&mut self) {
        extern "C" {
            fn minivm_idle_loop();
        }
        self.idle.elr = minivm_idle_loop as *const () as usize as u32;
        self.idle.ssr = (1 << Ssr::EX_BIT) | (1 << Ssr::IE_BIT);
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
    /// Shared interrupt state, indexed by interrupt number - PERCPU_INTS.
    pending: [u32; SH_WORDS],
    enable: [u32; SH_WORDS],
    percpu_mask: [[u32; SH_WORDS]; MAX_HT],
    /// Live virtual CPUs.
    pub num_cpus: u32,
    /// Hardware threads that exist.
    pub hthreads: u32,
    /// Hardware threads owed a wake-up interrupt.
    ipi_pending: u32,
    pub asids: AsidTable,
    pub tlb_next: u32,
    pub tlb_size: u32,
    pub plat: Platform,
    /// Where the monitor image is loaded.
    pub phys_base: u32,
    /// Where it ends.
    pub phys_end: u32,
    /// Physical 4 MiB window containing monitor-owned interrupt/timer MMIO.
    pub devices_base: u32,
    /// Guest-physical page range the VM may map.
    pub fence_lo: u32,
    pub fence_hi: u32,
    /// Timer tick the QTIMER match is programmed for.
    pub next_ticks: u64,
    pub last_ticks: u64,
    /// `vmsettime` state: what to add to the timer to get the guest's
    /// cycle count, and the packet count it asked for.
    pub cycle_offset: u64,
    pub pktcount: u64,
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
        if self.cpus[cpu].state == State::Running {
            self.do_work(cpu);
        }
        let c = &mut self.cpus[cpu];
        let next = if c.state == State::Running {
            &mut c.ctx
        } else {
            c.reset_idle();
            &mut c.idle
        };
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
            pending: [0; SH_WORDS],
            enable: [0; SH_WORDS],
            // Shared interrupts start locally disabled everywhere.
            percpu_mask: [[0; SH_WORDS]; MAX_HT],
            num_cpus: 0,
            hthreads: 1,
            ipi_pending: 0,
            asids: AsidTable::new(),
            tlb_next: FIRST_GUEST_TLB,
            tlb_size: 0,
            plat: Platform::empty(),
            phys_base: 0,
            phys_end: 0,
            devices_base: 0,
            fence_lo: 0,
            fence_hi: 0xFE000,
            next_ticks: !0,
            last_ticks: 0,
            cycle_offset: 0,
            pktcount: 0,
            shutdown: false,
            exit_code: 0,
            stop_reason: StopReason::None,
        }
    }

    /// Id of the virtual CPU on hardware thread `cpu`.
    pub fn vpid(cpu: usize) -> VmId {
        VmId::new(VM_IDX, cpu as u16, 0)
    }

    /// Hardware thread running the virtual CPU with this id.
    pub fn cpu_of(&self, id: u32) -> Option<usize> {
        let id = VmId(id);
        let cpu = id.cpuidx() as usize;
        (id.vmidx() == VM_IDX && cpu < MAX_HT && self.cpus[cpu].state != State::Off).then_some(cpu)
    }

    /// Make hardware thread `cpu` enter the monitor..
    fn kick(&mut self, cpu: usize) {
        if self.cpus[cpu].started {
            self.ipi_pending |= 1 << cpu;
            Self::steer_ipi(self.ipi_pending);
            arch::swi(1 << IPI_INT);
        }
    }

    /// Let exactly the threads in `threads` take the wake-up interrupt.
    fn steer_ipi(threads: u32) {
        unsafe { arch::iassignw(IPI_INT, !threads) };
    }

    /// Hardware thread `cpu` took the wake-up interrupt. Stop steering it
    /// there, and raise it again if another thread is still owed one: it
    /// is a single interrupt, and taking it auto-disabled it for everybody.
    pub fn ipi_received(&mut self, cpu: usize) {
        self.ipi_pending &= !(1 << cpu);
        Self::steer_ipi(self.ipi_pending);
        self.ack(cpu, 1 << IPI_INT);
        if self.ipi_pending != 0 {
            arch::swi(1 << IPI_INT);
        }
    }

    // ---- virtual interrupt controller ----

    /// Tell a virtual CPU it has an interrupt to look at. Returns false if
    /// there is no such CPU..
    fn deliver(&mut self, me: usize, target: usize, intno: u32) -> bool {
        let t = &mut self.cpus[target];
        match t.state {
            State::Off => false,
            State::Waiting => {
                t.state = State::Running;
                t.set_r0(intno);
                if target != me {
                    self.kick(target);
                }
                true
            }
            State::Running => {
                // The caller's own work is picked up on its way out.
                if target != me && t.ie() {
                    self.kick(target);
                }
                true
            }
        }
    }

    fn cpuint_post(&mut self, me: usize, target: usize, intno: u32) {
        let t = &mut self.cpus[target];
        t.ctx.cpuint_pending |= 1 << intno;
        t.ctx.vmstatus |= VMSTATUS_VMWORK;
        if t.cpuint_ready() != 0 {
            self.deliver(me, target, intno);
        }
    }

    fn shint_deliver_cpu(&mut self, me: usize, target: usize, idx: u32) -> bool {
        let (w, bit) = ((idx >> 5) as usize, 1u32 << (idx & 31));
        if self.percpu_mask[target][w] & bit == 0 {
            return false;
        }
        self.cpus[target].ctx.vmstatus |= VMSTATUS_VMWORK;
        self.deliver(me, target, idx + PERCPU_INTS)
    }

    /// Find a virtual CPU to take shared interrupt `idx`: one that is
    /// waiting if possible, otherwise the first that accepts it, starting
    /// with the caller.
    fn shint_deliver(&mut self, me: usize, idx: u32) {
        for cpu in 0..MAX_HT {
            if self.cpus[cpu].state == State::Waiting && self.shint_deliver_cpu(me, cpu, idx) {
                return;
            }
        }
        for i in 0..MAX_HT {
            if self.shint_deliver_cpu(me, (me + i) % MAX_HT, idx) {
                return;
            }
        }
    }

    /// Post shared interrupt `idx` (interrupt number - PERCPU_INTS).
    pub fn shint_post(&mut self, me: usize, idx: u32) {
        let (w, bit) = ((idx >> 5) as usize, 1u32 << (idx & 31));
        if self.pending[w] & bit != 0 {
            return;
        }
        self.pending[w] |= bit;
        if self.enable[w] & bit != 0 {
            self.shint_deliver(me, idx);
        }
    }

    /// Post an interrupt to a virtual CPU (per-CPU) or to the VM (shared).
    pub fn post(&mut self, me: usize, target: usize, intno: u32) -> i32 {
        if intno < PERCPU_INTS {
            self.cpuint_post(me, target, intno);
        } else if intno < PERCPU_INTS + SHARED_INTS {
            self.shint_post(me, intno - PERCPU_INTS);
        } else {
            return -1;
        }
        0
    }

    /// Take the best deliverable interrupt for `cpu`. Taking one disables
    /// it; the guest re-enables it when it is done.
    fn int_get(&mut self, cpu: usize) -> Option<u32> {
        let ready = self.cpus[cpu].cpuint_ready();
        if ready != 0 {
            let bit = ready.trailing_zeros();
            let c = &mut self.cpus[cpu].ctx;
            c.cpuint_pending &= !(1 << bit);
            c.cpuint_enabled &= !(1 << bit);
            return Some(bit);
        }
        for w in 0..SH_WORDS {
            let ready = self.pending[w] & self.enable[w] & self.percpu_mask[cpu][w];
            if ready != 0 {
                let bit = ready.trailing_zeros();
                self.pending[w] &= !(1 << bit);
                self.enable[w] &= !(1 << bit);
                return Some(PERCPU_INTS + w as u32 * 32 + bit);
            }
        }
        None
    }

    /// The interrupt `int_get` would take.
    fn int_peek(&self, cpu: usize) -> Option<u32> {
        let ready = self.cpus[cpu].cpuint_ready();
        if ready != 0 {
            return Some(ready.trailing_zeros());
        }
        for w in 0..SH_WORDS {
            let ready = self.pending[w] & self.enable[w] & self.percpu_mask[cpu][w];
            if ready != 0 {
                return Some(PERCPU_INTS + w as u32 * 32 + ready.trailing_zeros());
            }
        }
        None
    }

    /// With guest interrupts on, take a pending interrupt as an event; with
    /// them off, just report it..
    fn check_interrupts(&mut self, cpu: usize) -> Option<u32> {
        if self.cpus[cpu].ie() {
            let intno = self.int_get(cpu)?;
            self.cpus[cpu].vm_event(0, intno, GEVB_INTERRUPT);
            Some(intno)
        } else {
            self.int_peek(cpu)
        }
    }

    /// Look at interrupts if someone flagged work for this virtual CPU. The
    /// flag stays up while guest interrupts are off, so that it is still
    /// there when the guest turns them on or waits..
    pub fn do_work(&mut self, cpu: usize) -> Option<u32> {
        let c = &mut self.cpus[cpu];
        if !c.work() {
            return None;
        }
        if c.ie() {
            c.ctx.vmstatus &= !VMSTATUS_VMWORK;
        }
        self.check_interrupts(cpu)
    }

    /// Turn the guest's interrupts on, taking one if it was waiting.
    /// Returns whether they were on already.
    pub fn enable_ie(&mut self, cpu: usize) -> bool {
        let prev = self.cpus[cpu].ie();
        self.cpus[cpu].ctx.vmstatus |= VMSTATUS_IE;
        if !prev {
            self.check_interrupts(cpu);
        }
        prev
    }

    pub fn disable_ie(&mut self, cpu: usize) -> bool {
        let prev = self.cpus[cpu].ie();
        self.cpus[cpu].ctx.vmstatus &= !VMSTATUS_IE;
        prev
    }

    /// `vmintop`.
    pub fn intop(&mut self, me: usize, op: u32, intno: u32, arg: u32) -> i32 {
        let Some(op) = u8::try_from(op).ok().and_then(IntOp::from_raw) else {
            return -1;
        };
        match op {
            IntOp::Nop => return 0,
            IntOp::Get => return self.int_get(me).map_or(-1, |n| n as i32),
            IntOp::Peek => return self.int_peek(me).map_or(-1, |n| n as i32),
            IntOp::Post => {
                // A zero id means the caller.
                let target = if arg == 0 { Some(me) } else { self.cpu_of(arg) };
                return target.map_or(-1, |t| self.post(me, t, intno));
            }
            _ => {}
        }

        if intno < PERCPU_INTS {
            let bit = 1u16 << intno;
            let c = &mut self.cpus[me];
            match op {
                IntOp::GlobEn => c.ctx.cpuint_enabled |= bit,
                IntOp::GlobDis => c.ctx.cpuint_enabled &= !bit,
                IntOp::Clear => c.ctx.cpuint_pending &= !bit,
                IntOp::Status => {
                    let pending = (c.ctx.cpuint_pending >> intno) & 1;
                    let enabled = (c.ctx.cpuint_enabled >> intno) & 1;
                    return (pending | (enabled << 2)) as i32;
                }
                // A per-CPU interrupt has no local enable or affinity.
                _ => return -1,
            }
            // Enabling may have made a pending interrupt deliverable; it is
            // taken on the way out.
            if c.cpuint_ready() != 0 {
                c.ctx.vmstatus |= VMSTATUS_VMWORK;
            }
            0
        } else if intno < PERCPU_INTS + SHARED_INTS {
            let idx = intno - PERCPU_INTS;
            let (w, bit) = ((idx >> 5) as usize, 1u32 << (idx & 31));
            match op {
                IntOp::GlobEn => {
                    if self.enable[w] & bit == 0 {
                        self.enable[w] |= bit;
                        crate::hwint::guest_enabled(intno);
                        if self.pending[w] & bit != 0 {
                            self.shint_deliver(me, idx);
                        }
                    }
                }
                IntOp::GlobDis => self.enable[w] &= !bit,
                IntOp::LocEn => {
                    if self.percpu_mask[me][w] & bit == 0 {
                        self.percpu_mask[me][w] |= bit;
                        if self.enable[w] & self.pending[w] & bit != 0 {
                            self.shint_deliver_cpu(me, me, idx);
                        }
                    }
                }
                IntOp::LocDis => self.percpu_mask[me][w] &= !bit,
                IntOp::Affinity => {
                    let target = arg as usize;
                    if target >= MAX_HT {
                        return -1;
                    }
                    for cpu in 0..MAX_HT {
                        self.percpu_mask[cpu][w] &= !bit;
                    }
                    self.percpu_mask[target][w] |= bit;
                    if self.enable[w] & self.pending[w] & bit != 0 {
                        self.shint_deliver_cpu(me, target, idx);
                    }
                }
                IntOp::Clear => {
                    self.pending[w] &= !bit;
                    if clear_rearms_physical(intno, self.enable[w] & bit != 0) {
                        crate::hwint::guest_enabled(intno);
                    }
                }
                IntOp::Status => {
                    let pending = (self.pending[w] >> (idx & 31)) & 1;
                    let local = (self.percpu_mask[me][w] >> (idx & 31)) & 1;
                    let enabled = (self.enable[w] >> (idx & 31)) & 1;
                    return (pending | (local << 1) | (enabled << 2)) as i32;
                }
                _ => return -1,
            }
            0
        } else {
            -1
        }
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
        c.state = State::Running;
        c.timeout = 0;
        // Shared interrupts start locally disabled on a new vCPU.
        self.percpu_mask[cpu] = [0; SH_WORDS];
        self.num_cpus += 1;
    }

    /// Lowest hardware thread that is up and has no virtual CPU.
    pub fn free_cpu(&self) -> Option<usize> {
        (0..MAX_HT).find(|&i| self.cpus[i].started && self.cpus[i].state == State::Off)
    }

    /// Start bringing the machine down: every hardware thread stops as soon
    /// as it next leaves the monitor.
    pub fn begin_shutdown(&mut self, me: usize, code: u32) {
        self.shutdown = true;
        self.exit_code = code;
        for cpu in 0..MAX_HT {
            if cpu != me {
                self.kick(cpu);
            }
        }
    }

    /// Wake a hardware thread so that it notices new state (a virtual CPU it
    /// was just given).
    pub fn wake(&mut self, cpu: usize) {
        self.kick(cpu);
    }
}
