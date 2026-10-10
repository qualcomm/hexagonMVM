/*
 * Copyright (c) Qualcomm Technologies, Inc. and/or its subsidiaries.
 * SPDX-License-Identifier: BSD-3-Clause-Clear
 */

//! Boot sequence.
//!
//! The boot sequence initializes all kernel subsystems in the correct
//! order and then creates the first boot VM and thread.

use minivm_types::pmap::CacheAttr;

/// Boot thread initial cache policy (L1 writeback, L2 cacheable).
pub const BOOT_CACHE_POLICY: CacheAttr = CacheAttr::L1WB_L2C;

/// Boot thread initial GPUGP register value.
pub const BOOT_THREAD_GPUGP: u32 = 0;

/// Boot thread initial USR register value (v65+: prefetch enable bits).
pub const BOOT_THREAD_USR: u32 = 0x0005_7c00;

/// Boot thread initial CCR (cache control register) bits.
/// Matches C `BOOT_THREAD_CCR = 0x00170000`.
pub const BOOT_THREAD_CCR: u32 = 0x0017_0000;

/// Device page offset calculation.
///
/// `ssbase`: subsystem base address.
///
/// Returns the device page offset in 4K pages.
pub const fn device_page_offset(ssbase: u32) -> u32 {
    ssbase >> 12
}

/// QDSP6SS public-to-private offset (in bytes).
pub const QDSP6SS_PUB_PRIV_OFFSET: u32 = 0x0008_0000;

/// Calculate private device page offset.
pub const fn private_device_offset(ssbase: u32) -> u32 {
    device_page_offset(ssbase + QDSP6SS_PUB_PRIV_OFFSET)
}

/// Initialization phases, in order.
///
/// The init system processes these phases sequentially.
/// Each phase initializes one kernel subsystem.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InitPhase {
    /// Initialize kernel globals (first).
    KernelGlobals,
    /// Initialize temporary mapping support.
    TmpMap,
    /// Initialize L2 cache.
    L2Cache,
    /// Copy kernel to TCM (if available).
    TcmCopy,
    /// Initialize tracing.
    Trace,
    /// Initialize run list.
    RunList,
    /// Initialize ready list.
    ReadyList,
    /// Initialize low-priority management.
    LowPrio,
    /// Initialize futex subsystem.
    Futex,
    /// Initialize interrupt configuration.
    IntConfig,
    /// Initialize thread subsystem.
    Thread,
    /// Initialize ASID table.
    AsidTable,
    /// Initialize timer.
    Timer,
    /// Initialize HVX coprocessor.
    Hvx,
    /// Initialize memory allocator.
    MemAlloc,
    /// Initialize semihosting.
    Semihosting,
    /// Boot sequence complete.
    Complete,
}

/// Total number of initialization phases.
pub const NUM_INIT_PHASES: usize = 17;

/// Get the ordered list of initialization phases.
pub fn init_phases() -> &'static [InitPhase] {
    &[
        InitPhase::KernelGlobals,
        InitPhase::TmpMap,
        InitPhase::L2Cache,
        InitPhase::TcmCopy,
        InitPhase::Trace,
        InitPhase::RunList,
        InitPhase::ReadyList,
        InitPhase::LowPrio,
        InitPhase::Futex,
        InitPhase::IntConfig,
        InitPhase::Thread,
        InitPhase::AsidTable,
        InitPhase::Timer,
        InitPhase::Hvx,
        InitPhase::MemAlloc,
        InitPhase::Semihosting,
        InitPhase::Complete,
    ]
}

/// Boot thread parameters (from assembly to Rust entry).
#[derive(Clone, Copy, Debug)]
pub struct BootParams {
    /// Multicore shift for kernel image cloning.
    pub multicore_shift: u32,
    /// Subsystem base address.
    pub ssbase: u32,
    /// Last TLB index (from hardware).
    pub last_tlb_index: u32,
    /// Total TLB size.
    pub tlb_size: u32,
    /// TCM offset (0 if no TCM).
    pub tcm_offset: u32,
}

/// Memory layout computed from boot parameters.
#[derive(Clone, Copy, Debug)]
pub struct MemoryLayout {
    /// Heap start address.
    pub heap_start: u32,
    /// Heap size in bytes.
    pub heap_size: u32,
    /// Stack top address.
    pub stack_top: u32,
    /// Stack size in bytes.
    pub stack_size: u32,
    /// Allocator heap start.
    pub alloc_heap_start: u32,
    /// Allocator heap size.
    pub alloc_heap_size: u32,
}

/// Calculate memory layout from end-of-kernel address.
///
/// `kernel_end`: address of the end of the kernel image.
/// `heap_size`: desired heap size.
/// `stack_size`: desired stack size.
/// `alloc_heap_size`: desired allocator heap size.
pub fn compute_memory_layout(
    kernel_end: u32,
    heap_size: u32,
    stack_size: u32,
    alloc_heap_size: u32,
) -> MemoryLayout {
    let heap_start = (kernel_end + 15) & !15; // 16-byte align
    let stack_top = (heap_start + heap_size + 15) & !15;
    let alloc_start = (stack_top + stack_size + 15) & !15;

    MemoryLayout {
        heap_start,
        heap_size,
        stack_top,
        stack_size,
        alloc_heap_start: alloc_start,
        alloc_heap_size,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_device_page_offset() {
        assert_eq!(device_page_offset(0x0B00_0000), 0x0B000);
    }

    #[test]
    fn test_private_device_offset() {
        let ssbase = 0x0B00_0000;
        assert_eq!(
            private_device_offset(ssbase),
            device_page_offset(ssbase + QDSP6SS_PUB_PRIV_OFFSET)
        );
    }

    #[test]
    fn test_init_phase_order() {
        let phases = init_phases();
        assert_eq!(phases.len(), NUM_INIT_PHASES);
        assert_eq!(phases[0], InitPhase::KernelGlobals);
        assert_eq!(phases[NUM_INIT_PHASES - 1], InitPhase::Complete);
    }

    #[test]
    fn test_memory_layout() {
        let layout = compute_memory_layout(0xFF01_0000, 0x1000, 0x1000, 0x1000);
        assert_eq!(layout.heap_start, 0xFF01_0000); // already aligned
        assert_eq!(layout.heap_size, 0x1000);
        assert_eq!(layout.stack_top, 0xFF01_1000);
        assert_eq!(layout.stack_size, 0x1000);
        assert_eq!(layout.alloc_heap_start, 0xFF01_2000);
        assert_eq!(layout.alloc_heap_size, 0x1000);
    }

    #[test]
    fn test_memory_layout_alignment() {
        // Unaligned kernel_end should be rounded up
        let layout = compute_memory_layout(0xFF01_0001, 0x1000, 0x1000, 0x1000);
        assert_eq!(layout.heap_start, 0xFF01_0010); // aligned to 16
    }

    #[test]
    fn test_boot_constants() {
        assert_eq!(BOOT_CACHE_POLICY, CacheAttr::L1WB_L2C);
        assert_eq!(BOOT_THREAD_GPUGP, 0);
        assert_eq!(BOOT_THREAD_CCR, 0x0017_0000);
        assert_eq!(BOOT_THREAD_USR, 0x0005_7c00);
    }
}
