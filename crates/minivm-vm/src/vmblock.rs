/*
 * Copyright (c) Qualcomm Technologies, Inc. and/or its subsidiaries.
 * SPDX-License-Identifier: BSD-3-Clause-Clear
 */

//! VmBlock struct — the main VM control block.
//!
//! Each VM has a VmBlock that contains its identity, memory configuration,
//! interrupt state, thread pool, and scheduling parameters.

use minivm_intc::shared::SharedIntState;
use minivm_types::asid::AsidEntry;
use minivm_types::config::OffsetConfig;
use minivm_types::vm::{VmId, VM_VERSION};

/// V2P invalid marker.
pub const V2P_INVALID: u16 = 0;

/// VmBlock alignment.
pub const VMBLOCK_ALIGN: usize = 32;

/// Flags for the VM block.
#[derive(Clone, Copy, Debug, Default)]
pub struct VmFlags {
    /// Guest wants kernel extension switch.
    pub use_ext: bool,
    /// Extension context switch is active.
    pub do_ext: bool,
}

/// Virtual machine control block.
///
/// Fields are kept as simple types for testability; the actual
/// hardware integration uses these through accessor methods.
pub struct VmBlock {
    // Identity
    /// VM index (0..MAX_VMS).
    pub vmidx: u32,
    /// Parent VM ID.
    pub parent: VmId,
    /// VM exit status.
    pub status: i32,
    /// Allowed trap mask.
    pub trapmask: u32,

    // Memory
    /// Guest ASID mapping entry.
    pub guestmap: AsidEntry,
    /// TLB index mask for this VM's threads.
    pub tlbidxmask: u8,
    /// Physical offset (for offset translation mode).
    pub phys_offset: OffsetConfig,
    /// Low fence (pages).
    pub fence_lo: i32,
    /// High fence (pages).
    pub fence_hi: i32,

    // Interrupts
    /// Shared interrupt state.
    pub shint: SharedIntState,
    /// Virtual-to-physical interrupt mapping.
    pub int_v2p: [u16; 512],
    /// Number of shared interrupts configured.
    pub num_ints: u32,
    /// Bitmask of CPUs in VMWAIT state (up to 64).
    pub waiting_cpus: u64,

    // Threads
    /// Maximum CPUs allocated for this VM.
    pub max_cpus: u32,
    /// Currently active CPUs.
    pub num_cpus: u32,
    /// Best (lowest) allowed priority for this VM's threads.
    pub bestprio: u32,
    /// Head of free thread list (index, 0 = empty).
    pub free_threads: u32,

    // Flags
    pub flags: VmFlags,
}

impl VmBlock {
    /// Create a new, empty VM block.
    pub fn new(vmidx: u32) -> Self {
        Self {
            vmidx,
            parent: VmId::default(),
            status: 0,
            trapmask: 0,
            guestmap: AsidEntry::EMPTY,
            tlbidxmask: 0,
            phys_offset: OffsetConfig::default(),
            fence_lo: 0,
            fence_hi: 0,
            shint: SharedIntState::new(),
            int_v2p: [V2P_INVALID; 512],
            num_ints: 0,
            waiting_cpus: 0,
            max_cpus: 0,
            num_cpus: 0,
            bestprio: 0,
            free_threads: 0,
            flags: VmFlags::default(),
        }
    }

    /// Check if this VM has any waiting CPUs.
    pub fn has_waiting_cpus(&self) -> bool {
        self.waiting_cpus != 0
    }

    /// Mark a CPU as waiting (VMWAIT state).
    pub fn mark_cpu_waiting(&mut self, cpuidx: u32) {
        if cpuidx < 64 {
            self.waiting_cpus |= 1u64 << cpuidx;
        }
    }

    /// Clear a CPU's waiting flag.
    pub fn clear_cpu_waiting(&mut self, cpuidx: u32) {
        if cpuidx < 64 {
            self.waiting_cpus &= !(1u64 << cpuidx);
        }
    }

    /// Get the supported VM version.
    pub fn version() -> u32 {
        VM_VERSION
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_vmblock_new() {
        let vm = VmBlock::new(1);
        assert_eq!(vm.vmidx, 1);
        assert_eq!(vm.num_cpus, 0);
        assert_eq!(vm.max_cpus, 0);
        assert_eq!(vm.status, 0);
        assert!(!vm.has_waiting_cpus());
    }

    #[test]
    fn test_waiting_cpus() {
        let mut vm = VmBlock::new(1);
        vm.mark_cpu_waiting(3);
        assert!(vm.has_waiting_cpus());
        assert_eq!(vm.waiting_cpus, 1 << 3);

        vm.mark_cpu_waiting(7);
        assert_eq!(vm.waiting_cpus, (1 << 3) | (1 << 7));

        vm.clear_cpu_waiting(3);
        assert_eq!(vm.waiting_cpus, 1 << 7);
    }

    #[test]
    fn test_version() {
        assert_eq!(VmBlock::version(), 0x00000800);
    }
}
