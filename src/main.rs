/*
 * Copyright (c) Qualcomm Technologies, Inc. and/or its subsidiaries.
 * SPDX-License-Identifier: BSD-3-Clause-Clear
 */

//! minivm: Hexagon VM entry point.
//!
//! The assembly entry point ([`src/entry.S`]) sets up the stack and GP,
//! then jumps to `minivm_main` which initializes all subsystems.
//! Context save/restore lives in [`src/context.S`].

#![no_std]
#![no_main]
#![cfg_attr(target_arch = "hexagon", feature(asm_experimental_arch))]
#![allow(static_mut_refs)]

mod debug;
#[cfg(target_arch = "hexagon")]
mod hexagon_abi;
mod panic;
mod semihosting;
#[cfg(feature = "run-tests")]
mod tests;

// Entry point: exception vector table + startup code (src/entry.S)
#[cfg(target_arch = "hexagon")]
core::arch::global_asm!(include_str!("entry.S"));

// Context save/restore, thread switch, TLB insert (src/context.S)
#[cfg(target_arch = "hexagon")]
core::arch::global_asm!(include_str!("context.S"));

/// Alternative stack for context switch test target.
#[cfg(all(target_arch = "hexagon", feature = "run-tests"))]
#[repr(C, align(8))]
pub(crate) struct AlignedStack(pub [u8; 4096]);

#[cfg(all(target_arch = "hexagon", feature = "run-tests"))]
pub(crate) static mut ALT_STACK: AlignedStack = AlignedStack([0u8; 4096]);

/// Saved context for round-trip context switch test.
#[cfg(target_arch = "hexagon")]
pub(crate) static mut CTX_OLD: minivm_sched::context::ThreadContext =
    minivm_sched::context::ThreadContext::zeroed();

/// Monitor-mode context for full-save trap1 handler.
/// SGP0 must point to this before using trap1 from monitor mode.
#[cfg(target_arch = "hexagon")]
pub(crate) static mut MONITOR_CTX: minivm_sched::context::ThreadContext =
    minivm_sched::context::ThreadContext::zeroed();

/// Result from guest test (set by guest handler on "return" trap).
#[cfg(target_arch = "hexagon")]
pub(crate) static mut GUEST_VERSION_RESULT: u32 = 0;

use minivm_init::boot;
use minivm_init::globals::{KernelGlobals, MAX_HTHREADS, MAX_INTERRUPTS};
use minivm_intc::percpu::CpuIntState;
use minivm_intc::shared::SharedIntState;
use minivm_mem::asid::AsidTable;
use minivm_sched::context::ThreadContext;
use minivm_sched::lowprio::LowPrio;
use minivm_sched::readylist::ReadyList;
use minivm_sched::runlist::RunList;
use minivm_thread::futex::FutexTable;
use minivm_timer::timer::TimerState;
#[cfg(target_arch = "hexagon")]
use minivm_types::asid::AsidEntry;
#[cfg(target_arch = "hexagon")]
use minivm_types::translate::Translation;
use minivm_types::trap::VmTrap;
#[cfg(target_arch = "hexagon")]
use minivm_types::vm::VM_VERSION;
use minivm_vm::vmblock::VmBlock;
use minivm_vm::vmconfig;
use minivm_vm::vmfuncs;
use minivm_vm::vmtrap;

#[cfg(target_arch = "hexagon")]
use minivm_intc::intop::{intop_dispatch, IntOpResult};
#[cfg(target_arch = "hexagon")]
use minivm_types::info::InfoType;
#[cfg(target_arch = "hexagon")]
use minivm_types::regs::Ssr;
#[cfg(target_arch = "hexagon")]
use minivm_types::timer::TimerOp;
#[cfg(target_arch = "hexagon")]
use minivm_types::vm::{VmId, GSSR_IE, GSSR_UM, INTERRUPT_GEVB_OFFSET};
#[cfg(target_arch = "hexagon")]
use minivm_types::vmint::IntOp;
#[cfg(target_arch = "hexagon")]
use minivm_vm::vmevent;

// ---- FDT parsing ----

/// Information extracted from a Flattened Device Tree.
struct FdtInfo {
    /// Whether a valid FDT was found.
    valid: bool,
    /// Timer interrupt number (0 if not found).
    timer_irq: u32,
    /// Memory size in bytes (0 if not found).
    memory_size: u64,
}

impl FdtInfo {
    const fn empty() -> Self {
        Self {
            valid: false,
            timer_irq: 0,
            memory_size: 0,
        }
    }
}

/// Try to parse an FDT at the given physical address.
///
/// Returns `FdtInfo::empty()` if the address is zero or the FDT is invalid.
fn parse_fdt(fdt_phys: u64) -> FdtInfo {
    if fdt_phys == 0 {
        return FdtInfo::empty();
    }
    // Safety: We trust the bootloader/QEMU to pass a valid address.
    // If the address is garbage, fdt::Fdt::new will fail on the magic check.
    let slice = unsafe { core::slice::from_raw_parts(fdt_phys as *const u8, MAX_FDT_SIZE) };
    let fdt = match fdt::Fdt::new(slice) {
        Ok(fdt) => fdt,
        Err(_) => return FdtInfo::empty(),
    };
    let mut info = FdtInfo {
        valid: true,
        timer_irq: 0,
        memory_size: 0,
    };

    // Extract timer interrupt number from /soc/timer node
    if let Some(timer) = fdt.find_compatible(&["qcom,qtimer"]) {
        if let Some(interrupts) = timer.property("interrupts") {
            if interrupts.value.len() >= 8 {
                // interrupts property: each entry is <irq_num flags>
                // First cell is the interrupt number (big-endian u32)
                let irq = u32::from_be_bytes([
                    interrupts.value[0],
                    interrupts.value[1],
                    interrupts.value[2],
                    interrupts.value[3],
                ]);
                info.timer_irq = irq;
            }
        }
    }

    // Extract memory size from /memory node (use first region)
    if let Some(region) = fdt.memory().regions().next() {
        info.memory_size = region.size.unwrap_or(0) as u64;
    }

    info
}

#[cfg(target_arch = "hexagon")]
mod ctx_ops;
#[cfg(target_arch = "hexagon")]
use ctx_ops::*;

/// Apply a VM event result to the thread context.
///
/// Handles user→supervisor mode transition when the event was delivered
/// from user mode: saves user r29 to GOSP, restores kernel stack from
/// old GOSP, and clears SSR.UM.
#[cfg(target_arch = "hexagon")]
fn apply_vm_event(ctx: &mut ThreadContext, ev: vmevent::VmEventResult) {
    if ev.was_user_mode {
        let old_gosp = ctx.gosp;
        ctx.gosp = ev.gosp; // Save user r29 → GOSP
        set_r29(ctx, old_gosp); // Set r29 = old GOSP (kernel stack)
        ctx.ssr &= !(1 << Ssr::UM_BIT); // Clear UM for supervisor mode
    }
    ctx.gelr = ev.gelr;
    ctx.gssr = ev.gssr;
    ctx.gbadva = ev.gbadva;
    ctx.elr = ev.new_elr;
    set_ie(ctx, false);
}

/// Cached guest page table permissions — one byte per 1MB region (4096 entries
/// cover the full 4GB address space). Populated during `vmnewmap` by walking the
/// guest PT while its pages are still in the TLB. Looked up during TLB miss
/// handling without any guest memory reads, avoiding double exceptions.
#[cfg(target_arch = "hexagon")]
static mut GUEST_PERMS: [u8; 4096] = [0xF; 4096];

/// Extract xwru permission bits from a Hexagon VM PTE.
#[cfg(target_arch = "hexagon")]
fn pte_to_xwru(pte: u32) -> u8 {
    let u = (pte >> 5) & 1;
    let r = (pte >> 9) & 1;
    let w = (pte >> 10) & 1;
    let x = (pte >> 11) & 1;
    (u | (r << 1) | (w << 2) | (x << 3)) as u8
}

/// Walk the guest page table and cache permissions in `GUEST_PERMS`.
///
/// Must be called BEFORE flushing the TLB, while the guest's PT pages are
/// still mapped (from the guest's recent writes). Runs during `vmnewmap`
/// trap handling (SSR.EX=1), but the PT pages are in the TLB from the
/// guest's preceding stores, so `read_volatile` succeeds without causing
/// a nested TLB miss.
#[cfg(target_arch = "hexagon")]
unsafe fn cache_guest_pt_perms(pt_base: u32) {
    // Default: full permissions (passthrough for unmapped regions)
    GUEST_PERMS.fill(0xF);
    // Skip if no PT or if base address is not 4K-aligned (invalid/test PT)
    if pt_base == 0 || (pt_base & 0xFFF) != 0 {
        return;
    }
    // Walk all 1024 L1 entries (each covers 4MB = 4 × 1MB sub-regions)
    for l1_idx in 0..1024usize {
        let l1_pte_addr = pt_base.wrapping_add((l1_idx as u32) * 4);
        let l1_pte = core::ptr::read_volatile(l1_pte_addr as *const u32);
        let pgsize = l1_pte & 0x7;
        if pgsize == 7 {
            // Invalid → no permissions for all 4 sub-regions
            for sub in 0..4 {
                GUEST_PERMS[l1_idx * 4 + sub] = 0;
            }
            continue;
        }
        if pgsize >= 5 {
            // 4MB+ direct mapping → same perms for all 4 sub-regions
            let xwru = pte_to_xwru(l1_pte);
            for sub in 0..4 {
                GUEST_PERMS[l1_idx * 4 + sub] = xwru;
            }
        } else {
            // L1 points to L2 table → read 4 × 1MB entries
            let l2_base = l1_pte & 0xFFFF_FFF0;
            for l2_idx in 0..4usize {
                let l2_pte_addr = l2_base.wrapping_add((l2_idx as u32) * 4);
                let l2_pte = core::ptr::read_volatile(l2_pte_addr as *const u32);
                let l2_pgsize = l2_pte & 0x7;
                if l2_pgsize == 7 {
                    GUEST_PERMS[l1_idx * 4 + l2_idx] = 0;
                } else {
                    GUEST_PERMS[l1_idx * 4 + l2_idx] = pte_to_xwru(l2_pte);
                }
            }
        }
    }
}

/// Look up cached guest page table permissions for a virtual address.
///
/// Returns 0 if the page has no permissions, 0xF if full permissions (or no
/// guest PT active). Uses the pre-cached `GUEST_PERMS` array populated during
/// `vmnewmap`, so no guest memory reads are needed during TLB miss handling.
#[cfg(target_arch = "hexagon")]
fn guest_pt_lookup(va: u32) -> u8 {
    if unsafe { GUEST_PT_BASE } == 0 {
        return 0xF; // No guest PT → full permissions
    }
    // Index by 1MB region: va >> 20
    unsafe { GUEST_PERMS[(va >> 20) as usize] }
}

/// Static pointer to KernelGlobals for handler access from assembly-called functions.
#[cfg(target_arch = "hexagon")]
pub(crate) static mut KG_PTR: *const KernelGlobals = core::ptr::null();

/// Per-CPU interrupt state for the boot VM's vCPU 0.
#[cfg(target_arch = "hexagon")]
pub(crate) static mut BOOT_CPUINT: CpuIntState = CpuIntState::new();

/// Global ASID table pointer for TLB miss handler.
#[cfg(target_arch = "hexagon")]
pub(crate) static mut ASID_TABLE_PTR: *const AsidTable = core::ptr::null();

/// Global TLB index counter for round-robin TLB slot allocation.
#[cfg(target_arch = "hexagon")]
static mut TLB_IDX: u32 = 0;

/// Global mutable ASID table pointer (for CONFIG/VMOP handlers that need to allocate ASIDs).
#[cfg(target_arch = "hexagon")]
pub(crate) static mut ASID_TABLE_MUT_PTR: *mut AsidTable = core::ptr::null_mut();

