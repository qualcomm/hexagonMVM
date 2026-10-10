/*
 * Copyright (c) Qualcomm Technologies, Inc. and/or its subsidiaries.
 * SPDX-License-Identifier: BSD-3-Clause-Clear
 */

//! Kernel global state.
//!
//! The kernel globals struct holds all kernel state that is accessed
//! via the GP register (r28), grouped by subsystem.

use minivm_power::hvx::HvxState;
use minivm_types::arch::ArchVersion;

/// Maximum hardware threads.
pub const MAX_HTHREADS: usize = 6;

/// Maximum virtual machines.
pub const MAX_VMS: usize = 64;

/// Maximum physical interrupts (L1 + L2).
pub const MAX_INTERRUPTS: usize = 320;

/// Maximum ASID table entries.
pub const MAX_ASIDS: usize = 128;

/// Futex hash table size.
pub const FUTEX_HASHSIZE: usize = 64;

/// Kernel link address.
pub const KERNEL_LINK_ADDR: u32 = 0xFF00_0000;

/// Default alloc heap size.
pub const DEFAULT_ALLOC_HEAP_SIZE: u32 = 0x10_0000; // 1 MB

/// Default stack size.
pub const DEFAULT_STACK_SIZE: u32 = 0x10_0000; // 1 MB

/// Core revision info (packed union in C).
#[derive(Clone, Copy, Debug, Default)]
pub struct CoreRev {
    /// Architecture version (v65, v68, etc.).
    pub arch: u8,
    /// Microarchitecture version.
    pub uarch: u8,
    /// L2 array size encoding.
    pub l2arr: u8,
}

impl CoreRev {
    /// Get the ArchVersion enum from the raw arch byte.
    pub fn arch_version(&self) -> Option<ArchVersion> {
        ArchVersion::from_rev(self.arch)
    }
}

/// Interrupt handler entry (function pointer + parameter).
#[derive(Clone, Copy, Debug, Default)]
pub struct IntHandler {
    /// Handler function address (0 = no handler).
    pub handler: u32,
    /// Parameter passed to handler.
    pub param: u32,
}

/// Coprocessor state tracking.
#[derive(Clone, Copy, Debug, Default)]
pub struct CoprocState {
    pub hvx_state: HvxState,
    pub hvx_vlength: u32,
    pub coproc_contexts: u32,
    pub _reserved2: [u32; 2],
}

bitflags::bitflags! {
    /// Boot flags (persisted for INFO queries).
    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
    pub struct BootFlags: u8 {
        const HAVE_HVX = 0x01;
        const USE_TCM  = 0x02;
    }
}

/// Kernel global state.
///
/// On actual hardware, this struct is accessed via the GP (r28) register.
pub struct KernelGlobals {
    // Hardware info
    pub core_rev: CoreRev,
    pub hthreads: u32,
    pub hthreads_mask: u32,
    pub core_id: u32,
    pub core_count: u32,
    pub multicore_shift: u32,

    // Memory
    pub phys_offset: i32,
    pub tcm_base: u32,
    pub tcm_size: u32,
    pub vtcm_base: u32,
    pub vtcm_size: u32,
    pub l2size: u32,
    pub l2tags: u32,

    // TLB
    pub tlb_size: u32,
    pub tlb_index: u32,
    pub last_tlb_index: u32,
    pub pinned_tlb_mask: u64,

    // Interrupts
    pub inthandlers: [IntHandler; MAX_INTERRUPTS],
    pub timer_intnum: u32,

    // NOC
    pub noc_mbase: u32,
    pub noc_sbase: u32,

    // Memory allocation
    pub alloc_heap_size: u32,

    // Coprocessors
    pub coproc: CoprocState,

    // Boot
    pub boot_flags: BootFlags,
    pub build_id: u32,
}

impl Default for KernelGlobals {
    fn default() -> Self {
        Self::new()
    }
}

impl KernelGlobals {
    /// Create a new KernelGlobals with default/zero values.
    pub fn new() -> Self {
        Self {
            core_rev: CoreRev::default(),
            hthreads: 0,
            hthreads_mask: 0,
            core_id: 0,
            core_count: 0,
            multicore_shift: 0,
            phys_offset: 0,
            tcm_base: 0,
            tcm_size: 0,
            vtcm_base: 0,
            vtcm_size: 0,
            l2size: 0,
            l2tags: 0,
            tlb_size: 0,
            tlb_index: 0,
            last_tlb_index: 0,
            pinned_tlb_mask: 0,
            inthandlers: [IntHandler::default(); MAX_INTERRUPTS],
            timer_intnum: 0,
            noc_mbase: 0,
            noc_sbase: 0,
            alloc_heap_size: DEFAULT_ALLOC_HEAP_SIZE,
            coproc: CoprocState::default(),
            boot_flags: BootFlags::default(),
            build_id: 0,
        }
    }

    /// Initialize the globals with boot parameters.
    pub fn init(
        &mut self,
        phys_offset: i32,
        multicore_shift: u32,
        tlb_size: u32,
        core_id: u32,
        core_count: u32,
    ) {
        self.phys_offset = phys_offset;
        self.multicore_shift = multicore_shift;
        self.tlb_size = tlb_size;
        self.core_id = core_id;
        self.core_count = core_count;
    }

    /// Get the architecture version.
    pub fn arch_version(&self) -> Option<ArchVersion> {
        self.core_rev.arch_version()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_globals_new() {
        let kg = KernelGlobals::new();
        assert_eq!(kg.hthreads, 0);
        assert_eq!(kg.core_id, 0);
        assert_eq!(kg.tlb_size, 0);
        assert_eq!(kg.alloc_heap_size, DEFAULT_ALLOC_HEAP_SIZE);
        assert_eq!(kg.coproc.hvx_state, HvxState::Off);
    }

    #[test]
    fn test_globals_init() {
        let mut kg = KernelGlobals::new();
        kg.init(-0x1000, 0, 128, 0, 1);
        assert_eq!(kg.phys_offset, -0x1000);
        assert_eq!(kg.tlb_size, 128);
        assert_eq!(kg.core_count, 1);
    }

    #[test]
    fn test_core_rev() {
        let rev = CoreRev {
            arch: 0x68,
            ..CoreRev::default()
        };
        assert_eq!(rev.arch_version(), Some(ArchVersion::V68));
    }

    #[test]
    fn test_core_rev_unknown() {
        let rev = CoreRev::default();
        assert_eq!(rev.arch_version(), None);
    }

    #[test]
    fn test_boot_flags_default() {
        let flags = BootFlags::default();
        assert!(flags.is_empty());
        assert!(!flags.contains(BootFlags::HAVE_HVX));
        assert!(!flags.contains(BootFlags::USE_TCM));
    }

    #[test]
    fn test_boot_flags_bitflags() {
        let flags = BootFlags::HAVE_HVX | BootFlags::USE_TCM;
        assert!(flags.contains(BootFlags::HAVE_HVX));
        assert!(flags.contains(BootFlags::USE_TCM));
    }
}
