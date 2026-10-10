/*
 * Copyright (c) Qualcomm Technologies, Inc. and/or its subsidiaries.
 * SPDX-License-Identifier: BSD-3-Clause-Clear
 */

//! Timer trap handler and timeout management.
//!
//! The timer subsystem manages per-thread timeouts via a binary search
//! tree and provides a guest-facing API for time queries and timeout
//! operations.

use crate::tree::{TimeoutTree, TreeIdx, TreeNode, IDX_NONE};
use minivm_types::consts::MINIVM_TIME_GUESTINT;
use minivm_types::timer::TimerOp;

/// Sentinel: timeout disabled / no timeout set.
pub const TIME_BIGBANG: u64 = 0;
/// Sentinel: far future / no hardware timeout needed.
pub const TIME_FOREVER: u64 = !0u64;

/// Default tick granularity (minimum meaningful interval).
pub const TICK_GRANULARITY: u64 = 4;

/// Default nanoseconds per tick (architecture-dependent, v65+).
pub const NSEC_PER_TICK: u64 = 52;
/// Default tick frequency in Hz.
pub const TICK_REALFREQ: u64 = 19_200_000;
/// Nanosecond-scale frequency.
pub const NSEC_FREQ: u64 = TICK_REALFREQ * NSEC_PER_TICK;

/// Timer state for the system.
pub struct TimerState {
    /// Next hardware interrupt time (ticks).
    pub next_ticks: u64,
    /// Last hardware interrupt time (ticks).
    pub last_ticks: u64,
    /// Timeout tree (BST of thread timeouts).
    pub timeouts: TimeoutTree,
}

impl Default for TimerState {
    fn default() -> Self {
        Self::new()
    }
}

impl TimerState {
    pub const fn new() -> Self {
        Self {
            next_ticks: TIME_FOREVER,
            last_ticks: TIME_BIGBANG,
            timeouts: TimeoutTree::new(),
        }
    }

    /// Process a timer trap from a guest thread.
    ///
    /// `op`: the timer operation requested
    /// `arg`: 64-bit argument (nanoseconds for set/delta, unused otherwise)
    /// `timeout_key`: the thread's current timeout key (from tree node)
    /// `nodes`: the tree node storage
    /// `thread_idx`: the thread's tree node index
    ///
    /// Returns (result_u64, new_timeout_key, needs_hw_update).
    pub fn timer_trap(
        &mut self,
        op: TimerOp,
        arg: u64,
        timeout_key: u64,
        nodes: &mut [TreeNode],
        thread_idx: TreeIdx,
    ) -> TimerTrapResult {
        match op {
            TimerOp::GetFreq => TimerTrapResult::value(NSEC_FREQ),
            TimerOp::GetResolution => TimerTrapResult::value(NSEC_PER_TICK),
            TimerOp::GetTime => {
                let ticks = self.last_ticks;
                TimerTrapResult::value(ticks2ns(ticks))
            }
            TimerOp::GetTimeout => TimerTrapResult::value(ticks2ns(timeout_key)),
            TimerOp::SetTimeout => {
                let timeout_tick = if arg == TIME_FOREVER {
                    TIME_BIGBANG
                } else {
                    ns2ticks(arg)
                };
                self.set_timeout_tick(timeout_tick, timeout_key, nodes, thread_idx)
            }
            TimerOp::DeltaTimeout => {
                if arg == TIME_FOREVER {
                    self.set_timeout_tick(TIME_BIGBANG, timeout_key, nodes, thread_idx)
                } else {
                    let delta_ticks = ns2ticks(arg).max(TICK_GRANULARITY);
                    let timeout_tick = self.last_ticks.saturating_add(delta_ticks);
                    self.set_timeout_tick(timeout_tick, timeout_key, nodes, thread_idx)
                }
            }
        }
    }