/// VM block storage — up to MAX_VMS VmBlocks.
/// Index 0 is the boot VM. Indices 1+ are child VMs created via CONFIG trap.
#[cfg(target_arch = "hexagon")]
pub(crate) static mut VMBLOCKS: [Option<VmBlock>; minivm_types::vm::MAX_VMS as usize] =
    [const { None }; minivm_types::vm::MAX_VMS as usize];

/// Flag set by boot_guest after trap0(#28) returns, to verify control flow.
#[cfg(target_arch = "hexagon")]
static mut BOOT_GUEST_RETURNED: u32 = 0;

/// Guest context for VMOP_BOOT trap0 path.
/// The VMOP_BOOT handler fills this and sets SGP0 to point here,
/// causing context_restore_rte to enter the guest.
#[cfg(target_arch = "hexagon")]
pub(crate) static mut VMOP_BOOT_GUEST_CTX: ThreadContext = ThreadContext::zeroed();

/// Saved caller SGP0 for VMOP_BOOT trap0 path.
/// When non-zero, the Stop handler restores this instead of CTX_OLD.
#[cfg(target_arch = "hexagon")]
pub(crate) static mut VMOP_BOOT_CALLER_SGP0: u32 = 0;

/// Whether QTIMER hardware is available and initialized.
/// Set to true after qtimer_init() during boot_guest().
#[cfg(target_arch = "hexagon")]
static mut QTIMER_ACTIVE: bool = false;

/// Software virtual tick counter, used when upcycle registers return 0 (QEMU).
/// Incremented by TICKS_PER_TRAP at each trap event to provide virtual time.
#[cfg(target_arch = "hexagon")]
static mut VIRTUAL_TICKS: u64 = 0;
/// Whether to use VIRTUAL_TICKS instead of upcycle (auto-detected).
#[cfg(target_arch = "hexagon")]
static mut USE_VIRTUAL_TICKS: bool = false;

/// TLB miss counter for periodic timer injection.
#[cfg(target_arch = "hexagon")]
static mut TLB_MISS_COUNT: u32 = 0;

/// Error counter for limiting guest error delivery.
#[cfg(target_arch = "hexagon")]
static mut ERROR_COUNT: u32 = 0;

/// Guest page table base address (set by vmnewmap).
/// When non-zero, TLB miss handler walks the guest page table
/// to apply permission restrictions before inserting TLB entries.
#[cfg(target_arch = "hexagon")]
static mut GUEST_PT_BASE: u32 = 0;

/// Child vCPU context for vmstart/vmstop (single child supported).
#[cfg(target_arch = "hexagon")]
static mut CHILD_CTX: ThreadContext = ThreadContext::zeroed();

/// Parent context SGP0 when child vCPU is running.
/// Non-zero means a child is currently active.
#[cfg(target_arch = "hexagon")]
static mut PARENT_CTX_PTR: u32 = 0;

/// Ticks to advance per trap event when using software counter.
/// 19.2 MHz / ~1000 traps-per-jiffy = 192 ticks/trap gives ~10ms jiffies.
#[cfg(target_arch = "hexagon")]
const TICKS_PER_TRAP: u64 = 192;

/// Print a hex value (safe to call from non-exception context only).
#[cfg(target_arch = "hexagon")]
fn print_hex(prefix: &[u8], val: u32) {
    let hex = b"0123456789abcdef";
    let mut buf = [0u8; 48];
    let plen = prefix.len().min(36);
    buf[..plen].copy_from_slice(&prefix[..plen]);
    buf[plen] = b'0';
    buf[plen + 1] = b'x';
    for i in 0..8 {
        buf[plen + 9 - i] = hex[((val >> (i * 4)) & 0xF) as usize];
    }
    buf[plen + 10] = b'\n';
    buf[plen + 11] = 0;
    debug::write0(&buf);
}

/// Kernel translate context — connects minivm-mem's translation to kernel state.
#[cfg(target_arch = "hexagon")]
struct KernelTranslateCtx<'a> {
    vm: &'a VmBlock,
    kg: &'a KernelGlobals,
}

#[cfg(target_arch = "hexagon")]
impl<'a> minivm_mem::translate::TranslateCtx for KernelTranslateCtx<'a> {
    fn translate(&self, input: Translation, info: AsidEntry) -> Translation {
        minivm_mem::translate::translate(self, input, info)
    }
    fn vmblock_guestmap(&self, _vmidx: u8) -> AsidEntry {
        AsidEntry::EMPTY // No nested guests
    }
    fn vmblock_fence_lo(&self, _vmidx: u8) -> u32 {
        self.vm.fence_lo as u32
    }
    fn vmblock_fence_hi(&self, _vmidx: u8) -> u32 {
        self.vm.fence_hi as u32
    }
    fn tcm_range(&self) -> (u32, u32) {
        let base = self.kg.tcm_base >> 12;
        let size = self.kg.tcm_size >> 12;
        (base, size)
    }
    fn vtcm_range(&self) -> (u32, u32) {
        let base = self.kg.vtcm_base >> 12;
        let size = self.kg.vtcm_size >> 12;
        (base, size)
    }
    fn physread_dword(&self, _pa: u64) -> u64 {
        0 // Not needed for offset translation
    }
}

/// Trap1 handler called from assembly.
///
/// `trap_arg` is the value of r0 when trap1 was issued.
/// Bits [12:8] contain the VM trap number.
/// Returns a value placed in r0 before returning to caller.
#[no_mangle]
pub extern "C" fn minivm_trap1_handler(trap_arg: u32) -> u32 {
    let trap_num = ((trap_arg >> 8) & 0x1F) as u8;
    match vmtrap::dispatch(trap_num) {
        vmtrap::TrapAction::Handle(trap) => match trap {
            VmTrap::Version => vmfuncs::vmtrap_version(),
            VmTrap::GetIe => vmfuncs::vmtrap_getie(false),
            VmTrap::SetIe => {
                let enable = (trap_arg & 1) != 0;
                let result = vmfuncs::vmtrap_setie(enable, false);
                result.previous_ie
            }
            // For traps that need more context, return the trap number
            // as acknowledgment that dispatch worked correctly.
            other => other as u32,
        },
        vmtrap::TrapAction::Bad => 0xFFFF_FFFF,
    }
}

/// Guest trap1 handler called from assembly (full context save path).
///
/// `ctx` points to the saved ThreadContext of the guest.
/// `trap_num` is the trap number extracted from SSR.CAUSE bits [4:0].
/// This matches the C kernel convention: guest does `trap1(#TRAPNUM)`.
///
/// The handler modifies the context as needed. For "stop" traps,
/// it sets SGP0 to the minivm_main context (CTX_OLD) to switch back.
///
/// IE state is tracked in `ctx.vmstatus` bit 7 (VMSTATUS_IE = 0x80).
#[cfg(target_arch = "hexagon")]
#[no_mangle]
#[allow(clippy::not_unsafe_ptr_arg_deref)]
pub extern "C" fn minivm_guest_trap1_handler(ctx_ptr: *mut ThreadContext, trap_num: u32) {
    let ctx = unsafe { &mut *ctx_ptr };
    // Advance virtual clock and compute timer expiry.
    // Timer injection is deferred to AFTER the dispatch to avoid corrupting
    // GSSR/GELR during Return traps.
    let mut fire_timer = false;
    #[cfg(target_arch = "hexagon")]
    unsafe {
        if QTIMER_ACTIVE {
            ctx.totalcycles = qtimer_read_count();
        } else if USE_VIRTUAL_TICKS {
            VIRTUAL_TICKS += TICKS_PER_TRAP;
            ctx.totalcycles = VIRTUAL_TICKS;
        } else {
            let pcycle_lo: u32;
            let pcycle_hi: u32;
            core::arch::asm!("{lo} = upcyclelo", lo = out(reg) pcycle_lo);
            core::arch::asm!("{hi} = upcyclehi", hi = out(reg) pcycle_hi);
            let upcycles = ((pcycle_hi as u64) << 32) | (pcycle_lo as u64);
            if upcycles == 0 {
                USE_VIRTUAL_TICKS = true;
                VIRTUAL_TICKS += TICKS_PER_TRAP;
                ctx.totalcycles = VIRTUAL_TICKS;
            } else {
                ctx.totalcycles = upcycles;
            }
        }
        // Check if timer expired — do NOT clear tree_timeout yet.
        if ctx.tree_timeout > 0 && ctx.totalcycles >= ctx.tree_timeout {
            fire_timer = true;
        }
    }
    #[cfg(not(target_arch = "hexagon"))]
    {
        ctx.totalcycles = ctx.totalcycles.wrapping_add(1000);
    }

    match vmtrap::dispatch(trap_num as u8) {
        vmtrap::TrapAction::Handle(trap) => match trap {
            VmTrap::Version => {
                set_r0(ctx, VM_VERSION);
            }
            VmTrap::Return => {
                // Return from VM event: restore ELR from GELR,
                // handle user/guest mode transition, restore IE.
                // Note: fire_timer is NOT cleared here — deliver_timer_if_ready
                // checks tree_timeout > 0, so after the first delivery clears
                // tree_timeout, re-injection is naturally prevented.
                unsafe {
                    ERROR_COUNT = 0;
                }
                let gssr = ctx.gssr;
                ctx.elr = ctx.gelr;
                // If was in user mode, swap r29 and GOSP
                if gssr & GSSR_UM != 0 {
                    let r29 = ctx_r29(ctx);
                    set_r29(ctx, ctx.gosp);
                    ctx.gosp = r29;
                    ctx.ssr |= 1 << Ssr::UM_BIT; // Enter user mode
                } else {
                    ctx.ssr &= !(1 << Ssr::UM_BIT); // Back to supervisor
                }
                // Restore IE from GSSR
                if gssr & GSSR_IE != 0 {
                    set_ie(ctx, true);
                    // Check for pending interrupts after enabling
                    try_deliver_interrupt(ctx);
                }
            }
            VmTrap::SetVec => {
                // Set guest exception vector base
                ctx.gevb = ctx_r0(ctx);
                set_r0(ctx, 0); // success
            }
            VmTrap::SetIe => {
                let enable = (ctx_r0(ctx) & 1) != 0;
                let prev = ie_enabled(ctx);
                let result = vmfuncs::vmtrap_setie(enable, prev);
                set_ie(ctx, result.now_enabled);
                set_r0(ctx, result.previous_ie);
                // If enabling, check for pending interrupts AND expired timers
                if result.now_enabled && !prev {
                    try_deliver_interrupt(ctx);
                    // Check for expired timer that was deferred due to IE being off
                    if ctx.tree_timeout > 0 && ctx.totalcycles >= ctx.tree_timeout && ctx.gevb != 0
                    {
                        ctx.tree_timeout = 0;
                        let timer_int = unsafe { (*KG_PTR).timer_intnum };
                        deliver_interrupt_event(ctx, timer_int);
                    }
                }
            }
            VmTrap::GetIe => {
                set_r0(ctx, vmfuncs::vmtrap_getie(ie_enabled(ctx)));
            }
            VmTrap::IntOp => {
                guest_handle_intop(ctx);
            }
            VmTrap::ClrMap => {
                // TLB invalidation — no-op on simulator
                set_r0(ctx, 0);
            }
            VmTrap::NewMap => {
                // Store guest page table base and cache permissions.
                // Cache BEFORE flushing TLB — the guest just wrote the PT
                // entries, so those pages are in the TLB right now.
                let pt_base = ctx_r0(ctx);
                unsafe {
                    GUEST_PT_BASE = pt_base;
                    cache_guest_pt_perms(pt_base);
                    // Flush TLB to clear stale permission entries
                    extern "C" {
                        fn minivm_tlb_insert(entry: u64, index: u32);
                    }
                    for i in 0..128u32 {
                        minivm_tlb_insert(0, i);
                    }
                    TLB_IDX = 0;
                }
                set_r0(ctx, 0);
            }
            VmTrap::CacheCtl => {
                // Cache control — no-op on simulator
                set_r0(ctx, 0);
            }
            VmTrap::GetPcycles => {
                // Advance timer so caller sees time progression since last set
                fire_timer |= advance_virtual_timer(ctx);
                // Return accumulated cycles in r1:r0, pktcount in r3:r2
                ctx.r0100 = ctx.totalcycles;
                ctx.r0302 = ctx.pktcount;
            }
            VmTrap::SetPcycles => {
                // Set pcycles/pktcount from r1:0 and r3:2
                ctx.totalcycles = ctx.r0100;
                ctx.pktcount = ctx.r0302;
                #[cfg(target_arch = "hexagon")]
                unsafe {
                    VIRTUAL_TICKS = ctx.r0100;
                }
            }
            VmTrap::Wait => {
                // Wait for interrupt — advance virtual time first
                fire_timer |= advance_virtual_timer(ctx);
                guest_handle_wait(ctx);
            }
            VmTrap::Yield => {
                // Yield — advance virtual time and check timer
                fire_timer |= advance_virtual_timer(ctx);
                set_r0(ctx, 0);
            }
            VmTrap::Start => {
                // Start new vCPU — r0=entry, r1=stack pointer
                let entry = ctx_r0(ctx);
                let sp = ctx_r1(ctx);
                unsafe {
                    let child = &mut *core::ptr::addr_of_mut!(CHILD_CTX);
                    *child = ThreadContext::zeroed();
                    child.elr = entry;
                    child.r2928 = (sp as u64) << 32; // r29=sp
                    child.ssr = ctx.ssr;
                    child.ccr = ctx.ccr;
                    child.usrp30 = ctx.usrp30;
                    child.gevb = ctx.gevb;
                    child.vmblock = ctx.vmblock;
                    child.id = VmId::new(ctx.id.vmidx(), 1, 0);
                    child.trapmask = ctx.trapmask;
                    child.tlbidxmask = ctx.tlbidxmask;
                }
                // Return child's VmId as VPID
                set_r0(ctx, VmId::new(ctx.id.vmidx(), 1, 0).0);
            }
            VmTrap::Stop => {
                // Clear guest PT to prevent stale lookups in subsequent boots
                unsafe {
                    GUEST_PT_BASE = 0;
                }
                // Stop guest thread — store exit code and switch back.
                // Supports three paths:
                //   Child vCPU: switch back to parent context
                //   VMOP_BOOT: restore SGP0 to caller context (VMOP_BOOT_CALLER_SGP0)
                //   vmboot():  restore SGP0 to CTX_OLD (direct minivm_switch path)
                let exit_code = ctx_r0(ctx);
                unsafe {
                    // Check if we're a child vCPU
                    if PARENT_CTX_PTR != 0 {
                        CHILD_CTX.elr = 0; // Mark child as done
                        core::arch::asm!("sgp0 = {val}", val = in(reg) PARENT_CTX_PTR);
                        PARENT_CTX_PTR = 0;
                    } else {
                        let caller_sgp0 = VMOP_BOOT_CALLER_SGP0;
                        if caller_sgp0 != 0 {
                            // VMOP_BOOT path: store exit code in caller's r0100
                            // NOTE: No semihosting here — on QEMU, trap0(#0) fires
                            // EVB exception which corrupts the context via crswap.
                            let caller_ctx = &mut *(caller_sgp0 as *mut ThreadContext);
                            set_r0(caller_ctx, exit_code);
                            core::arch::asm!("sgp0 = {val}", val = in(reg) caller_sgp0);
                            VMOP_BOOT_CALLER_SGP0 = 0;
                            // Decrement VM CPU count
                            if ctx.vmblock != 0 {
                                let vm = &mut *(ctx.vmblock as *mut VmBlock);
                                vm.num_cpus -= 1;
                            }
                        } else {
                            // vmboot() path: use CTX_OLD
                            GUEST_VERSION_RESULT = exit_code;
                            let old_ptr = core::ptr::addr_of_mut!(CTX_OLD) as u32;
                            core::arch::asm!("sgp0 = {val}", val = in(reg) old_ptr);
                        }
                    }
                }
            }
            VmTrap::VmPid => {
                // Return vCPU ID
                set_r0(ctx, ctx.id.0);
            }
            VmTrap::SetRegs => {
                // Set guest registers from r0-r3
                ctx.gelr = ctx_r0(ctx);
                ctx.gssr = ctx_r1(ctx);
                ctx.gosp = ctx_r2(ctx);
                ctx.gbadva = ctx_r3(ctx);
            }
            VmTrap::GetRegs => {
                // Return guest registers: r0=gelr, r1=gssr, r2=gosp, r3=gbadva
                ctx.r0100 = ((ctx.gssr as u64) << 32) | ctx.gelr as u64;
                ctx.r0302 = ((ctx.gbadva as u64) << 32) | ctx.gosp as u64;
            }
            VmTrap::TimerOp => {
                guest_handle_timer(ctx);
            }
            VmTrap::PmuCtrl => {
                // PMU control — no-op on simulator
                set_r0(ctx, 0);
            }
            VmTrap::Info => {
                guest_handle_info(ctx);
            }
        },
        vmtrap::TrapAction::Bad => {
            // Bad trap — deliver error event or return error
            if ctx.gevb != 0 {
                // Deliver error event to guest via GEVB
                let event = vmevent::compute_vm_event(
                    0,                                   // gbadva
                    vmtrap::CAUSE_NO_GUEST_PERM,         // cause
                    minivm_types::vm::ERROR_GEVB_OFFSET, // vec_offset
                    ctx.gevb,
                    ctx.elr,
                    ctx_r29(ctx),
                    !Ssr::new(ctx.ssr).um(), // user mode awareness
                    ie_enabled(ctx),
                    false, // no single-step
                );
                if let Some(ev) = event {
                    apply_vm_event(ctx, ev);
                }
            } else {
                set_r0(ctx, 0xFFFF_FFFF);
            }
        }
    }

    // Inject timer AFTER the dispatch (Return clears fire_timer to avoid loops).
    #[cfg(target_arch = "hexagon")]
    deliver_timer_if_ready(ctx, fire_timer);
}

