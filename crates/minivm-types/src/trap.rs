/*
 * Copyright (c) Qualcomm Technologies, Inc. and/or its subsidiaries.
 * SPDX-License-Identifier: BSD-3-Clause-Clear
 */

//! Trap numbers for kernel (trap0) and VM (trap1) hypercalls.
//!

/// Kernel trap numbers (trap0 from guest OS → kernel).
pub mod kernel {
    pub const THREAD_ID: u32 = 1;
    pub const FUTEX_WAIT: u32 = 2;
    pub const FUTEX_WAKE: u32 = 3;
    pub const THREAD_CREATE: u32 = 4;
    pub const THREAD_STOP: u32 = 5;
    pub const CPUTIME: u32 = 6;
    pub const REGISTER_FASTINT: u32 = 8;
    pub const PRIO_SET: u32 = 9;
    pub const PRIO_GET: u32 = 10;
    pub const INTWAIT: u32 = 11;
    pub const YIELD: u32 = 12;
    pub const INTPOOL_CONFIG: u32 = 14;
    pub const INTPOOL_WAIT: u32 = 15;
    pub const GET_PCYCLES: u32 = 16;
    pub const SET_TID: u32 = 18;
    pub const GET_TID: u32 = 19;
    pub const FUTEX_LOCK_PI: u32 = 20;
    pub const FUTEX_UNLOCK_PI: u32 = 21;
    pub const TIMEROP: u32 = 22;
    pub const SOFT_NMI: u32 = 23;
    pub const TLBOP: u32 = 24;
    pub const THREAD_STATE: u32 = 25;
    pub const INFO: u32 = 26;
    pub const WAITCYCLES: u32 = 27;
    pub const VMOP: u32 = 28;
    pub const PMUCTRL: u32 = 29;
    pub const CONFIG: u32 = 30;
    pub const HWCONFIG: u32 = 31;
}

/// VM trap numbers (trap1 from guest → VMM).
///
/// These are the trap numbers used in the VM trap dispatch table.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum VmTrap {
    Version = 0,
    Return = 1,
    SetVec = 2,
    SetIe = 3,
    GetIe = 4,
    IntOp = 5,
    ClrMap = 10,
    NewMap = 11,
    CacheCtl = 13,
    GetPcycles = 14,
    SetPcycles = 15,
    Wait = 16,
    Yield = 17,
    Start = 18,
    Stop = 19,
    VmPid = 20,
    SetRegs = 21,
    GetRegs = 22,
    TimerOp = 24,
    PmuCtrl = 25,
    Info = 26,
}

/// Maximum number of VM trap entries in the dispatch table.
pub const VM_TRAP_TABLE_SIZE: usize = 32;

impl VmTrap {
    /// Try to convert a raw trap number to a VmTrap variant.
    pub const fn from_raw(val: u8) -> Option<Self> {
        match val {
            0 => Some(Self::Version),
            1 => Some(Self::Return),
            2 => Some(Self::SetVec),
            3 => Some(Self::SetIe),
            4 => Some(Self::GetIe),
            5 => Some(Self::IntOp),
            10 => Some(Self::ClrMap),
            11 => Some(Self::NewMap),
            13 => Some(Self::CacheCtl),
            14 => Some(Self::GetPcycles),
            15 => Some(Self::SetPcycles),
            16 => Some(Self::Wait),
            17 => Some(Self::Yield),
            18 => Some(Self::Start),
            19 => Some(Self::Stop),
            20 => Some(Self::VmPid),
            21 => Some(Self::SetRegs),
            22 => Some(Self::GetRegs),
            24 => Some(Self::TimerOp),
            25 => Some(Self::PmuCtrl),
            26 => Some(Self::Info),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_vm_trap_round_trip() {
        let traps = [
            VmTrap::Version,
            VmTrap::Return,
            VmTrap::SetVec,
            VmTrap::SetIe,
            VmTrap::GetIe,
            VmTrap::IntOp,
            VmTrap::ClrMap,
            VmTrap::NewMap,
            VmTrap::CacheCtl,
            VmTrap::GetPcycles,
            VmTrap::SetPcycles,
            VmTrap::Wait,
            VmTrap::Yield,
            VmTrap::Start,
            VmTrap::Stop,
            VmTrap::VmPid,
            VmTrap::SetRegs,
            VmTrap::GetRegs,
            VmTrap::TimerOp,
            VmTrap::PmuCtrl,
            VmTrap::Info,
        ];
        for trap in traps {
            assert_eq!(VmTrap::from_raw(trap as u8), Some(trap));
        }
    }

    #[test]
    fn test_vm_trap_invalid() {
        assert_eq!(VmTrap::from_raw(6), None);
        assert_eq!(VmTrap::from_raw(31), None);
    }

    #[test]
    fn test_vm_trap_version() {
        assert_eq!(VmTrap::from_raw(0), Some(VmTrap::Version));
    }

    #[test]
    fn test_kernel_trap_values() {
        assert_eq!(kernel::THREAD_ID, 1);
        assert_eq!(kernel::VMOP, 28);
        assert_eq!(kernel::HWCONFIG, 31);
    }
}
