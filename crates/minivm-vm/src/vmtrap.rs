/*
 * Copyright (c) Qualcomm Technologies, Inc. and/or its subsidiaries.
 * SPDX-License-Identifier: BSD-3-Clause-Clear
 */

//! VM trap dispatch table.
//!
//! The trap dispatch table maps trap1 numbers (0x00-0x1F) to handler
//! actions. Invalid trap numbers result in a "bad trap" error event.

use minivm_types::trap::{VmTrap, VM_TRAP_TABLE_SIZE};

/// Action to take for a VM trap.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TrapAction {
    /// Valid VM trap: dispatch to the appropriate handler.
    Handle(VmTrap),
    /// Invalid trap: generate an error event.
    Bad,
}

/// Dispatch a VM trap number to a TrapAction.
///
/// `trap_num`: the 5-bit trap number from SSR[4:0].
pub fn dispatch(trap_num: u8) -> TrapAction {
    if trap_num >= VM_TRAP_TABLE_SIZE as u8 {
        return TrapAction::Bad;
    }
    match VmTrap::from_raw(trap_num) {
        Some(trap) => TrapAction::Handle(trap),
        None => TrapAction::Bad,
    }
}

/// SSR cause code for "no guest permission" error.
pub const CAUSE_NO_GUEST_PERM: u32 = 0x1B;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dispatch_valid() {
        assert_eq!(dispatch(0), TrapAction::Handle(VmTrap::Version));
        assert_eq!(dispatch(1), TrapAction::Handle(VmTrap::Return));
        assert_eq!(dispatch(5), TrapAction::Handle(VmTrap::IntOp));
        assert_eq!(dispatch(16), TrapAction::Handle(VmTrap::Wait));
        assert_eq!(dispatch(26), TrapAction::Handle(VmTrap::Info));
    }

    #[test]
    fn test_dispatch_invalid() {
        assert_eq!(dispatch(6), TrapAction::Bad);
        assert_eq!(dispatch(7), TrapAction::Bad);
        assert_eq!(dispatch(8), TrapAction::Bad);
        assert_eq!(dispatch(9), TrapAction::Bad);
        assert_eq!(dispatch(12), TrapAction::Bad);
        assert_eq!(dispatch(23), TrapAction::Bad);
        assert_eq!(dispatch(27), TrapAction::Bad);
        assert_eq!(dispatch(31), TrapAction::Bad);
    }

    #[test]
    fn test_dispatch_out_of_range() {
        assert_eq!(dispatch(32), TrapAction::Bad);
        assert_eq!(dispatch(255), TrapAction::Bad);
    }
}