/// Handle intop trap (trap1 #5) — interrupt operations.
///
/// r0 = operation type (IntOp), r1 = interrupt number, r2 = extra param.
#[cfg(target_arch = "hexagon")]
fn guest_handle_intop(ctx: &mut ThreadContext) {
    let op_raw = ctx_r0(ctx) as u8;
    let intno = ctx_r1(ctx);
    let _extra = ctx_r2(ctx);

    let Some(op) = IntOp::from_raw(op_raw) else {
        set_r0(ctx, 0xFFFF_FFFF); // -1 = invalid op
        return;
    };

    // Get VM block from context
    if ctx.vmblock == 0 {
        set_r0(ctx, 0xFFFF_FFFF);
        return;
    }
    let vm = unsafe { &mut *(ctx.vmblock as *mut VmBlock) };
    let cpuidx = ctx.id.cpuidx() as u32;
    let cpuint = &raw mut BOOT_CPUINT;

    let result = intop_dispatch(unsafe { &mut *cpuint }, &mut vm.shint, op, intno, cpuidx);
    match result {
        IntOpResult::Ok(val) => set_r0(ctx, val as u32),
        IntOpResult::Fail => set_r0(ctx, 0xFFFF_FFFF),
        IntOpResult::Deliver(_cpu) => {
            set_r0(ctx, 0);
            // Only deliver immediately for LocEn — the explicit "enable and
            // deliver" operation. POST/GlobEn mark state but defer delivery
            // to the next check point (SetIe, Return, Wait).
            if op == IntOp::LocEn && ie_enabled(ctx) && ctx.gevb != 0 {
                let cpuint_ref = unsafe { &mut *cpuint };
                let pc_int = cpuint_ref.get();
                if pc_int >= 0 {
                    deliver_interrupt_event(ctx, pc_int as u32);
                    return;
                }
                let sh_int = vm
                    .shint
                    .get(cpuidx, minivm_types::consts::PERCPU_INTERRUPTS);
                if sh_int >= 0 {
                    deliver_interrupt_event(ctx, sh_int as u32);
                }
            }
        }
    }
}

/// Handle wait trap (trap1 #16) — wait for interrupt.
///
/// Checks for pending deliverable interrupts. If found, delivers via
/// VM event and returns. If none, on a real kernel we'd deschedule
/// the thread; on single-thread sim we return -1.
#[cfg(target_arch = "hexagon")]
fn guest_handle_wait(ctx: &mut ThreadContext) {
    // Check for ready child vCPU
    unsafe {
        let child = &*core::ptr::addr_of!(CHILD_CTX);
        if child.elr != 0 && PARENT_CTX_PTR == 0 {
            // Save parent's return value
            set_r0(ctx, 0);
            // Save parent SGP0
            let parent_sgp0: u32;
            core::arch::asm!("{val} = sgp0", val = out(reg) parent_sgp0);
            PARENT_CTX_PTR = parent_sgp0;
            // Switch to child
            let child_ptr = core::ptr::addr_of_mut!(CHILD_CTX) as u32;
            core::arch::asm!("sgp0 = {val}", val = in(reg) child_ptr);
            return;
        }
    }

    // Check for pending interrupts
    if ctx.vmblock != 0 && ie_enabled(ctx) {
        let vm = unsafe { &mut *(ctx.vmblock as *mut VmBlock) };
        let cpuidx = ctx.id.cpuidx() as u32;
        let cpuint = unsafe { &mut BOOT_CPUINT };

        // Try per-CPU first
        let pc_int = cpuint.get();
        if pc_int >= 0 {
            deliver_interrupt_event(ctx, pc_int as u32);
            return;
        }
        // Then shared
        let sh_int = vm
            .shint
            .get(cpuidx, minivm_types::consts::PERCPU_INTERRUPTS);
        if sh_int >= 0 {
            deliver_interrupt_event(ctx, sh_int as u32);
            return;
        }
    }
    // No interrupt pending in CpuIntState.
    // During guest boot (VMOP_BOOT active), deliver a synthetic timer interrupt
    // so the guest doesn't hang in its idle loop waiting for a timer.
    // Only deliver if the guest has actually requested a timer (tree_timeout > 0)
    // to avoid spurious interrupts before the guest's timer ISR is registered.
    unsafe {
        if VMOP_BOOT_CALLER_SGP0 != 0 && ctx.tree_timeout > 0 && ie_enabled(ctx) && ctx.gevb != 0 {
            let timer_int = (*KG_PTR).timer_intnum;
            let _ = advance_virtual_timer(ctx);
            deliver_interrupt_event(ctx, timer_int);
            return;
        }
    }
    set_r0(ctx, 0xFFFF_FFFF);
}

/// Deliver an interrupt as a VM event to the guest.
///
/// Sets up GELR, GSSR, GOSP, GBADVA and redirects ELR to GEVB + interrupt offset.
/// The interrupt number is placed in the GSSR cause field.
#[cfg(target_arch = "hexagon")]
fn deliver_interrupt_event(ctx: &mut ThreadContext, intno: u32) {
    if ctx.gevb == 0 {
        // No GEVB set — can't deliver, return interrupt number in r0
        set_r0(ctx, intno);
        return;
    }

    let event = vmevent::compute_vm_event(
        0,                     // gbadva (not relevant for interrupts)
        intno,                 // cause = interrupt number
        INTERRUPT_GEVB_OFFSET, // vector offset for interrupts
        ctx.gevb,
        ctx.elr,
        ctx_r29(ctx),
        !Ssr::new(ctx.ssr).um(), // user mode → ssr_guest=false for r29/GOSP swap
        ie_enabled(ctx),
        false, // ss_enabled
    );
    if let Some(ev) = event {
        apply_vm_event(ctx, ev);
    }
}

