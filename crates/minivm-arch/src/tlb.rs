/*
 * Copyright (c) Qualcomm Technologies, Inc. and/or its subsidiaries.
 * SPDX-License-Identifier: BSD-3-Clause-Clear
 */

//! TLB entry format and operations.
//!

use minivm_types::consts;
use minivm_types::pmap::PAGE_BITS;

/// TLB entry format (64-bit).
///
/// Low 32 bits: PPD (page properties descriptor)
///   - `ppn` + cache attrs + permissions
///
/// High 32 bits:
///   - `vpn:20 | asid:7 | abits:2 | unused:1 | global:1 | valid:1`
///
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[repr(transparent)]
pub struct TlbEntry(pub u64);

impl TlbEntry {
    pub const VALID_BIT: u32 = 63;
    pub const GLOBAL_BIT: u32 = 62;

    /// Create an invalid (zero) TLB entry.
    pub const fn invalid() -> Self {
        Self(0)
    }

    /// Check if the entry is valid.
    pub const fn is_valid(self) -> bool {
        self.0 & (1u64 << Self::VALID_BIT) != 0
    }

    /// Check if the entry is global.
    pub const fn is_global(self) -> bool {
        self.0 & (1u64 << Self::GLOBAL_BIT) != 0
    }

    /// Get the PPD (low 32 bits).
    pub const fn ppd(self) -> u32 {
        self.0 as u32
    }

    /// Get the high 32 bits (VPN + ASID + flags).
    pub const fn hi(self) -> u32 {
        (self.0 >> 32) as u32
    }

    /// Extract the page size encoding from the PPD.
    pub const fn size(self) -> u8 {
        let ppd = self.ppd();
        let c_bits = ppd >> consts::TLB_ENTRY_C_BITS;
        // size is encoded in bits [27:24] of PPD
        (c_bits & 0xF) as u8
    }

    /// Extract the permission bits (XWRU).
    pub const fn perm(self) -> u8 {
        let ppd = self.ppd();
        ((ppd >> 28) & 0xF) as u8
    }

    /// Extract the ASID from the high word.
    pub const fn asid(self) -> u8 {
        let hi = self.hi();
        ((hi >> 20) & 0x7F) as u8
    }

    /// Build a TLB entry from components.
    pub const fn build(
        vpn: u32,
        asid: u8,
        ppn: u32,
        size: u8,
        perm: u8,
        cache: u8,
        global: bool,
    ) -> Self {
        let ppd = (ppn & 0x00FFFFFF)
			| ((cache as u32 & 0xF) << consts::TLB_ENTRY_C_BITS)
			| ((size as u32 & 0xF) << (consts::TLB_ENTRY_C_BITS + 4))  // size is 4 bits above cache
			| ((perm as u32 & 0xF) << 28);

        let hi = (vpn & 0x000FFFFF)
            | (((asid as u32) & 0x7F) << 20)
            | if global { 1u32 << 30 } else { 0 }
            | (1u32 << 31); // valid bit in high word

        Self(((hi as u64) << 32) | (ppd as u64))
    }

    /// Get the page number from the PPD.
    pub const fn ppn(self) -> u32 {
        self.ppd() & 0x00FFFFFF
    }

    /// Get the base physical address from the entry.
    pub const fn base_pa(self) -> u64 {
        (self.ppn() as u64) << PAGE_BITS
    }
}

/// Insert a TLB entry at the given index.
///
/// Uses `tlbw(Rss, Rd)` — the 64-bit entry is composed from two GPRs
/// via `combine` into an explicit register pair.
#[inline(always)]
pub fn tlb_insert(index: u32, entry: TlbEntry) {
    #[cfg(target_arch = "hexagon")]
    unsafe {
        let lo = entry.0 as u32;
        let hi = (entry.0 >> 32) as u32;
        core::arch::asm!(
            "r7:6 = combine({hi}, {lo})",
            "{{ tlbw(r7:6, {idx}) }}",
            "isync",
            lo = in(reg) lo,
            hi = in(reg) hi,
            idx = in(reg) index,
            out("r6") _,
            out("r7") _,
            options(nostack),
        );
    }
    #[cfg(not(target_arch = "hexagon"))]
    {
        let _ = (index, entry);
    }
}

/// Read TLB entry at the given index.
///
/// Uses `Rdd = tlbr(Rs)` — the 64-bit result from an explicit register
/// pair is decomposed into two GPRs.
#[inline(always)]
pub fn tlb_read(index: u32) -> TlbEntry {
    #[cfg(target_arch = "hexagon")]
    unsafe {
        let lo: u32;
        let hi: u32;
        core::arch::asm!(
            "r7:6 = tlbr({idx})",
            "{lo} = r6",
            "{hi} = r7",
            idx = in(reg) index,
            lo = out(reg) lo,
            hi = out(reg) hi,
            out("r6") _,
            out("r7") _,
            options(nomem, nostack),
        );
        TlbEntry(((hi as u64) << 32) | (lo as u64))
    }
    #[cfg(not(target_arch = "hexagon"))]
    {
        let _ = index;
        TlbEntry::invalid()
    }
}

/// Invalidate all TLB entries matching the given ASID.
#[inline(always)]
pub fn tlb_invalidate_asid(asid: u8) {
    #[cfg(target_arch = "hexagon")]
    unsafe {
        core::arch::asm!(
            "tlbinvasid({asid})",
            asid = in(reg) asid as u32,
            options(nomem, nostack),
        );
    }
    #[cfg(not(target_arch = "hexagon"))]
    {
        let _ = asid;
    }
}

/// Invalidate all TLB entries.
#[inline(always)]
pub fn tlb_invalidate_all() {
    // On Hexagon, this is done by writing to all entries or using ctlbw
    // For now, invalidate ASID 0 as a placeholder for full implementation
    #[cfg(target_arch = "hexagon")]
    unsafe {
        core::arch::asm!("tlbinvasid({val})", val = in(reg) 0u32, options(nomem, nostack));
    }
}