    /// Set a thread's timeout to an absolute tick value.
    ///
    /// Returns the result with the new timeout value and whether HW needs update.
    fn set_timeout_tick(
        &mut self,
        timeout_tick: u64,
        old_key: u64,
        nodes: &mut [TreeNode],
        thread_idx: TreeIdx,
    ) -> TimerTrapResult {
        let timeout_tick = if timeout_tick != TIME_BIGBANG && timeout_tick <= self.last_ticks {
            TIME_BIGBANG
        } else {
            timeout_tick
        };

        // Remove existing timeout if active
        if old_key != TIME_BIGBANG {
            self.timeouts.remove(nodes, thread_idx, old_key);
        }

        // Set new timeout
        if timeout_tick != TIME_BIGBANG {
            self.timeouts.add(nodes, thread_idx, timeout_tick);
            let needs_hw = timeout_tick < self.next_ticks;
            if needs_hw {
                self.next_ticks = timeout_tick;
            }
            TimerTrapResult {
                value: ticks2ns(timeout_tick),
                new_timeout_key: timeout_tick,
                needs_hw_update: needs_hw,
            }
        } else {
            TimerTrapResult {
                value: ticks2ns(TIME_BIGBANG),
                new_timeout_key: TIME_BIGBANG,
                needs_hw_update: false,
            }
        }
    }

    /// Handle a timer interrupt: process all expired timeouts.
    ///
    /// `now_ticks`: current hardware timer value (+ granularity buffer).
    ///
    /// Returns a list of thread indices whose timeouts expired and need
    /// `MINIVM_TIME_GUESTINT` posted.
    pub fn handle_interrupt(
        &mut self,
        now_ticks: u64,
        nodes: &mut [TreeNode],
        expired: &mut impl FnMut(TreeIdx),
    ) {
        let (mut le, gt) = self.timeouts.bisect(nodes, now_ticks);

        // Find next timeout from remaining tree
        let min_idx = gt.min(nodes);
        self.next_ticks = if min_idx != IDX_NONE {
            nodes[min_idx as usize].key
        } else {
            TIME_FOREVER
        };

        // Replace timeout tree with pending timeouts only
        self.timeouts = gt;

        // Process expired timeouts
        le.collect_and_clear(nodes, expired);
    }

    /// Update last known time.
    pub fn update_time(&mut self, now_ticks: u64) {
        if now_ticks > self.last_ticks {
            self.last_ticks = now_ticks;
        }
    }
}

/// Result of a timer trap operation.
pub struct TimerTrapResult {
    /// Return value (nanoseconds).
    pub value: u64,
    /// New timeout key for the thread (0 = disabled).
    pub new_timeout_key: u64,
    /// Whether the hardware timer needs to be rescheduled.
    pub needs_hw_update: bool,
}

impl TimerTrapResult {
    fn value(v: u64) -> Self {
        Self {
            value: v,
            new_timeout_key: TIME_BIGBANG,
            needs_hw_update: false,
        }
    }
}

/// Guest interrupt number posted on timeout expiry.
pub const TIME_GUESTINT: u32 = MINIVM_TIME_GUESTINT;

/// Convert ticks to nanoseconds.
#[inline]
pub fn ticks2ns(ticks: u64) -> u64 {
    ticks.wrapping_mul(NSEC_PER_TICK)
}

