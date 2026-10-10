/*
 * Copyright (c) Qualcomm Technologies, Inc. and/or its subsidiaries.
 * SPDX-License-Identifier: BSD-3-Clause-Clear
 */

//! Lowest-priority thread management.
//!
//! Manages `priomask` and `wait_mask` bitmasks that track which hardware
//! threads are running the lowest-priority thread or are in wait mode.
//! When a low-priority thread is identified, its interrupt mask is reduced
//! so that only reschedule interrupts can preempt it.

/// Low-priority tracking state.
pub struct LowPrio {
    /// Bitmask: bit N set means HW thread N is running the lowest-priority thread.
    pub priomask: u32,
    /// Bitmask: bit N set means HW thread N is in wait mode.
    pub wait_mask: u32,
}

impl Default for LowPrio {
    fn default() -> Self {
        Self::new()
    }
}

impl LowPrio {
    pub const fn new() -> Self {
        Self {
            priomask: 0,
            wait_mask: 0,
        }
    }

    /// Clear the priomask and notify the old lowest-priority thread
    /// that it is no longer the worst.
    ///
    /// Returns the hardware thread that was previously lowest (for IMASK update),
    /// or `None` if no thread was marked.
    pub fn raise(&mut self) -> Option<u32> {
        if self.wait_mask != 0 {
            return None;
        }
        let mask = self.priomask;
        if mask == 0 {
            return None;
        }
        self.priomask = 0;
        Some(mask.trailing_zeros())
    }

    /// Mark a hardware thread as the new lowest-priority thread.
    pub fn mark_low(&mut self, hthread: u32) {
        self.priomask |= 1 << hthread;
    }

    /// Unmark a hardware thread from the low-priority set.
    pub fn unmark_low(&mut self, hthread: u32) {
        self.priomask &= !(1 << hthread);
    }

    /// Check if a hardware thread is marked as low priority.
    pub fn is_low(&self, hthread: u32) -> bool {
        (self.priomask & (1 << hthread)) != 0
    }

    /// Mark a hardware thread as waiting.
    pub fn mark_wait(&mut self, hthread: u32) {
        self.wait_mask |= 1 << hthread;
    }

    /// Unmark a hardware thread from wait mode.
    pub fn unmark_wait(&mut self, hthread: u32) {
        self.wait_mask &= !(1 << hthread);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lowprio_initial() {
        let lp = LowPrio::new();
        assert_eq!(lp.priomask, 0);
        assert_eq!(lp.wait_mask, 0);
        assert!(!lp.is_low(0));
    }

    #[test]
    fn test_lowprio_mark_unmark() {
        let mut lp = LowPrio::new();
        lp.mark_low(2);
        assert!(lp.is_low(2));
        assert!(!lp.is_low(1));

        lp.unmark_low(2);
        assert!(!lp.is_low(2));
    }

    #[test]
    fn test_lowprio_raise() {
        let mut lp = LowPrio::new();
        lp.mark_low(3);
        let ht = lp.raise();
        assert_eq!(ht, Some(3));
        assert_eq!(lp.priomask, 0);
    }

    #[test]
    fn test_lowprio_raise_with_wait() {
        let mut lp = LowPrio::new();
        lp.mark_low(3);
        lp.mark_wait(1);
        // Should not raise when threads are waiting
        assert_eq!(lp.raise(), None);
        assert_eq!(lp.priomask, 1 << 3);
    }
}