/// Try to deliver a pending interrupt to the guest (if IE enabled).
///
/// Called after enabling interrupts to check if any are pending.
#[cfg(target_arch = "hexagon")]
fn try_deliver_interrupt(ctx: &mut ThreadContext) {
    if !ie_enabled(ctx) || ctx.gevb == 0 {
        return;
    }
    if ctx.vmblock == 0 {
        return;
    }
    let vm = unsafe { &mut *(ctx.vmblock as *mut VmBlock) };
    let cpuidx = ctx.id.cpuidx() as u32;
    let cpuint = unsafe { &mut BOOT_CPUINT };

    // Check per-CPU interrupts (fully deliverable: pending+enabled+local)
    let pc_int = cpuint.peek_deliverable();
    if pc_int >= 0 {
        let _ = cpuint.get(); // consume it
        deliver_interrupt_event(ctx, pc_int as u32);
        return;
    }
    // Check shared interrupts
    let sh_int = vm
        .shint
        .peek(cpuidx, minivm_types::consts::PERCPU_INTERRUPTS);
    if sh_int >= 0 {
        let _ = vm
            .shint
            .get(cpuidx, minivm_types::consts::PERCPU_INTERRUPTS);
        deliver_interrupt_event(ctx, sh_int as u32);
    }
}

/// TLB miss handler called from assembly (vectors 4 and 6).
///
/// `ctx_ptr`: pointer to saved ThreadContext (from SGP0).
/// `va`: faulting virtual address (from BADVA register).
///
/// Translates the virtual address using the ASID table and the translation
/// system, formats a TLB entry, and inserts it into the hardware TLB.
/// On failure, delivers a page fault event to the guest via GEVB.
#[cfg(target_arch = "hexagon")]
#[no_mangle]
#[allow(clippy::not_unsafe_ptr_arg_deref)]
pub extern "C" fn minivm_tlb_miss_handler(ctx_ptr: *mut ThreadContext, va: u32) {
    let ctx = unsafe { &mut *ctx_ptr };
    let ssr = Ssr::new(ctx.ssr);
    let asid = ssr.asid() as u32;

    unsafe {
        TLB_MISS_COUNT += 1;
    }

    // Look up ASID table
    let asid_table = unsafe { &*ASID_TABLE_PTR };
    let info = *asid_table.get(asid);
    if info.is_empty() {
        handle_pagefault(ctx, va);
        return;
    }

    // Get VmBlock from context
    if ctx.vmblock == 0 {
        handle_pagefault(ctx, va);
        return;
    }
    let vm = unsafe { &*(ctx.vmblock as *const VmBlock) };
    let kg = unsafe { &*KG_PTR };

    // Run translation
    let translate_ctx = KernelTranslateCtx { vm, kg };
    let input = Translation::default_for_va(va);
    let mut result = minivm_mem::translate::translate(&translate_ctx, input, info);
    if result.is_bad() {
        handle_pagefault(ctx, va);
        return;
    }

    // Apply guest page table permission restrictions.
    // Check the specific access type against guest permissions and deliver
    // the correct HVM cause code for any violation. This handles both
    // missing permissions (e.g., no X on a fetch) and user-mode violations
    // (e.g., U=0 on a page accessed from user mode).
    if unsafe { GUEST_PT_BASE } != 0 {
        let guest_xwru = guest_pt_lookup(va);
        let ssr_cause = ssr.cause();
        let is_user = ssr.um();

        let violation_cause = match ssr_cause {
            // Instruction fetch
            0x60..=0x62 => {
                if guest_xwru & 0x8 == 0 {
                    Some(0x11u32) // PROT_EX: no execute permission
                } else if is_user && guest_xwru & 0x1 == 0 {
                    Some(0x14u32) // PROT_UEX: user execute, no user bit
                } else {
                    None
                }
            }
            // Data write
            0x71 => {
                if guest_xwru & 0x4 == 0 {
                    Some(0x23u32) // PROT_WR: no write permission
                } else if is_user && guest_xwru & 0x1 == 0 {
                    Some(0x25u32) // PROT_UWR: user write, no user bit
                } else {
                    None
                }
            }
            // Data read (0x70 or other)
            _ => {
                if guest_xwru & 0x2 == 0 {
                    Some(0x22u32) // PROT_RD: no read permission
                } else if is_user && guest_xwru & 0x1 == 0 {
                    Some(0x24u32) // PROT_URD: user read, no user bit
                } else {
                    None
                }
            }
        };

        if let Some(cause) = violation_cause {
            handle_pagefault_cause(ctx, va, cause);
            return;
        }
        result = result.with_xwru(result.xwru() & guest_xwru);
    }

    // Format as TLB entry
    let entry = minivm_mem::tlb_fill::tlbfmt_from_translation(result, va, asid);
    if entry == 0 {
        handle_pagefault(ctx, va);
        return;
    }

    // Insert into hardware TLB with round-robin index
    let idx = unsafe {
        let i = TLB_IDX;
        let tlb_size = (*KG_PTR).tlb_size;
        TLB_IDX = (i + 1) % if tlb_size > 0 { tlb_size } else { 128 };
        i
    };
    extern "C" {
        fn minivm_tlb_insert(entry: u64, index: u32);
    }
    unsafe {
        minivm_tlb_insert(entry, idx);
    }

    // Timer injection: on every 256th TLB miss, check if the timer expired
    // and deliver an interrupt. This ensures timer delivery even when the guest
    // is in a tight loop with no trap1/yield/wait calls (only TLB misses).
    unsafe {
        if VMOP_BOOT_CALLER_SGP0 != 0 && (TLB_MISS_COUNT & 0xFF) == 0 && QTIMER_ACTIVE {
            ctx.totalcycles = qtimer_read_count();
            if ctx.tree_timeout > 0 && ctx.totalcycles >= ctx.tree_timeout {
                ctx.tree_timeout = 0;
                if ctx.gevb != 0 && ie_enabled(ctx) {
                    let timer_int = (*KG_PTR).timer_intnum;
                    deliver_interrupt_event(ctx, timer_int);
                }
            }
        }
    }
}

/// Handle a TLB page fault — deliver error event to guest via GEVB.
#[cfg(target_arch = "hexagon")]
fn handle_pagefault(ctx: &mut ThreadContext, va: u32) {
    handle_pagefault_cause(ctx, va, vmtrap::CAUSE_NO_GUEST_PERM);
}

/// Handle a page fault with a specific cause code.
///
/// Used by the guest page table walker to deliver permission violations
/// with the correct hardware cause code (PROT_WR, PROT_RD, etc.).
#[cfg(target_arch = "hexagon")]
fn handle_pagefault_cause(ctx: &mut ThreadContext, va: u32, cause: u32) {
    if ctx.gevb != 0 {
        let event = vmevent::compute_vm_event(
            va, // gbadva = faulting VA
            cause,
            minivm_types::vm::ERROR_GEVB_OFFSET, // error vector
            ctx.gevb,
            ctx.elr,
            ctx_r29(ctx),
            !Ssr::new(ctx.ssr).um(), // user mode → ssr_guest=false
            ie_enabled(ctx),
            false, // no single-step
        );
        if let Some(ev) = event {
            apply_vm_event(ctx, ev);
        }
    }
    // If no GEVB, we can't deliver the fault. The context_restore_rte will
    // re-execute the faulting instruction causing an infinite loop.
}

/// Error exception handler called from assembly (vector 2).
///
/// `ctx_ptr`: pointer to saved ThreadContext (from SGP0).
/// `badva`: faulting virtual address (from BADVA register).
/// `cause`: error cause code (SSR.CAUSE, lower 8 bits).
///
/// In guest mode: delivers error event to guest via GEVB.
/// In monitor mode: fatal error — hangs (should not happen in normal operation).
#[cfg(target_arch = "hexagon")]
#[no_mangle]
#[allow(clippy::not_unsafe_ptr_arg_deref)]
pub extern "C" fn minivm_error_handler(ctx_ptr: *mut ThreadContext, badva: u32, cause: u32) {
    let ctx = unsafe { &mut *ctx_ptr };
    let ssr = Ssr::new(ctx.ssr);

    // Check if this is a genuine guest error vs a monitor-mode error during
    // context restore. When minivm_context_restore_rte writes SSR with GUEST=1
    // before RTE, an error can fire with SSR.GUEST=1 but ELR still pointing
    // to hypervisor code. Detect this by checking if ELR is in hypervisor
    // address range (0xFF000000+). These errors are benign — the RTE will
    // complete fine after we return.
    let in_hypervisor = ctx.elr >= 0xFF000000;

    if in_hypervisor {
        // Benign error during context restore — don't count toward error total.
        return;
    }

    unsafe {
        ERROR_COUNT = ERROR_COUNT.saturating_add(1);
    }

    if ssr.guest() {
        // Deliver error to guest's GEVB error handler.
        // Only deliver first batch of errors to prevent recursive exception loops.
        let first_error = unsafe { ERROR_COUNT <= 100 };
        if first_error && ctx.gevb != 0 {
            let event = vmevent::compute_vm_event(
                badva,
                cause,
                minivm_types::vm::ERROR_GEVB_OFFSET,
                ctx.gevb,
                ctx.elr,
                ctx_r29(ctx),
                !ssr.um(), // user mode → ssr_guest=false for r29/GOSP swap
                ie_enabled(ctx),
                false, // no single-step
            );
            if let Some(ev) = event {
                apply_vm_event(ctx, ev);
            }
        } else {
            // Skip subsequent errors by advancing past the faulting instruction
            ctx.elr = ctx.elr.wrapping_add(4);
        }
    } else {
        // Monitor mode error — fatal. Just hang.
        // NOTE: Cannot use semihosting here — we're in exception mode
        // (SSR.EX=1) and trap0(#0) would cause a double exception.
        loop {
            unsafe {
                core::arch::asm!("nop");
            }
        }
    }
}

/// Guest trap0 handler — delivers trap0 events to the guest's GEVB.
///
/// Called for all trap0 instructions from guest mode that are not VMOP (#28)
/// or CONFIG (#30). The simulator handles trap0(#0) semihosting at the ISA
/// level (output appears before this handler runs). This handler delivers
/// the trap0 event to the guest's GEVB trap0 vector, allowing the guest OS
/// to handle system calls, semihosting acknowledgment, and exit requests.
#[cfg(target_arch = "hexagon")]
#[no_mangle]
#[allow(clippy::not_unsafe_ptr_arg_deref)]
pub extern "C" fn minivm_trap0_semihost_handler(ctx_ptr: *mut ThreadContext) {
    if ctx_ptr.is_null() {
        return;
    }
    let ctx = unsafe { &mut *ctx_ptr };
    let ssr = Ssr::new(ctx.ssr);

    // Only handle guest-mode trap0s
    if !ssr.guest() {
        return;
    }

    // Advance virtual clock so the guest sees time progression.
    // Timer delivery is NOT done here — the trap0 event below overwrites
    // ELR/GELR/GSSR, which would corrupt a timer event. Timer will fire
    // on the next trap1 via the deferred fire_timer path.
    let _ = advance_virtual_timer(ctx);

    // Deliver trap0 to guest's GEVB trap0 handler.
    // The cause code (trap0 number) is in SSR.CAUSE.
    if ctx.gevb != 0 {
        let cause = ssr.cause() as u32;
        let event = vmevent::compute_vm_event(
            0, // gbadva = 0 for trap0
            cause,
            minivm_types::vm::TRAP0_GEVB_OFFSET,
            ctx.gevb,
            ctx.elr,
            ctx_r29(ctx),
            !ssr.um(), // user mode → ssr_guest=false
            ie_enabled(ctx),
            false,
        );
        if let Some(ev) = event {
            apply_vm_event(ctx, ev);
        }
    }
}