/// Convert nanoseconds to ticks.
#[inline]
pub fn ns2ticks(ns: u64) -> u64 {
    ns / NSEC_PER_TICK
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tree::TreeNode;
    use alloc::vec::Vec;

    fn make_nodes(n: usize) -> Vec<TreeNode> {
        let mut v = Vec::new();
        v.resize(n, TreeNode::default());
        v
    }

    #[test]
    fn test_timer_get_freq() {
        let mut state = TimerState::new();
        let mut nodes = make_nodes(4);
        let r = state.timer_trap(TimerOp::GetFreq, 0, TIME_BIGBANG, &mut nodes, 1);
        assert_eq!(r.value, NSEC_FREQ);
    }

    #[test]
    fn test_timer_get_resolution() {
        let mut state = TimerState::new();
        let mut nodes = make_nodes(4);
        let r = state.timer_trap(TimerOp::GetResolution, 0, TIME_BIGBANG, &mut nodes, 1);
        assert_eq!(r.value, NSEC_PER_TICK);
    }

    #[test]
    fn test_timer_get_time() {
        let mut state = TimerState::new();
        let mut nodes = make_nodes(4);
        state.last_ticks = 1000;
        let r = state.timer_trap(TimerOp::GetTime, 0, TIME_BIGBANG, &mut nodes, 1);
        assert_eq!(r.value, 1000 * NSEC_PER_TICK);
    }

    #[test]
    fn test_timer_set_timeout() {
        let mut state = TimerState::new();
        let mut nodes = make_nodes(4);
        state.last_ticks = 100;

        // Set timeout to 10000 ns → 192 ticks
        let ns = 10000u64;
        let r = state.timer_trap(TimerOp::SetTimeout, ns, TIME_BIGBANG, &mut nodes, 1);
        let expected_ticks = ns / NSEC_PER_TICK;
        assert_eq!(r.new_timeout_key, expected_ticks);
        assert!(r.needs_hw_update); // Should need HW update
    }

    #[test]
    fn test_timer_set_timeout_in_past() {
        let mut state = TimerState::new();
        let mut nodes = make_nodes(4);
        state.last_ticks = 1000;

        // Set timeout in the past → should be disabled
        let r = state.timer_trap(TimerOp::SetTimeout, 50, TIME_BIGBANG, &mut nodes, 1);
        assert_eq!(r.new_timeout_key, TIME_BIGBANG);
    }

    #[test]
    fn test_timer_delta_timeout() {
        let mut state = TimerState::new();
        let mut nodes = make_nodes(4);
        state.last_ticks = 100;

        // Delta timeout of 1000 ns
        let r = state.timer_trap(TimerOp::DeltaTimeout, 1000, TIME_BIGBANG, &mut nodes, 1);
        let delta_ticks = (1000u64 / NSEC_PER_TICK).max(TICK_GRANULARITY);
        assert_eq!(r.new_timeout_key, 100 + delta_ticks);
    }

    #[test]
    fn test_timer_cancel_timeout() {
        let mut state = TimerState::new();
        let mut nodes = make_nodes(4);
        state.last_ticks = 100;

        // Set a timeout
        let r = state.timer_trap(TimerOp::SetTimeout, 100000, TIME_BIGBANG, &mut nodes, 1);
        let old_key = r.new_timeout_key;
        assert_ne!(old_key, TIME_BIGBANG);

        // Cancel with TIME_FOREVER
        let r = state.timer_trap(TimerOp::SetTimeout, TIME_FOREVER, old_key, &mut nodes, 1);
        assert_eq!(r.new_timeout_key, TIME_BIGBANG);
    }

    #[test]
    fn test_timer_interrupt() {
        let mut state = TimerState::new();
        let mut nodes = make_nodes(8);

        // Add some timeouts
        state.timeouts.add(&mut nodes, 1, 100);
        state.timeouts.add(&mut nodes, 2, 200);
        state.timeouts.add(&mut nodes, 3, 300);

        // Fire interrupt at time 250 → 1 and 2 expire
        let mut expired = Vec::new();
        state.handle_interrupt(250, &mut nodes, &mut |idx| expired.push(idx));

        assert_eq!(expired.len(), 2);
        assert!(expired.contains(&1));
        assert!(expired.contains(&2));
        assert_eq!(state.next_ticks, 300);
    }

    #[test]
    fn test_timer_interrupt_all_expire() {
        let mut state = TimerState::new();
        let mut nodes = make_nodes(8);

        state.timeouts.add(&mut nodes, 1, 100);
        state.timeouts.add(&mut nodes, 2, 200);

        let mut expired = Vec::new();
        state.handle_interrupt(500, &mut nodes, &mut |idx| expired.push(idx));

        assert_eq!(expired.len(), 2);
        assert_eq!(state.next_ticks, TIME_FOREVER);
    }

    #[test]
    fn test_conversions() {
        assert_eq!(ticks2ns(100), 100 * NSEC_PER_TICK);
        assert_eq!(ns2ticks(5200), 100);
        assert_eq!(ns2ticks(0), 0);
    }
}
