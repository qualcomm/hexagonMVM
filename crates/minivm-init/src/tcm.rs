/*
 * Copyright (c) Qualcomm Technologies, Inc. and/or its subsidiaries.
 * SPDX-License-Identifier: BSD-3-Clause-Clear
 */

//! TCM initialization.
//!
//! TCM (Tightly Coupled Memory) is fast on-chip SRAM that can hold
//! the kernel image for reduced latency. The init code copies the
//! kernel from physical memory to TCM and updates the TLB to point
//! to the new location.

/// Maximum TCM copy size for crash debug (bytes).
pub const CRASH_TCM_MAX_COPY: u32 = 512 * 1024; // 512 KB

/// Calculate the page size encoding for a given TCM size.
///
/// `tcm_size`: TCM size in 4K pages.
///
/// Returns the page size encoding: (32 - clz(size)) >> 1.
/// This yields the smallest power-of-4 page size that covers the TCM.
pub fn tcm_page_size(tcm_size: u32) -> u32 {
    if tcm_size == 0 {
        return 0;
    }
    let bits = 32 - tcm_size.leading_zeros();
    bits >> 1
}

/// Check if TCM is large enough to hold the kernel.
///
/// `tcm_size`: TCM size in 4K pages.
/// `kernel_npages`: number of kernel pages.
/// `kernel_pagesize`: kernel page size in 4K-page units.
pub fn tcm_can_hold_kernel(tcm_size: u32, kernel_npages: u32, kernel_pagesize: u32) -> bool {
    tcm_size > kernel_npages * kernel_pagesize
}

/// Compute the new physical offset after TCM copy.
///
/// `link_addr`: kernel link address (virtual).
/// `tcm_base`: TCM base in 4K pages.
///
/// Returns the new phys_offset: link_addr - (tcm_base << 12).
pub fn tcm_phys_offset(link_addr: u32, tcm_base: u32) -> i32 {
    link_addr as i32 - ((tcm_base << 12) as i32)
}

/// TCM copy configuration.
#[derive(Clone, Copy, Debug)]
pub struct TcmCopyConfig {
    /// TCM base in 4K pages.
    pub tcm_base: u32,
    /// TCM size in 4K pages.
    pub tcm_size: u32,
    /// Page size encoding for the TCM mapping.
    pub tcm_pgsize: u32,
    /// New physical offset after copy.
    pub new_phys_offset: i32,
}

/// Compute TCM copy configuration.
///
/// Returns None if TCM is not available or too small.
pub fn plan_tcm_copy(
    tcm_base: u32,
    tcm_size: u32,
    kernel_npages: u32,
    kernel_pagesize: u32,
    link_addr: u32,
) -> Option<TcmCopyConfig> {
    if tcm_size == 0 {
        return None;
    }
    if !tcm_can_hold_kernel(tcm_size, kernel_npages, kernel_pagesize) {
        return None;
    }
    Some(TcmCopyConfig {
        tcm_base,
        tcm_size,
        tcm_pgsize: tcm_page_size(tcm_size),
        new_phys_offset: tcm_phys_offset(link_addr, tcm_base),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tcm_page_size() {
        assert_eq!(tcm_page_size(0), 0);
        assert_eq!(tcm_page_size(1), 0); // 4K: 1 bit, 1>>1 = 0
        assert_eq!(tcm_page_size(4), 1); // 16K: 3 bits, 3>>1 = 1
        assert_eq!(tcm_page_size(16), 2); // 64K: 5 bits, 5>>1 = 2
        assert_eq!(tcm_page_size(64), 3); // 256K: 7 bits, 7>>1 = 3
        assert_eq!(tcm_page_size(256), 4); // 1M: 9 bits, 9>>1 = 4
    }

    #[test]
    fn test_tcm_can_hold_kernel() {
        assert!(tcm_can_hold_kernel(256, 4, 1)); // 256 pages > 4 pages
        assert!(!tcm_can_hold_kernel(4, 4, 1)); // 4 pages == 4 pages (not >)
        assert!(!tcm_can_hold_kernel(0, 4, 1));
    }

    #[test]
    fn test_tcm_phys_offset() {
        // link_addr=0xFF000000, tcm_base=0x100 (page 256 = 0x100000)
        let offset = tcm_phys_offset(0xFF00_0000, 0x100);
        assert_eq!(offset, 0xFF00_0000u32 as i32 - 0x0010_0000);
    }

    #[test]
    fn test_plan_tcm_copy_no_tcm() {
        assert!(plan_tcm_copy(0, 0, 4, 1, 0xFF00_0000).is_none());
    }

    #[test]
    fn test_plan_tcm_copy_too_small() {
        assert!(plan_tcm_copy(0x100, 2, 4, 1, 0xFF00_0000).is_none());
    }

    #[test]
    fn test_plan_tcm_copy_ok() {
        let cfg = plan_tcm_copy(0x100, 256, 4, 1, 0xFF00_0000).unwrap();
        assert_eq!(cfg.tcm_base, 0x100);
        assert_eq!(cfg.tcm_size, 256);
        assert_eq!(cfg.tcm_pgsize, 4); // 256 pages → pgsize 4
    }
}