/// trap0(#30) CONFIG handler — VM configuration operations.
///
/// Called from assembly with ctx_ptr pointing to the saved guest context.
/// Reads r0-r4 from the context:
///   r0 = config_type (must be CONFIG_VMBLOCK_INIT = 0)
///   r1 = vm index (or 0 for SET_CPUS_INTS which allocates)
///   r2 = vmblock_init op
///   r3 = arg1
///   r4 = arg2
/// Returns the VM index on success, 0 on failure.
#[cfg(target_arch = "hexagon")]
#[no_mangle]
#[allow(clippy::not_unsafe_ptr_arg_deref)]
pub extern "C" fn minivm_trap0_config_handler(ctx_ptr: *mut ThreadContext) -> u32 {
    use minivm_types::config::VmblockInitOp;

    let ctx = unsafe { &mut *ctx_ptr };
    let config_type = ctx_r0(ctx);
    let vm_idx = ctx_r1(ctx);
    let op_raw = ctx_r2(ctx);
    let arg1 = ctx_r3(ctx);
    let arg2 = ctx_r4(ctx);

    // Only CONFIG_VMBLOCK_INIT (type 0) is supported
    if config_type != 0 {
        return 0;
    }

    let Some(op) = VmblockInitOp::from_raw(op_raw as u8) else {
        return 0;
    };

    // SET_CPUS_INTS allocates a new VmBlock
    if op == VmblockInitOp::SetCpusInts {
        // Find a free VM slot (start from 1; 0 is the boot VM)
        let vmblocks = unsafe { &mut *core::ptr::addr_of_mut!(VMBLOCKS) };
        let mut free_idx = 0u32;
        for (i, slot) in vmblocks.iter().enumerate().skip(1) {
            if slot.is_none() {
                free_idx = i as u32;
                break;
            }
        }
        if free_idx == 0 {
            return 0; // no free slot
        }

        let mut new_vm = VmBlock::new(free_idx);
        // Set parent to caller's VM
        new_vm.parent = ctx.id;
        let r = vmconfig::set_cpus_ints(&mut new_vm, arg1, arg2);
        if r != vmconfig::ConfigResult::Ok {
            return 0;
        }
        vmblocks[free_idx as usize] = Some(new_vm);
        return free_idx;
    }

    // Other ops require an existing VmBlock
    let vmblocks = unsafe { &mut *core::ptr::addr_of_mut!(VMBLOCKS) };
    if vm_idx as usize >= vmblocks.len() {
        return 0;
    }
    let Some(vm) = vmblocks.get_mut(vm_idx as usize).and_then(|v| v.as_mut()) else {
        return 0;
    };

    // Permission check: caller must be the parent
    if ctx.id.vmidx() != vm.parent.vmidx() {
        return 0;
    }

    // VM must not be running
    if vm.num_cpus > 0 {
        return 0;
    }

    let result = match op {
        VmblockInitOp::SetFences => vmconfig::set_fences(vm, arg1 as i32, arg2 as i32),
        VmblockInitOp::SetPrioTrapmask => vmconfig::set_prio_trapmask(vm, arg1, arg2),
        VmblockInitOp::SetPmapType => {
            // arg1 = translation config (OffsetConfig raw value for offset mode)
            // arg2 = translation type
            use minivm_types::config::OffsetConfig;
            vmconfig::set_pmap_type(
                vm,
                AsidEntry::EMPTY,
                OffsetConfig(arg1),
                0, // tlbidxmask
            )
        }
        VmblockInitOp::MapPhysIntr => {
            use minivm_types::config::PhysintConfig;
            let cfg = PhysintConfig(arg2);
            vmconfig::map_phys_intr(vm, arg1, cfg.physint())
        }
        VmblockInitOp::SetCpusInts => unreachable!(),
    };

    match result {
        vmconfig::ConfigResult::Ok => vm_idx,
        _ => 0,
    }
}

/// trap0(#28) VMOP handler — VM operations (boot, status, free).
///
/// Called from assembly with ctx_ptr pointing to the saved guest context.
/// Reads r0-r5 from the context:
///   r0 = vmop operation (BOOT=0, STATUS=1, FREE=2)
///   For BOOT: r1=pc, r2=sp, r3=arg1, r4=prio, r5=vm
///   For STATUS: r1=status_op, r2=vm
///   For FREE: r1=vm
/// Returns operation result in r0 (-1 on failure).
#[cfg(target_arch = "hexagon")]
#[no_mangle]
#[allow(clippy::not_unsafe_ptr_arg_deref)]
pub extern "C" fn minivm_trap0_vmop_handler(ctx_ptr: *mut ThreadContext) -> u32 {
    use minivm_types::asid::TranslationType;

    let ctx = unsafe { &mut *ctx_ptr };
    let op = ctx_r0(ctx);

    match op {
        0 => {
            // VMOP_BOOT
            let pc = ctx_r1(ctx);
            let sp = ctx_r2(ctx);
            let arg1 = ctx_r3(ctx);
            let prio = ctx_r4(ctx) as u8;
            let vm_idx = ctx_r5(ctx);

            let vmblocks = unsafe { &mut *core::ptr::addr_of_mut!(VMBLOCKS) };
            if vm_idx as usize >= vmblocks.len() {
                return 0xFFFF_FFFF; // -1
            }
            let Some(vm) = vmblocks.get_mut(vm_idx as usize).and_then(|v| v.as_mut()) else {
                return 0xFFFF_FFFF;
            };

            // Permission check: caller must be parent
            if ctx.id.vmidx() != vm.parent.vmidx() {
                return 0xFFFF_FFFF;
            }

            // VM must not be running
            if vm.num_cpus > 0 {
                return 0xFFFF_FFFF;
            }

            // Allocate ASID using VM's configured phys_offset (set by SET_PMAP_TYPE)
            let asid_table = unsafe { &mut *ASID_TABLE_MUT_PTR };
            let asid = asid_table.inc(
                vm.phys_offset.0,
                TranslationType::Offset,
                false,
                0,
                vm.vmidx as u8,
                |_| {},
            );
            if asid < 0 {
                return 0xFFFF_FFFF;
            }

            // Wire up globals for TLB miss handler
            unsafe {
                ASID_TABLE_PTR = ASID_TABLE_MUT_PTR as *const AsidTable;
                BOOT_CPUINT = CpuIntState::new();
            }

            // Build guest ThreadContext
            let guest_ctx = unsafe { &mut *core::ptr::addr_of_mut!(VMOP_BOOT_GUEST_CTX) };
            *guest_ctx = ThreadContext::zeroed();
            guest_ctx.elr = pc;
            guest_ctx.r2928 = (sp as u64) << 32; // r29=sp, r28=0
                                                 // r0100 will be set by the assembly (memw(r7+#128) = r0)
                                                 // after this handler returns arg1.
            guest_ctx.ssr = Ssr::new(0)
                .with_guest(true)
                .with_asid(asid as u8)
                .with_ie(true) // Enable hardware interrupts (host IE) during guest execution
                .0;
            guest_ctx.ccr = minivm_types::regs::boot_defaults::THREAD_CCR;
            guest_ctx.usrp30 = (minivm_types::regs::boot_defaults::THREAD_USR as u64) << 32;
            guest_ctx.id = VmId::new(vm_idx as u8, 0, 0);
            guest_ctx.vmblock = vm as *mut VmBlock as u32;
            guest_ctx.prio = prio;
            guest_ctx.base_prio = prio;
            guest_ctx.trapmask = vm.trapmask;
            guest_ctx.tlbidxmask = vm.tlbidxmask;
            guest_ctx.gevb = 0;
            guest_ctx.vmstatus = 0;

            // Save caller's SGP0 (so Stop handler can restore it)
            let caller_sgp0: u32;
            unsafe {
                core::arch::asm!("{val} = sgp0", val = out(reg) caller_sgp0);
                VMOP_BOOT_CALLER_SGP0 = caller_sgp0;
            }

            // Set SGP0 to guest context — the assembly after this handler
            // will read SGP0, store our return value as guest r0, and
            // context_restore_rte to enter the guest.
            let guest_ptr = guest_ctx as *mut ThreadContext as u32;
            unsafe {
                core::arch::asm!("sgp0 = {val}", val = in(reg) guest_ptr);
            }

            vm.num_cpus += 1;

            // Return arg1 — assembly stores this as the guest's initial r0
            arg1
        }
        1 => {
            // VMOP_STATUS
            let status_op = ctx_r1(ctx);
            let vm_idx = ctx_r2(ctx);

            let vmblocks = unsafe { &*core::ptr::addr_of!(VMBLOCKS) };
            if vm_idx as usize >= vmblocks.len() {
                return 0xFFFF_FFFF;
            }
            let Some(vm) = vmblocks.get(vm_idx as usize).and_then(|v| v.as_ref()) else {
                return 0xFFFF_FFFF;
            };

            match status_op {
                0 => vm.status as u32, // STATUS_STATUS
                1 => vm.num_cpus,      // STATUS_CPUS
                _ => 0xFFFF_FFFF,
            }
        }
        2 => {
            // VMOP_FREE
            let vm_idx = ctx_r1(ctx);

            let vmblocks = unsafe { &mut *core::ptr::addr_of_mut!(VMBLOCKS) };
            if vm_idx as usize >= vmblocks.len() {
                return 0xFFFF_FFFF;
            }
            if let Some(vm) = vmblocks.get(vm_idx as usize).and_then(|v| v.as_ref()) {
                // Must not be running
                if vm.num_cpus > 0 {
                    return 0xFFFF_FFFF;
                }
            } else {
                return 0xFFFF_FFFF;
            }
            vmblocks[vm_idx as usize] = None;
            0
        }
        _ => 0xFFFF_FFFF,
    }
}

/// Handle timer trap (trap1 #24) — timer operations.
///
/// Calling convention (standard Hexagon ABI for `__vmtimerop(op, dummy, arg)`):
///   r0 = operation type (TimerOp)
///   r1 = dummy (unused)
///   r3:r2 = 64-bit argument (in nanoseconds for set/delta timeout)
///   Return: r1:r0 = 64-bit result
///
/// All time values exchanged with the guest are in NANOSECONDS.
/// Internally, tree_timeout and totalcycles are in raw ticks (pcycles).
#[cfg(target_arch = "hexagon")]
fn guest_handle_timer(ctx: &mut ThreadContext) {
    let op_raw = ctx_r0(ctx) as u8;
    let arg64 = ctx.r0302; // r3:r2 = 64-bit argument

    let Some(op) = TimerOp::from_raw(op_raw) else {
        ctx.r0100 = 0xFFFF_FFFF_FFFF_FFFF;
        return;
    };

    // Timer frequency: 19.2 MHz (standard Hexagon timer)
    const NS_PER_TICK: u64 = 52;
    // Frequency reported to guest: ticks/sec * ns/tick = ns/sec ≈ 998.4 MHz
    const NSEC_FREQ: u64 = 19_200_000 * NS_PER_TICK; // 998_400_000
                                                     // Minimum timer granularity in ticks (from C hypervisor)
    const TICK_GRANULARITY: u64 = 4;

    let result: u64 = match op {
        TimerOp::GetFreq => NSEC_FREQ,
        TimerOp::GetResolution => NS_PER_TICK,
        TimerOp::GetTime => {
            // Return current time in nanoseconds
            ctx.totalcycles.wrapping_mul(NS_PER_TICK)
        }
        TimerOp::GetTimeout => {
            // Return current timeout in nanoseconds
            ctx.tree_timeout.wrapping_mul(NS_PER_TICK)
        }
        TimerOp::SetTimeout => {
            // arg is absolute timeout in nanoseconds.
            // Convert to ticks, store, return timeout in NS (non-zero=success)
            // or 0 if already expired (kernel retries on 0).
            let timeout_tick = arg64 / NS_PER_TICK;
            if timeout_tick <= ctx.totalcycles {
                ctx.tree_timeout = 0;
                #[cfg(target_arch = "hexagon")]
                if unsafe { QTIMER_ACTIVE || ON_QEMU } {
                    qtimer_set_match(!0u64);
                }
                0 // expired — kernel will retry
            } else {
                ctx.tree_timeout = timeout_tick;
                #[cfg(target_arch = "hexagon")]
                if unsafe { QTIMER_ACTIVE || ON_QEMU } {
                    qtimer_set_match(timeout_tick);
                }
                timeout_tick.wrapping_mul(NS_PER_TICK) // success: return NS
            }
        }
        TimerOp::DeltaTimeout => {
            // arg is delta in nanoseconds. Convert to ticks, add to now.
            // Return timeout in NS (non-zero=success) or 0 if expired.
            if arg64 == u64::MAX {
                // TIME_FOREVER — disable timer
                ctx.tree_timeout = 0;
                #[cfg(target_arch = "hexagon")]
                if unsafe { QTIMER_ACTIVE || ON_QEMU } {
                    qtimer_set_match(!0u64);
                }
                0
            } else {
                let delta_ticks = (arg64 / NS_PER_TICK).max(TICK_GRANULARITY);
                let timeout_ticks = ctx.totalcycles.wrapping_add(delta_ticks);
                ctx.tree_timeout = timeout_ticks;
                #[cfg(target_arch = "hexagon")]
                if unsafe { QTIMER_ACTIVE || ON_QEMU } {
                    qtimer_set_match(timeout_ticks);
                }
                timeout_ticks.wrapping_mul(NS_PER_TICK) // success: return NS
            }
        }
    };
    ctx.r0100 = result;
}

