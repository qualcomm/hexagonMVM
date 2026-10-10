/*
 * Copyright (c) Qualcomm Technologies, Inc. and/or its subsidiaries.
 * SPDX-License-Identifier: BSD-3-Clause-Clear
 */

//! Per-CPU interrupt logic.
//!
//! Per-CPU interrupts (first 16) have three state bits each:
//!   - **pending**: interrupt has been asserted (POST)
//!   - **enabled**: globally enabled (POST auto-enables; GlobEn/GlobDis toggle)
//!   - **local_en**: locally enabled for this CPU (LocEn/LocDis toggle)
//!
//! An interrupt is deliverable when all three bits AND the global IE flag are set.
//! PEEK returns the next pending+globally-enabled interrupt (ignoring local_en).

use minivm_types::consts::PERCPU_INTERRUPTS;

/// Per-CPU interrupt state.
#[derive(Clone, Copy, Debug, Default)]
pub struct CpuIntState {
    pub pending: u16,
    pub enabled: u16,
    pub local_en: u16,
}

impl CpuIntState {
    pub const fn new() -> Self {
        Self {
            pending: 0,
            enabled: 0,
            local_en: 0,
        }
    }

    /// Post (assert) a per-CPU interrupt.
    ///
    /// Sets both the pending and global-enable bits.
    /// Returns true if the interrupt is now deliverable
    /// (pending AND enabled AND locally enabled).
    pub fn post(&mut self, intno: u32) -> bool {
        if intno >= PERCPU_INTERRUPTS {
            return false;
        }
        let mask = 1u16 << intno;
        self.pending |= mask;
        self.enabled |= mask;
        (self.pending & self.enabled & self.local_en) != 0
    }

    /// Clear a per-CPU interrupt's pending bit.
    ///
    /// Returns true if the interrupt was pending before clearing.
    pub fn clear(&mut self, intno: u32) -> bool {
        if intno >= PERCPU_INTERRUPTS {
            return false;
        }
        let mask = 1u16 << intno;
        let was_pending = self.pending & mask != 0;
        self.pending &= !mask;
        was_pending
    }

    /// Enable a per-CPU interrupt globally.
    ///
    /// Returns true if there's now a deliverable interrupt.
    pub fn enable(&mut self, intno: u32) -> bool {
        if intno >= PERCPU_INTERRUPTS {
            return false;
        }
        let mask = 1u16 << intno;
        self.enabled |= mask;
        (self.pending & self.enabled & self.local_en & mask) != 0
    }

    /// Disable a per-CPU interrupt globally.
    pub fn disable(&mut self, intno: u32) {
        if intno >= PERCPU_INTERRUPTS {
            return;
        }
        self.enabled &= !(1u16 << intno);
    }

    /// Enable a per-CPU interrupt locally.
    ///
    /// Returns true if the interrupt is now deliverable.
    pub fn local_enable(&mut self, intno: u32) -> bool {
        if intno >= PERCPU_INTERRUPTS {
            return false;
        }
        let mask = 1u16 << intno;
        self.local_en |= mask;
        (self.pending & self.enabled & mask) != 0
    }

    /// Disable a per-CPU interrupt locally.
    pub fn local_disable(&mut self, intno: u32) {
        if intno >= PERCPU_INTERRUPTS {
            return;
        }
        self.local_en &= !(1u16 << intno);
    }

    /// Get the highest-priority deliverable interrupt (lowest bit index).
    ///
    /// Deliverable = pending AND enabled AND locally enabled.
    /// Returns the interrupt number, or -1 if none.
    /// Clears both the pending and enabled bits (acknowledges the interrupt).
    pub fn get(&mut self) -> i32 {
        let deliverable = self.enabled & self.pending & self.local_en;
        if deliverable == 0 {
            return -1;
        }
        let bit = deliverable.trailing_zeros();
        self.pending &= !(1u16 << bit);
        self.enabled &= !(1u16 << bit);
        bit as i32
    }

    /// Peek at the highest-priority pending+globally-enabled interrupt.
    ///
    /// Returns the interrupt number, or -1 if none.
    /// Does NOT require local_en — shows what would be deliverable if locally enabled.
    pub fn peek(&self) -> i32 {
        let visible = self.enabled & self.pending;
        if visible == 0 {
            return -1;
        }
        visible.trailing_zeros() as i32
    }

    /// Peek at the highest-priority fully deliverable interrupt.
    ///
    /// Requires pending AND enabled AND locally enabled.
    /// Used internally for delivery checks (try_deliver_interrupt).
    pub fn peek_deliverable(&self) -> i32 {
        let deliverable = self.enabled & self.pending & self.local_en;
        if deliverable == 0 {
            return -1;
        }
        deliverable.trailing_zeros() as i32
    }

    /// Get the status of a specific interrupt.
    ///
    /// Returns: bit 0 = pending, bit 1 = local enable, bit 2 = global enable.
    pub fn status(&self, intno: u32) -> u32 {
        if intno >= PERCPU_INTERRUPTS {
            return 0;
        }
        let mut ret = 0u32;
        ret |= ((self.pending >> intno) & 1) as u32;
        ret |= (((self.local_en >> intno) & 1) as u32) << 1;
        ret |= (((self.enabled >> intno) & 1) as u32) << 2;
        ret
    }

