/*
 * Copyright (c) Qualcomm Technologies, Inc. and/or its subsidiaries.
 * SPDX-License-Identifier: BSD-3-Clause-Clear
 */

//! Inter-processor interrupt mechanism.
//!
//! IPIs are used to notify a hardware thread that its currently running
//! guest thread has a pending interrupt. This causes a reschedule on
//! the target hardware thread, allowing the interrupt to be delivered.

use minivm_types::consts::RESCHED_INT;

/// Compute the IPI target mask for a given hardware thread.
///
/// Returns the interrupt number and a bitmask suitable for
/// hardware interrupt steering (architecture-specific).
pub fn ipi_target(hthread: u8) -> (u32, u32) {
    (RESCHED_INT, 1u32 << hthread)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ipi_target() {
        let (int, mask) = ipi_target(3);
        assert_eq!(int, RESCHED_INT);
        assert_eq!(mask, 1 << 3);
    }
}
