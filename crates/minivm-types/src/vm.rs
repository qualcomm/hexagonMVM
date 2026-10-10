/*
 * Copyright (c) Qualcomm Technologies, Inc. and/or its subsidiaries.
 * SPDX-License-Identifier: BSD-3-Clause-Clear
 */

//! VM types: ID encoding, thread status, VM version.
//!

/// VM version supported by the hypervisor.
pub const VM_VERSION: u32 = 0x00000800;

// Guest Event Vector Base (GEVB) offsets
/// Reset event vector offset.
pub const RESET_GEVB_OFFSET: u32 = 0;
/// Check event vector offset.
pub const CHECK_GEVB_OFFSET: u32 = 4;
/// Error event vector offset.
pub const ERROR_GEVB_OFFSET: u32 = 8;
/// Debug event vector offset.
pub const DEBUG_GEVB_OFFSET: u32 = 12;
/// Trap0 event vector offset.
pub const TRAP0_GEVB_OFFSET: u32 = 20;
/// Interrupt event vector offset.
pub const INTERRUPT_GEVB_OFFSET: u32 = 28;

// Guest SSR (GSSR) bits
/// GSSR user mode bit position.
pub const GSSR_UM_BIT: u32 = 31;
/// GSSR user mode mask.
pub const GSSR_UM: u32 = 1 << GSSR_UM_BIT;
/// GSSR interrupt enable bit position.
pub const GSSR_IE_BIT: u32 = 30;
/// GSSR interrupt enable mask.
pub const GSSR_IE: u32 = 1 << GSSR_IE_BIT;
/// GSSR single step bit position.
pub const GSSR_SS_BIT: u32 = 29;
/// GSSR single step mask.
pub const GSSR_SS: u32 = 1 << GSSR_SS_BIT;

// VM status word bits (atomic_status_word / vmstatus byte)
/// Deferred work pending bit.
pub const VMSTATUS_VMWORK: u8 = 0x01;
/// Thread kill flag.
pub const VMSTATUS_KILL: u8 = 0x02;
/// Extended registers live.
pub const VMSTATUS_SAVEXT: u8 = 0x40;
/// Interrupt enable (mirrored in vmstatus).
pub const VMSTATUS_IE: u8 = 0x80;

/// Max number of VMs.
pub const MAX_VMS: u32 = 64;

/// Max number of CPUs (virtual).
pub const MAX_CPUS: u32 = 65536;

/// Max number of VM shared interrupts.
pub const MAX_VM_INTS: u32 = 65535;

/// Boot VM index.
pub const BOOTVM_INDEX: u32 = 1;

/// Thread status values.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum ThreadStatus {
    Dead = 0,
    Running = 1,
    Ready = 2,
    Blocked = 3,
    VmWait = 4,
    IntBlocked = 5,
}

impl ThreadStatus {
    pub const fn from_raw(val: u8) -> Option<Self> {
        match val {
            0 => Some(Self::Dead),
            1 => Some(Self::Running),
            2 => Some(Self::Ready),
            3 => Some(Self::Blocked),
            4 => Some(Self::VmWait),
            5 => Some(Self::IntBlocked),
            _ => None,
        }
    }
}

/// VM ID type (composite: vmidx:6 | cpuidx:16 | vint:10).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[repr(transparent)]
pub struct VmId(pub u32);

impl VmId {
    pub const VINT_BITS: u32 = 10;
    pub const CPUIDX_BITS: u32 = 16;
    pub const VMIDX_BITS: u32 = 6;

    pub const VINT_MASK: u32 = (1 << Self::VINT_BITS) - 1;
    pub const CPUIDX_SHIFT: u32 = Self::VINT_BITS;
    pub const CPUIDX_MASK: u32 = ((1 << Self::CPUIDX_BITS) - 1) << Self::CPUIDX_SHIFT;
    pub const VMIDX_SHIFT: u32 = Self::CPUIDX_SHIFT + Self::CPUIDX_BITS;
    pub const VMIDX_MASK: u32 = ((1 << Self::VMIDX_BITS) - 1) << Self::VMIDX_SHIFT;

    pub const fn new(vmidx: u8, cpuidx: u16, vint: u16) -> Self {
        Self(
            ((vint as u32) & Self::VINT_MASK)
                | (((cpuidx as u32) << Self::CPUIDX_SHIFT) & Self::CPUIDX_MASK)
                | (((vmidx as u32) << Self::VMIDX_SHIFT) & Self::VMIDX_MASK),
        )
    }

    pub const fn vint(self) -> u16 {
        (self.0 & Self::VINT_MASK) as u16
    }
    pub const fn cpuidx(self) -> u16 {
        ((self.0 & Self::CPUIDX_MASK) >> Self::CPUIDX_SHIFT) as u16
    }
    pub const fn vmidx(self) -> u8 {
        ((self.0 & Self::VMIDX_MASK) >> Self::VMIDX_SHIFT) as u8
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_vm_id_round_trip() {
        let id = VmId::new(0x3F, 0xFFFF, 0x3FF);
        assert_eq!(id.vmidx(), 0x3F);
        assert_eq!(id.cpuidx(), 0xFFFF);
        assert_eq!(id.vint(), 0x3FF);
    }

    #[test]
    fn test_vm_id_fields_independent() {
        let id = VmId::new(1, 0, 0);
        assert_eq!(id.vmidx(), 1);
        assert_eq!(id.cpuidx(), 0);
        assert_eq!(id.vint(), 0);

        let id = VmId::new(0, 42, 0);
        assert_eq!(id.vmidx(), 0);
        assert_eq!(id.cpuidx(), 42);
        assert_eq!(id.vint(), 0);
    }

    #[test]
    fn test_thread_status_round_trip() {
        for i in 0..=5 {
            let s = ThreadStatus::from_raw(i).unwrap();
            assert_eq!(s as u8, i);
        }
        assert!(ThreadStatus::from_raw(6).is_none());
    }

    #[test]
    fn test_bit_layout() {
        // Ensure fields pack into 32 bits
        assert_eq!(VmId::VINT_BITS + VmId::CPUIDX_BITS + VmId::VMIDX_BITS, 32);
    }
}