    /// Check if any interrupt is fully deliverable.
    pub fn any_deliverable(&self) -> bool {
        (self.pending & self.enabled & self.local_en) != 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cpuint_initial() {
        let s = CpuIntState::new();
        assert_eq!(s.pending, 0);
        assert_eq!(s.enabled, 0);
        assert_eq!(s.local_en, 0);
        assert_eq!(s.peek(), -1);
        assert!(!s.any_deliverable());
    }

    #[test]
    fn test_cpuint_post_not_locally_enabled() {
        let mut s = CpuIntState::new();
        let deliverable = s.post(3);
        assert!(!deliverable); // Not locally enabled
        assert_eq!(s.pending, 1 << 3);
        assert_eq!(s.enabled, 1 << 3); // POST auto-enables
                                       // PEEK shows it (pending + enabled, local_en not needed)
        assert_eq!(s.peek(), 3);
        // But not fully deliverable
        assert!(!s.any_deliverable());
    }

    #[test]
    fn test_cpuint_post_locally_enabled() {
        let mut s = CpuIntState::new();
        s.local_enable(3);
        let deliverable = s.post(3);
        assert!(deliverable);
        assert_eq!(s.peek(), 3);
        assert_eq!(s.peek_deliverable(), 3);
    }

    #[test]
    fn test_cpuint_get_requires_local() {
        let mut s = CpuIntState::new();
        s.post(3); // pending + enabled, but not local
        assert_eq!(s.get(), -1); // Not deliverable

        s.local_enable(3);
        assert_eq!(s.get(), 3); // Now deliverable
        assert_eq!(s.get(), -1); // Consumed
    }

    #[test]
    fn test_cpuint_get_priority() {
        let mut s = CpuIntState::new();
        s.local_enable(5);
        s.local_enable(3);
        s.post(5);
        s.post(3);

        // Get should return lowest-numbered first
        assert_eq!(s.get(), 3);
        assert_eq!(s.get(), 5);
        assert_eq!(s.get(), -1);
    }

    #[test]
    fn test_cpuint_clear_returns_was_pending() {
        let mut s = CpuIntState::new();
        s.post(3);
        assert!(s.clear(3)); // Was pending
        assert!(!s.clear(3)); // Not pending anymore
    }

    #[test]
    fn test_cpuint_disable() {
        let mut s = CpuIntState::new();
        s.local_enable(3);
        s.post(3);
        assert!(s.any_deliverable());

        s.disable(3);
        assert!(!s.any_deliverable());
        assert_eq!(s.pending, 1 << 3); // Still pending
    }

    #[test]
    fn test_cpuint_local_disable() {
        let mut s = CpuIntState::new();
        s.local_enable(3);
        s.post(3);
        assert!(s.any_deliverable());

        s.local_disable(3);
        assert!(!s.any_deliverable());
        assert_eq!(s.pending, 1 << 3); // Still pending
    }

    #[test]
    fn test_cpuint_status() {
        let mut s = CpuIntState::new();
        assert_eq!(s.status(3), 0); // Nothing set

        s.post(3);
        // POST sets pending + global enable
        assert_eq!(s.status(3), 5); // pending(1) + global_en(4)

        s.local_enable(3);
        assert_eq!(s.status(3), 7); // pending(1) + local_en(2) + global_en(4)
    }

    #[test]
    fn test_cpuint_locen_triggers_delivery() {
        let mut s = CpuIntState::new();
        s.post(13); // pending + globally enabled
        assert!(!s.any_deliverable()); // Not locally enabled
        assert_eq!(s.status(13), 5); // pending + global_en

        let deliverable = s.local_enable(13);
        assert!(deliverable); // Now deliverable
        assert_eq!(s.status(13), 7); // pending + local + global

        // Consume it
        assert_eq!(s.get(), 13);
        // After get: pending cleared, enabled cleared, local still set
        assert_eq!(s.status(13), 2); // Just local_en
    }

    #[test]
    fn test_cpuint_full_interrupt_flow() {
        // Mimics the test_interrupts.S flow for interrupt 13
        let mut s = CpuIntState::new();

        // POST 13 — sets pending + enabled
        s.post(13);
        assert_eq!(s.status(13), 5); // pending + global_en

        // POST 15
        s.post(15);
        assert_eq!(s.peek(), 13); // Lowest pending+enabled

        // LOCEN 13 — triggers delivery
        assert!(s.local_enable(13));
        assert_eq!(s.get(), 13); // Consume

        // After delivery: pending cleared, enabled cleared
        // Interrupt handler calls GLOBEN to re-enable
        s.enable(13);
        assert_eq!(s.status(13), 6); // local_en(2) + global_en(4)
        assert_eq!(s.peek(), 15); // 15 still pending+enabled

        // LOCDIS 13
        s.local_disable(13);
        assert_eq!(s.status(13), 4); // Just global_en

        // POST 13 again
        s.post(13);
        assert_eq!(s.status(13), 5); // pending + global_en (local cleared)

        // LOCEN 13 — triggers delivery again
        assert!(s.local_enable(13));
        assert_eq!(s.get(), 13);
        s.enable(13); // Handler re-enables

        // CLEAR 15
        assert!(s.clear(15)); // Was pending
        assert_eq!(s.peek(), -1); // None left
    }

    #[test]
    fn test_cpuint_out_of_range() {
        let mut s = CpuIntState::new();
        assert!(!s.post(16));
        assert!(!s.enable(20));
        assert!(!s.local_enable(20));
        assert_eq!(s.status(16), 0);
    }
}