/// Advance virtual time. Returns true if the timer has expired.
///
/// Each call advances `totalcycles` by one trap increment.
/// The caller is responsible for delivering the timer interrupt
/// to avoid corrupting event state mid-handler.
#[cfg(target_arch = "hexagon")]
fn advance_virtual_timer(ctx: &mut ThreadContext) -> bool {
    // Update totalcycles from timer source
    unsafe {
        if QTIMER_ACTIVE {
            ctx.totalcycles = qtimer_read_count();
        } else if USE_VIRTUAL_TICKS {
            VIRTUAL_TICKS += TICKS_PER_TRAP;
            ctx.totalcycles = VIRTUAL_TICKS;
        } else {
            let pcycle_lo: u32;
            let pcycle_hi: u32;
            core::arch::asm!("{lo} = upcyclelo", lo = out(reg) pcycle_lo);
            core::arch::asm!("{hi} = upcyclehi", hi = out(reg) pcycle_hi);
            let upcycles = ((pcycle_hi as u64) << 32) | (pcycle_lo as u64);
            if upcycles == 0 {
                // upcycle registers not supported (QEMU) — switch to software ticks
                USE_VIRTUAL_TICKS = true;
                VIRTUAL_TICKS += TICKS_PER_TRAP;
                ctx.totalcycles = VIRTUAL_TICKS;
            } else {
                ctx.totalcycles = upcycles;
            }
        }
    }

    // Check if a timer timeout is set and has expired (both in tick units)
    ctx.tree_timeout > 0 && ctx.totalcycles >= ctx.tree_timeout
}

/// Deliver a pending timer interrupt to the guest (if conditions are met).
///
/// Called after advance_virtual_timer returns true, or when fire_timer is set.
/// Clears tree_timeout and delivers interrupt 12 (or configured timer_intnum).
#[cfg(target_arch = "hexagon")]
fn deliver_timer_if_ready(ctx: &mut ThreadContext, fire: bool) {
    if fire && ctx.tree_timeout > 0 && ctx.gevb != 0 && ie_enabled(ctx) {
        ctx.tree_timeout = 0;
        let timer_int = unsafe { (*KG_PTR).timer_intnum };
        deliver_interrupt_event(ctx, timer_int);
    }
}

// ============================================================
// QTIMER Hardware Timer Support
// ============================================================
//
// The QTIMER is a 19.2 MHz timer device attached via the L2VIC
// interrupt controller. When the QTIMER counter reaches the match
// register value, it fires a hardware interrupt (CPU interrupt 2
// via L2VIC), which arrives at EVB vector 7.
//
// This provides real timer interrupts that can break the guest out
// of tight CPU loops (where no trap1/trap0 events occur).
//
// MMIO addresses (with --subsystem_base 0xfe28):
//   QTIMER base: 0xfe2a0000

// QTIMER MMIO base address (subsystem_base 0xfe28 + timer offset 0x20000).
// QTIMER/L2VIC hardware addresses — detected at runtime.
//
// hexagon-sim cosim: QTIMER=0xfe2a0000 (frame1 layout), L2VIC=0xfe290000
// QEMU virt:         QTIMER=0xfc921000 (flat layout),   L2VIC=0xfc910000

/// Detected QTIMER base address (set during qtimer_init).
#[cfg(target_arch = "hexagon")]
static mut QTIMER_ADDR: u32 = 0;
/// Detected L2VIC base address.
#[cfg(target_arch = "hexagon")]
static mut L2VIC_ADDR: u32 = 0;
/// True if QTIMER uses flat register layout (QEMU), false for frame1 (sim).
#[cfg(target_arch = "hexagon")]
static mut QTIMER_FLAT: bool = false;
/// True if running on QEMU (detected by QTIMER probe).
#[cfg(target_arch = "hexagon")]
static mut ON_QEMU: bool = false;

/// Read QTIMER counter at a given base address with given layout.
#[cfg(target_arch = "hexagon")]
fn qtimer_read_count_at(base: u32, flat: bool) -> u64 {
    unsafe {
        let p = base as *const u32;
        let (lo_off, hi_off) = if flat {
            (0x000, 0x004)
        } else {
            (0x1000, 0x1004)
        };
        loop {
            let hi1 = p.byte_add(hi_off).read_volatile();
            let lo = p.byte_add(lo_off).read_volatile();
            let hi2 = p.byte_add(hi_off).read_volatile();
            if hi1 == hi2 {
                return ((hi1 as u64) << 32) | (lo as u64);
            }
        }
    }
}

/// Initialize the QTIMER hardware and enable interrupts.
///
/// Probes two known QTIMER locations:
///   1. hexagon-sim cosim at 0xfe2a0000 (frame1 layout)
///   2. QEMU virt at 0xfc921000 (flat layout)
///
/// Falls back to CPU cycle counters if neither is found.
#[cfg(target_arch = "hexagon")]
fn qtimer_init() {
    unsafe {
        // Enable CPU interrupts: unmask all L1 interrupts and clear IAD.
        // IMASK=0 unmasks all 8 L1 interrupt lines (each bit masks one line).
        // ciad(-1) clears the Interrupt Ack/Disable register for all lines.
        // The hypervisor is safe because SSR.EX=1 blocks delivery in monitor mode;
        // only the guest (SSR.EX=0, SSR.IE=1) will actually receive interrupts.
        // Note: context_restore_rte does NOT save/restore IMASK, so whatever
        // we set here persists into guest execution.
        core::arch::asm!(
            "r0 = #0",
            "imask = r0",
            "r0 = #-1",
            "{{ ciad(r0) }}",
            out("r0") _,
        );

        // Probe hexagon-sim QTIMER (frame1 layout at 0xfe2a0000)
        let sim_base: u32 = 0xfe2a_0000;
        let sim_l2vic: u32 = 0xfe29_0000;
        let cnt = qtimer_read_count_at(sim_base, false);
        if cnt != 0 {
            debug::write0(b"qtimer: found at 0xfe2a0000 (sim)\n\0");
            QTIMER_ADDR = sim_base;
            L2VIC_ADDR = sim_l2vic;
            QTIMER_FLAT = false;

            // Init sim QTIMER — enable but do NOT arm (CVAL = MAX).
            // The guest (Linux) programs the timer when ready.
            let base = sim_base as *mut u32;
            base.byte_add(0x0004).write_volatile(!0u32); // CNTSR
            base.byte_add(0x0040).write_volatile(!0u32); // CNTACR
            base.byte_add(0x0000).write_volatile(19_200_000); // CNTFRQ
            base.byte_add(0x102C).write_volatile(1); // ENABLE
            base.byte_add(0x1024).write_volatile(!0u32); // MATCH_HI = MAX
            base.byte_add(0x1020).write_volatile(!0u32); // MATCH_LO = MAX
            base.byte_add(0x1024).write_volatile(!0u32); // MATCH_HI = MAX

            // Init sim L2VIC
            let l2vic = sim_l2vic as *mut u32;
            l2vic.byte_add(0x400).write_volatile(!0u32);
            l2vic.byte_add(0x280).write_volatile(0);
            l2vic.byte_add(0x100).write_volatile(1 << 2);

            QTIMER_ACTIVE = true;
            return;
        }

        // Probe QEMU QTIMER (flat layout at 0xfc921000)
        // NOTE: QEMU's QTIMER ptimer has limit=1, which means the CNTPCT
        // counter only advances by 1 per timer callback. With 1ns period,
        // QEMU can't process billions of callbacks per second, so the
        // counter advances ~100x slower than the expected 19.2MHz.
        // Therefore, do NOT use QTIMER as the time source (QTIMER_ACTIVE=false).
        // Instead, fall through to upcycle counters which advance properly.
        // We still detect QEMU to set IMASK/L2VIC, but timer delivery uses
        // the software path via totalcycles from upcycle counters.
        let qemu_base: u32 = 0xfc92_1000;
        let qemu_l2vic: u32 = 0xfc91_0000;
        let cnt = qtimer_read_count_at(qemu_base, true);
        // Also try kick-starting if counter is 0
        let is_qemu = if cnt != 0 {
            true
        } else {
            // Try kick-start
            let qw = qemu_base as *mut u32;
            qw.byte_add(0x020).write_volatile(!0u32); // CVAL_LO = MAX
            qw.byte_add(0x02c).write_volatile(1); // CNTP_CTL = ENABLE
            for _ in 0..1000 {
                core::hint::spin_loop();
            }
            let cnt2 = qtimer_read_count_at(qemu_base, true);
            cnt2 != 0
        };

        if is_qemu {
            debug::write0(b"qtimer: QEMU detected, using upcycle\n\0");
            QTIMER_ADDR = qemu_base;
            L2VIC_ADDR = qemu_l2vic;
            QTIMER_FLAT = true;

            // Disable the QTIMER to prevent spurious interrupts during guest boot.
            // The guest's vmtimerop handler will re-arm it when needed.
            let qw = qemu_base as *mut u32;
            qw.byte_add(0x02c).write_volatile(0); // CNTP_CTL = disable
            qw.byte_add(0x020).write_volatile(!0u32); // CVAL_LO = MAX
            qw.byte_add(0x024).write_volatile(!0u32); // CVAL_HI = MAX

            // Set up L2VIC for interrupt delivery and clear any pending state.
            let l2vic = qemu_l2vic as *mut u32;
            l2vic.byte_add(0x400).write_volatile(!0u32); // INT_CLEAR
            l2vic.byte_add(0x280).write_volatile(0); // INT_TYPE: level
            l2vic.byte_add(0x200).write_volatile(1 << 2); // INT_ENABLE_SET

            ON_QEMU = true;
            debug::write0(b"qtimer: QEMU detected, using L2VIC timer\n\0");
            return;
        }

        debug::write0(b"qtimer: not found, using upcycle\n\0");
    }
}

/// Program the QTIMER match register.
///
/// When the QTIMER counter reaches `ticks`, a hardware interrupt fires.
/// Pass `!0u64` to disable (set match to far future).
#[cfg(target_arch = "hexagon")]
fn qtimer_set_match(ticks: u64) {
    unsafe {
        let base = QTIMER_ADDR as *mut u32;
        if QTIMER_FLAT {
            base.byte_add(0x024).write_volatile(!0u32);
            base.byte_add(0x020).write_volatile(ticks as u32);
            base.byte_add(0x024).write_volatile((ticks >> 32) as u32);
        } else {
            base.byte_add(0x1024).write_volatile(!0u32);
            base.byte_add(0x1020).write_volatile(ticks as u32);
            base.byte_add(0x1024).write_volatile((ticks >> 32) as u32);
        }
    }
}

/// Read the QTIMER counter (64-bit, atomic).
#[cfg(target_arch = "hexagon")]
fn qtimer_read_count() -> u64 {
    unsafe { qtimer_read_count_at(QTIMER_ADDR, QTIMER_FLAT) }
}

