/*
 * Copyright (c) Qualcomm Technologies, Inc. and/or its subsidiaries.
 * SPDX-License-Identifier: BSD-3-Clause-Clear
 */

//! VM configuration.
//!
//! VM configuration is performed through a series of sub-operations
//! after initial vmblock allocation (SET_CPUS_INTS, SET_PMAP_TYPE,
//! SET_FENCES, SET_PRIO_TRAPMASK, MAP_PHYS_INTR).

use crate::vmblock::VmBlock;
use minivm_types::asid::AsidEntry;
use minivm_types::config::{OffsetConfig, PhysintConfig, VmblockInitOp};

/// Result of a vmblock configuration operation.
#[derive(Debug, PartialEq, Eq)]
pub enum ConfigResult {
    /// Operation succeeded.
    Ok,
    /// Invalid argument.
    BadArg,
    /// Out of memory.
    NoMem,
}

/// Set the number of CPUs and interrupts for a VM.
///
/// This is the SET_CPUS_INTS sub-operation.
pub fn set_cpus_ints(vm: &mut VmBlock, max_cpus: u32, num_ints: u32) -> ConfigResult {
    if max_cpus == 0 {
        return ConfigResult::BadArg;
    }
    vm.max_cpus = max_cpus;
    vm.num_ints = num_ints;

    // Initialize shared interrupt state if interrupts configured
    if num_ints > 0 {
        vm.shint.init(num_ints, max_cpus);
    }

    ConfigResult::Ok
}

/// Set the physical memory fences for a VM.
///
/// `fence_lo` and `fence_hi` are page numbers.
pub fn set_fences(vm: &mut VmBlock, fence_lo: i32, fence_hi: i32) -> ConfigResult {
    vm.fence_lo = fence_lo;
    vm.fence_hi = fence_hi;
    ConfigResult::Ok
}

/// Set the priority and trap mask for a VM.
///
/// `bestprio`: best (lowest number = highest) priority allowed.
/// `trapmask`: bitmask of enabled trap numbers.
pub fn set_prio_trapmask(vm: &mut VmBlock, bestprio: u32, trapmask: u32) -> ConfigResult {
    vm.bestprio = bestprio;
    vm.trapmask = trapmask;
    ConfigResult::Ok
}

/// Set the PMAP type (translation mode) for a VM.
///
/// `guestmap`: the ASID entry to use for the guest.
/// `phys_offset`: the physical offset configuration (for offset mode).
/// `tlbidxmask`: TLB index mask for this VM.
pub fn set_pmap_type(
    vm: &mut VmBlock,
    guestmap: AsidEntry,
    phys_offset: OffsetConfig,
    tlbidxmask: u8,
) -> ConfigResult {
    vm.guestmap = guestmap;
    vm.phys_offset = phys_offset;
    vm.tlbidxmask = tlbidxmask;
    ConfigResult::Ok
}

/// Map a physical interrupt to a virtual interrupt for a VM.
///
/// `virt_int`: virtual interrupt number (in the VM's namespace).
/// `phys_int`: physical interrupt number.
pub fn map_phys_intr(vm: &mut VmBlock, virt_int: u32, phys_int: u16) -> ConfigResult {
    if virt_int as usize >= vm.int_v2p.len() {
        return ConfigResult::BadArg;
    }
    vm.int_v2p[virt_int as usize] = phys_int;
    ConfigResult::Ok
}

/// Dispatch a vmblock_init sub-operation.
pub fn vmblock_init_dispatch(
    vm: &mut VmBlock,
    op: VmblockInitOp,
    val1: u32,
    val2: u32,
    val3: u32,
    val4: u32,
) -> ConfigResult {
    match op {
        VmblockInitOp::SetCpusInts => set_cpus_ints(vm, val1, val2),
        VmblockInitOp::SetFences => set_fences(vm, val1 as i32, val2 as i32),
        VmblockInitOp::SetPrioTrapmask => set_prio_trapmask(vm, val1, val2),
        VmblockInitOp::SetPmapType => set_pmap_type(
            vm,
            AsidEntry::from_raw(((val2 as u64) << 32) | val1 as u64),
            OffsetConfig(val3),
            val4 as u8,
        ),
        VmblockInitOp::MapPhysIntr => {
            // C: arg1 = virt_int, arg2 = PhysintConfig
            let cfg = PhysintConfig(val2);
            map_phys_intr(vm, val1, cfg.physint())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_set_cpus_ints() {
        let mut vm = VmBlock::new(1);
        assert_eq!(set_cpus_ints(&mut vm, 4, 320), ConfigResult::Ok);
        assert_eq!(vm.max_cpus, 4);
        assert_eq!(vm.num_ints, 320);
    }

    #[test]
    fn test_set_cpus_ints_zero() {
        let mut vm = VmBlock::new(1);
        assert_eq!(set_cpus_ints(&mut vm, 0, 0), ConfigResult::BadArg);
    }

    #[test]
    fn test_set_fences() {
        let mut vm = VmBlock::new(1);
        assert_eq!(set_fences(&mut vm, 0, 0x1000), ConfigResult::Ok);
        assert_eq!(vm.fence_lo, 0);
        assert_eq!(vm.fence_hi, 0x1000);
    }

    #[test]
    fn test_set_prio_trapmask() {
        let mut vm = VmBlock::new(1);
        assert_eq!(set_prio_trapmask(&mut vm, 10, 0xFFFF), ConfigResult::Ok);
        assert_eq!(vm.bestprio, 10);
        assert_eq!(vm.trapmask, 0xFFFF);
    }

    #[test]
    fn test_map_phys_intr() {
        let mut vm = VmBlock::new(1);
        assert_eq!(map_phys_intr(&mut vm, 5, 42), ConfigResult::Ok);
        assert_eq!(vm.int_v2p[5], 42);
    }

    #[test]
    fn test_map_phys_intr_out_of_range() {
        let mut vm = VmBlock::new(1);
        assert_eq!(map_phys_intr(&mut vm, 1000, 42), ConfigResult::BadArg);
    }

    #[test]
    fn test_dispatch() {
        let mut vm = VmBlock::new(1);
        assert_eq!(
            vmblock_init_dispatch(&mut vm, VmblockInitOp::SetCpusInts, 4, 32, 0, 0),
            ConfigResult::Ok
        );
        assert_eq!(vm.max_cpus, 4);
        assert_eq!(vm.num_ints, 32);
    }
}