/// Hardware interrupt handler (EVB vector 7).
///
/// Called from assembly after full context save.
/// Handles QTIMER interrupts: delivers timer event to guest and
/// re-arms the QTIMER for the next timeout.
#[cfg(target_arch = "hexagon")]
#[no_mangle]
pub extern "C" fn minivm_int_handler() {
    // QTIMER frequency = 19.2 MHz, Linux HZ = 100 → period = 192000 ticks
    unsafe {
        if !QTIMER_ACTIVE && !ON_QEMU {
            // No L2VIC/QTIMER hardware — nothing to acknowledge or re-arm
            return;
        }

        // Acknowledge L2VIC interrupt 2:
        //   1. Clear active status (INT_CLEARn at 0x400)
        //   2. Re-enable (INT_ENABLE_SETn at 0x200 — sets bits without clearing others)
        let l2vic = L2VIC_ADDR as *mut u32;
        l2vic.byte_add(0x400).write_volatile(1 << 2); // INT_CLEAR: clear int_status
        l2vic.byte_add(0x200).write_volatile(1 << 2); // INT_ENABLE_SET: re-enable

        // Clear the CPU-level Interrupt Acknowledge/Disable (IAD) register.
        // L2_CORE_INTERRUPT = 2 for ARCHV >= 65. Without ciad, the CPU
        // blocks all subsequent L2VIC interrupt delivery.
        core::arch::asm!("r0 = #4", "{{ ciad(r0) }}", out("r0") _);

        // Do NOT re-arm the QTIMER from the hypervisor. The guest
        // programs the timer itself via memory-mapped QTIMER registers.
        // Re-arming here would conflict with the guest's timer programming.

        let ctx_ptr: u32;
        core::arch::asm!("{val} = sgp0", val = out(reg) ctx_ptr);
        let ctx = &mut *(ctx_ptr as *mut ThreadContext);

        // Update totalcycles from QTIMER counter (or upcycle if QTIMER inactive)
        if QTIMER_ACTIVE {
            ctx.totalcycles = qtimer_read_count();
        }

        // Deliver timer interrupt to guest via GEVB interrupt vector,
        // but only if the guest has an active timer request.
        if ctx.tree_timeout > 0 && ie_enabled(ctx) && ctx.gevb != 0 {
            ctx.tree_timeout = 0;
            let timer_int = (*KG_PTR).timer_intnum;
            deliver_interrupt_event(ctx, timer_int);
        }
    }
}

/// Handle info trap (trap1 #26) — system information queries.
///
/// r0 = info type (InfoType). Returns result in r0.
#[cfg(target_arch = "hexagon")]
fn guest_handle_info(ctx: &mut ThreadContext) {
    let info_raw = ctx_r0(ctx);

    let Some(info_type) = InfoType::from_raw(info_raw) else {
        set_r0(ctx, 0);
        return;
    };

    let kg = unsafe { &*KG_PTR };

    let result: u32 = match info_type {
        InfoType::Rev => kg.core_rev.arch as u32,
        InfoType::HThreads => kg.hthreads_mask,
        InfoType::TimerInt => kg.timer_intnum,
        InfoType::TlbSize => kg.tlb_size,
        InfoType::TlbFree => kg.tlb_size, // simplified: all free on sim
        InfoType::BootFlags => {
            use minivm_init::globals::BootFlags;
            let mut flags = minivm_types::info::BootFlags(0);
            if kg.boot_flags.contains(BootFlags::HAVE_HVX) {
                flags = flags.with_have_hvx(true);
            }
            if kg.boot_flags.contains(BootFlags::USE_TCM) {
                flags = flags.with_use_tcm(true);
            }
            flags.0
        }
        InfoType::KernelPgSize => 0x10000, // 64KB kernel page size
        InfoType::KernelNPages => 16,
        InfoType::L2MemSize => kg.l2size,
        InfoType::TcmBase => kg.tcm_base,
        InfoType::TcmSize => kg.tcm_size,
        InfoType::VtcmBase => kg.vtcm_base,
        InfoType::VtcmSize => kg.vtcm_size,
        InfoType::CoreId => kg.core_id,
        InfoType::CoreCount => kg.core_count,
        InfoType::CoprocContexts => kg.coproc.coproc_contexts,
        InfoType::HvxVLength => kg.coproc.hvx_vlength,
        InfoType::BuildId => kg.build_id,
        InfoType::PhysAddr => minivm_init::globals::KERNEL_LINK_ADDR,
        InfoType::TimerBase => 0,
        InfoType::L2VicBase => 0,
        InfoType::Error => 0,
        InfoType::Stlb => 0,
        InfoType::Syscfg => 0,
        InfoType::SsBase => 0,
        InfoType::Shift => kg.multicore_shift,
        InfoType::NocMBase => kg.noc_mbase,
        InfoType::NocSBase => kg.noc_sbase,
        _ => 0,
    };
    set_r0(ctx, result);
}

/// Boot a guest VM (VMOP_BOOT + thread creation without squashing the caller).
///
/// Creates a guest ThreadContext, allocates an ASID for the guest's address
/// space (identity offset translation), and switches to the guest via minivm_switch.
///
/// The guest runs until it does trap1(#19) [stop], at which point the stop
/// handler switches SGP0 back to CTX_OLD and minivm_switch returns here.
///
/// Parameters:
///   - `pc`: guest entry point
///   - `sp`: guest stack pointer (must be 8-byte aligned)
///   - `arg1`: value passed to guest in r0
///   - `prio`: guest thread priority
///   - `vm`: mutable reference to the guest's VmBlock
///   - `asid_table`: ASID table for address translation registration
///   - `kg`: kernel globals (needed for TLB miss handler)
///
/// Returns the guest's exit code (from the stop trap r0 value).
#[cfg(all(target_arch = "hexagon", feature = "run-tests"))]
pub(crate) fn vmboot(
    pc: u32,
    sp: u32,
    arg1: u32,
    prio: u8,
    vm: &mut VmBlock,
    asid_table: &mut AsidTable,
    kg: &KernelGlobals,
) -> u32 {
    use minivm_types::asid::TranslationType;
    use minivm_types::config::OffsetConfig;

    extern "C" {
        static __global_pointer: u8;
        fn minivm_switch(old: *mut ThreadContext, new: *mut ThreadContext);
    }

    // 1. Allocate ASID with identity offset (same as C identity_offset)
    //    size=6, cccc=7 (L1WB_L2C), weak_ccc=true, xwru=0xF (URWX), pages=0
    let identity_offset = OffsetConfig::new(6, 7, true, 0xF, 0);
    let asid = asid_table.inc(
        identity_offset.0,
        TranslationType::Offset,
        false,
        0,
        vm.vmidx as u8,
        |_| {},
    );
    if asid < 0 {
        return 0xFFFF_FFFF;
    }

    // 2. Wire up global pointers for TLB miss handler and guest trap handler
    unsafe {
        ASID_TABLE_PTR = asid_table as *const AsidTable;
        KG_PTR = kg as *const KernelGlobals;
        BOOT_CPUINT = CpuIntState::new();
    }

    // 3. Build guest ThreadContext
    let mut guest_ctx = ThreadContext::zeroed();

    // Entry point and stack
    guest_ctx.elr = pc;
    guest_ctx.r2928 = {
        let gp = unsafe { &__global_pointer as *const u8 as u32 };
        ((sp as u64) << 32) | (gp as u64)
    };
    guest_ctx.r0100 = arg1 as u64;

    // SSR: GUEST=1, ASID=allocated
    // Note: UM bit is NOT set here. On real hardware, the first guest thread
    // starts in supervisor mode within the guest. Linux quickly sets up its
    // own exception vectors and enters user mode itself.
    let ssr = Ssr::new(0).with_guest(true).with_asid(asid as u8);
    guest_ctx.ssr = ssr.0;

    // CCR from boot defaults
    guest_ctx.ccr = minivm_types::regs::boot_defaults::THREAD_CCR;

    // USR from boot defaults
    guest_ctx.usrp30 = (minivm_types::regs::boot_defaults::THREAD_USR as u64) << 32;

    // VM identity
    guest_ctx.id = VmId::new(vm.vmidx as u8, 0, 0);
    guest_ctx.vmblock = vm as *mut VmBlock as u32;
    guest_ctx.prio = prio;
    guest_ctx.base_prio = prio;
    guest_ctx.trapmask = vm.trapmask;
    guest_ctx.tlbidxmask = vm.tlbidxmask;

    // GEVB = 0 for first CPU (guest sets via setvec)
    guest_ctx.gevb = 0;
    guest_ctx.vmstatus = 0;

    // Track CPU count
    vm.num_cpus += 1;

    // 4. Clear result, enter exception mode, switch to guest
    unsafe {
        GUEST_VERSION_RESULT = 0;
    }

    unsafe {
        core::arch::asm!(
            "r0 = ##0x20000",
            "ssr = r0",
            "isync",
            out("r0") _,
        );
    }

    unsafe {
        minivm_switch(
            core::ptr::addr_of_mut!(CTX_OLD),
            &mut guest_ctx as *mut ThreadContext,
        );
    }

    // Guest has stopped — decrement CPU count and return exit code
    vm.num_cpus -= 1;
    unsafe { GUEST_VERSION_RESULT }
}

/// Wrapper for trap0(#28) VMOP_BOOT with full ABI save/restore.
///
/// The `#[inline(never)]` ensures a proper function call boundary, so the
/// compiler saves/restores all callee-saved registers. The trap handler's
/// context save/restore might interfere with the compiler's register
/// assumptions in the caller, so isolating the trap in its own function
/// prevents register corruption issues.
#[cfg(target_arch = "hexagon")]
#[inline(never)]
fn do_vmop_boot(pc: u32, sp: u32, arg1: u32, prio: u32, vm_idx: u32) -> u32 {
    let result: u32;
    unsafe {
        core::arch::asm!(
            "trap0(#28)",
            inout("r0") 0u32 => result,
            in("r1") pc,
            in("r2") sp,
            in("r3") arg1,
            in("r4") prio,
            in("r5") vm_idx,
            lateout("r6") _,
            lateout("r7") _,
            lateout("r8") _,
            lateout("r9") _,
            lateout("r10") _,
            lateout("r11") _,
            lateout("r12") _,
            lateout("r13") _,
            lateout("r14") _,
            lateout("r15") _,
        );
    }
    result
}

/// Boot a guest binary via VMOP_BOOT.
///
/// The guest binary must be pre-loaded at `GUEST_ENTRY_VA` (e.g. via
/// QEMU `-device loader,addr=0xa0000000,file=guest.bin`).
///
/// 1. CONFIG SET_CPUS_INTS → create child VM
/// 2. CONFIG SET_PMAP_TYPE, SET_FENCES, SET_PRIO_TRAPMASK
/// 3. MAP_PHYS_INTR for each interrupt
/// 4. VMOP_BOOT → enters guest
///
/// Returns when the guest stops (or never if the guest runs indefinitely).
#[cfg(target_arch = "hexagon")]
fn boot_guest() {
    use minivm_types::config::PhysintConfig;

    const GUEST_ENTRY_VA: u32 = 0xa0000000;
    const GUEST_NUM_VCPU: u32 = 1;
    const TOTAL_INTS: u32 = 288;
    const SHARED_INTS: u32 = TOTAL_INTS + 32;
    const GUEST_VM_PRIO: u32 = 3;

    // Enable cycle counters: SSR.CE (bit 23) must be set for
    // upcyclelo/upcyclehi to return non-zero values on QEMU.
    unsafe {
        let ssr: u32;
        core::arch::asm!("{val} = ssr", val = out(reg) ssr);
        core::arch::asm!("ssr = {val}", val = in(reg) ssr | (1u32 << 23));
    }

    // Set SGP0 to MONITOR_CTX before any trap0/trap1 instructions.
    // The trap handler uses crswap(r0, sgp0) to save/restore context.
    unsafe {
        let monitor_ptr = core::ptr::addr_of_mut!(MONITOR_CTX) as u32;
        core::arch::asm!("sgp0 = {val}", val = in(reg) monitor_ptr);
    }

    debug::write0(b"guest: start boot\n\0");

    // 1. CONFIG SET_CPUS_INTS — allocate child VM
    let vm_idx: u32;
    unsafe {
        core::arch::asm!(
            "trap0(#30)",
            inout("r0") 0u32 => vm_idx,
            in("r1") 0u32,
            in("r2") 3u32,               // SET_CPUS_INTS
            in("r3") GUEST_NUM_VCPU,
            in("r4") SHARED_INTS,
            out("r5") _,
        );
    }
    if vm_idx == 0 {
        debug::write0(b"guest: FAIL SET_CPUS_INTS\n\0");
        return;
    }

    // 2. CONFIG SET_PMAP_TYPE — identity offset translation (pages=0)
    // SIZE_4M=5, L1WB_L2C=7, URWX=0xF, pages=0
    let guest_offset = minivm_types::config::OffsetConfig::new(5, 7, false, 0xF, 0);
    let pmap_result: u32;
    unsafe {
        core::arch::asm!(
            "trap0(#30)",
            inout("r0") 0u32 => pmap_result,
            in("r1") vm_idx,
            in("r2") 0u32,               // SET_PMAP_TYPE
            in("r3") guest_offset.0,
            in("r4") 1u32,               // OFFSET type
            out("r5") _,
        );
    }
    if pmap_result != vm_idx {
        debug::write0(b"guest: FAIL SET_PMAP_TYPE\n\0");
        return;
    }

    // 3. CONFIG SET_FENCES — match C loadlinux: raw 0 to 0xFE000000
    // (stored as-is; any 20-bit page number passes the fence check)
    let fence_result: u32;
    unsafe {
        core::arch::asm!(
            "trap0(#30)",
            inout("r0") 0u32 => fence_result,
            in("r1") vm_idx,
            in("r2") 1u32,               // SET_FENCES
            in("r3") 0u32,               // fence_lo
            in("r4") 0xFE000000u32,      // fence_hi (raw, matching C)
            out("r5") _,
        );
    }
    if fence_result != vm_idx {
        debug::write0(b"guest: FAIL SET_FENCES\n\0");
        return;
    }

    // 4. CONFIG SET_PRIO_TRAPMASK
    let trapmask_result: u32;
    unsafe {
        core::arch::asm!(
            "trap0(#30)",
            inout("r0") 0u32 => trapmask_result,
            in("r1") vm_idx,
            in("r2") 2u32,               // SET_PRIO_TRAPMASK
            in("r3") 0u32,               // bestprio
            in("r4") 0x1u32,             // trapmask (same as C loadlinux)
            out("r5") _,
        );
    }
    if trapmask_result != vm_idx {
        debug::write0(b"guest: FAIL SET_PRIO_TRAPMASK\n\0");
        return;
    }

    // 5. MAP_PHYS_INTR for each interrupt (identity mapping: virt N → phys N)
    for i in 0..TOTAL_INTS {
        let physint_cfg = PhysintConfig::new(i as u16, (GUEST_NUM_VCPU - 1) as u16);
        let map_result: u32;
        unsafe {
            core::arch::asm!(
                "trap0(#30)",
                inout("r0") 0u32 => map_result,
                in("r1") vm_idx,
                in("r2") 4u32,            // MAP_PHYS_INTR
                in("r3") i,
                in("r4") physint_cfg.0,
                out("r5") _,
            );
        }
        if map_result != vm_idx {
            debug::write0(b"guest: FAIL MAP_PHYS_INTR\n\0");
            return;
        }
    }
    debug::write0(b"guest: vm configured\n\0");

    // Guest binary is pre-loaded at GUEST_ENTRY_VA by QEMU's -device loader.

    // 6. Set up SYSCFG — enable DMT (bit 15), clear BQ (bit 13).
    // The C loadlinux does this before minivm_vmboot. DMT (Dual Memory Translation)
    // is needed for proper TLB operation in guest mode.
    let syscfg_before: u32;
    let syscfg_after: u32;
    unsafe {
        core::arch::asm!("{val} = syscfg", val = out(reg) syscfg_before);
        let mut val = syscfg_before;
        val |= 1 << 4; // G: guest mode enable
        val |= 1 << 15; // DMT: dual memory translation
        val &= !(1u32 << 13); // clear BQ
        core::arch::asm!("syscfg = {v}", v = in(reg) val);
        core::arch::asm!("{val} = syscfg", val = out(reg) syscfg_after);
    }
    print_hex(b"guest: syscfg_before=", syscfg_before);
    print_hex(b"guest: syscfg_after=", syscfg_after);

    // 6b. Initialize QTIMER (if present) and enable interrupts
    qtimer_init();

    unsafe {
        print_hex(b"guest: qtimer_active=", QTIMER_ACTIVE as u32);
    }

    // Set SGP0 explicitly to MONITOR_CTX before VMOP_BOOT.
    // On QEMU, trap0(#0) semihosting fires the EVB exception even in monitor mode.
    // This causes the context save/restore cycle to use SGP0 as the context buffer.
    // Previous tests may have left SGP0 pointing to a test context (TRAP0_CTX).
    // We need SGP0 = MONITOR_CTX so the VMOP_BOOT handler saves the correct caller
    // context for the Stop handler to restore.
    unsafe {
        let monitor_ptr = core::ptr::addr_of_mut!(MONITOR_CTX) as u32;
        core::arch::asm!("sgp0 = {val}", val = in(reg) monitor_ptr);
        // Verify
        let sgp0_check: u32;
        core::arch::asm!("{val} = sgp0", val = out(reg) sgp0_check);
        print_hex(b"guest: sgp0_set=", sgp0_check);
    }

    debug::write0(b"guest: booting\n\0");
    let boot_result = do_vmop_boot(
        GUEST_ENTRY_VA,
        GUEST_ENTRY_VA + 0x100_0000,
        0,
        GUEST_VM_PRIO,
        vm_idx,
    );
    // Set a global flag FIRST — before any semihosting calls.
    // This verifies we reach this point even if semihosting is broken.
    unsafe {
        BOOT_GUEST_RETURNED = 0xDEAD_BEEF;
    }
    debug::write0(b"guest: stopped\n\0");

    // Print exit code
    let hex = b"0123456789ABCDEF";
    let mut buf = [0u8; 12];
    buf[0] = b'0';
    buf[1] = b'x';
    for i in 0..8 {
        buf[9 - i] = hex[((boot_result >> (i * 4)) & 0xF) as usize];
    }
    buf[10] = b'\n';
    buf[11] = 0;
    debug::write0(b"guest: exit code=\0");
    debug::write0(&buf);
}

/// Maximum FDT size to scan (64 KiB).
const MAX_FDT_SIZE: usize = 0x10000;

/// Main entry point called from assembly after stack/GP setup.
///
/// `fdt_phys` is the FDT physical address passed by QEMU in r1:0 at boot.
/// On bare-metal sim (no FDT), this will be 0 or garbage.
#[no_mangle]
pub extern "C" fn minivm_main(fdt_phys: u64) -> ! {
    debug::init();
    debug::write0(b"minivm: Hexagon VM (Rust)\n\0");

    // === FDT Discovery ===
    let fdt_info = parse_fdt(fdt_phys);
    if fdt_info.valid {
        debug::write0(b"  [fdt] device tree found\n\0");
    }

    // === Subsystem Initialization ===

    debug::write0(b"  [init] kernel globals\n\0");
    // KernelGlobals and AsidTable must be static because trap handlers reset
    // the stack pointer to __stack_end (see context_save), clobbering any
    // locals in minivm_main. KG_PTR and ASID_TABLE_MUT_PTR reference these.
    // Safety: KernelGlobals is all-zero-valid and we call kg.init() immediately.
    static mut KG_STORAGE: KernelGlobals = unsafe { core::mem::zeroed() };
    let kg = unsafe { &mut *core::ptr::addr_of_mut!(KG_STORAGE) };
    kg.init(0, 0, 128, 0, 1);
    kg.hthreads = 1;
    kg.hthreads_mask = 1;
    kg.core_rev.arch = 0x81;
    kg.timer_intnum = if fdt_info.timer_irq != 0 {
        fdt_info.timer_irq
    } else {
        12 // default: must match qemu_virt.dts: gpt interrupts = <12 0>
    };

    debug::write0(b"  [init] scheduler\n\0");
    let readylist = ReadyList::new();
    let runlist = RunList::new(kg.hthreads);
    let _lowprio = LowPrio::new();
    assert_eq!(readylist.best_prio(), minivm_types::consts::MAX_PRIOS);
    assert_eq!(runlist.hthreads, kg.hthreads);

    debug::write0(b"  [init] ASID table\n\0");
    static mut ASID_STORAGE: AsidTable = AsidTable::new();
    let asid_table = unsafe { &mut *core::ptr::addr_of_mut!(ASID_STORAGE) };
    asid_table.init();

    debug::write0(b"  [init] interrupt controller\n\0");
    let _cpuint = CpuIntState::new();
    let mut shint = SharedIntState::new();
    shint.init(MAX_INTERRUPTS as u32, MAX_HTHREADS as u32);

    debug::write0(b"  [init] timer\n\0");
    let _timer = TimerState::new();

    debug::write0(b"  [init] futex table\n\0");
    let _futex = FutexTable::new();

    debug::write0(b"  [init] thread contexts\n\0");
    let ctx = ThreadContext::zeroed();
    assert_eq!(ctx.status, 0);

    debug::write0(b"  [init] boot VM\n\0");
    let mut bootvm = VmBlock::new(0);
    let res = vmconfig::set_cpus_ints(&mut bootvm, 4, MAX_INTERRUPTS as u32);
    assert_eq!(res, vmconfig::ConfigResult::Ok);
    let res = vmconfig::set_fences(&mut bootvm, 0, 0xFE000);
    assert_eq!(res, vmconfig::ConfigResult::Ok);
    let res = vmconfig::set_prio_trapmask(&mut bootvm, 0, 0xFFFF_FFFF);
    assert_eq!(res, vmconfig::ConfigResult::Ok);
    assert_eq!(bootvm.max_cpus, 4);
    assert_eq!(bootvm.num_ints, MAX_INTERRUPTS as u32);

    debug::write0(b"  [init] memory layout\n\0");
    let layout = boot::compute_memory_layout(0xFF01_0000, 0x1_0000, 0x2_0000, 0x10_0000);
    assert!(layout.heap_start >= 0xFF01_0000);
    assert!(layout.alloc_heap_size == 0x10_0000);

    // === On-Target Integration Tests ===

    #[cfg(feature = "run-tests")]
    tests::run_all_tests(kg, asid_table);

    // === Guest Boot (binary pre-loaded at 0xa0000000 by QEMU loader) ===

    #[cfg(target_arch = "hexagon")]
    {
        // Wire up globals needed by trap0 CONFIG/VMOP handlers.
        unsafe {
            KG_PTR = kg as *const KernelGlobals;
            ASID_TABLE_MUT_PTR = asid_table as *mut AsidTable;
        }
        boot_guest();
    }

    // === Post-boot diagnostics (test build only) ===

    #[cfg(all(target_arch = "hexagon", feature = "run-tests"))]
    {
        // Check if code after trap0(#28) executed
        let flag = unsafe { BOOT_GUEST_RETURNED };
        print_hex(b"post-boot flag=", flag);
        // Check if MONITOR_CTX was corrupted during guest execution
        let monitor_elr = unsafe { MONITOR_CTX.elr };
        let monitor_ssr = unsafe { MONITOR_CTX.ssr };
        print_hex(b"post-boot monitor_elr=", monitor_elr);
        print_hex(b"post-boot monitor_ssr=", monitor_ssr);
        // Check SGP0 value
        let sgp0_val: u32;
        unsafe {
            core::arch::asm!("{val} = sgp0", val = out(reg) sgp0_val);
        }
        print_hex(b"post-boot sgp0=", sgp0_val);
        print_hex(b"post-boot caller_sgp0=", unsafe { VMOP_BOOT_CALLER_SGP0 });
        debug::write0(b"minivm: boot_guest returned\n\0");
    }

    // === Summary ===

    #[cfg(feature = "run-tests")]
    {
        debug::write0(b"minivm: All tests passed\n\0");
        debug::write0(b"TEST PASSED\n\0");
    }
    #[cfg(not(feature = "run-tests"))]
    {
        debug::write0(b"minivm: boot complete\n\0");
    }
    semihosting::exit(0);
}
