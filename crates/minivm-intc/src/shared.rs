/*
 * Copyright (c) Qualcomm Technologies, Inc. and/or its subsidiaries.
 * SPDX-License-Identifier: BSD-3-Clause-Clear
 */

//! Shared interrupt logic (multi-CPU delivery).
//!
//! Shared interrupts are stored in per-VM bitmask arrays:
//! - `pending[]`: globally pending interrupt bits
//! - `enable[]`: globally enabled interrupt bits
//! - `percpu_mask[cpu][]`: per-CPU local enable mask
//!
//! An interrupt is deliverable to a CPU if it is pending, globally enabled,
//! and locally enabled for that CPU.

/// Maximum number of shared interrupt words (32 interrupts per word).
/// Supports up to 512 shared interrupts (16 words * 32 bits).
pub const MAX_SHINT_WORDS: usize = 16;

/// Maximum number of virtual CPUs per VM for interrupt masking.
pub const MAX_CPUS: usize = 64;

/// Shared interrupt state for a VM.
pub struct SharedIntState {
    /// Number of shared interrupts configured.
    pub num_ints: u32,
    /// Global pending bitmask.
    pub pending: [u32; MAX_SHINT_WORDS],
    /// Global enable bitmask.
    pub enable: [u32; MAX_SHINT_WORDS],
    /// Per-CPU local enable mask.
    pub percpu_mask: [[u32; MAX_SHINT_WORDS]; MAX_CPUS],
    /// Number of active CPUs.
    pub max_cpus: u32,
}

impl Default for SharedIntState {
    fn default() -> Self {
        Self::new()
    }
}

impl SharedIntState {
    /// Create an empty shared interrupt state.
    pub const fn new() -> Self {
        Self {
            num_ints: 0,
            pending: [0u32; MAX_SHINT_WORDS],
            enable: [0u32; MAX_SHINT_WORDS],
            percpu_mask: [[0u32; MAX_SHINT_WORDS]; MAX_CPUS],
            max_cpus: 0,
        }
    }

    /// Initialize with a given number of shared interrupts and CPUs.
    pub fn init(&mut self, num_ints: u32, max_cpus: u32) {
        self.num_ints = num_ints;
        self.max_cpus = max_cpus;
        // Enable all per-cpu masks by default
        let words = num_ints.div_ceil(32) as usize;
        for cpu in 0..max_cpus as usize {
            for w in 0..words {
                self.percpu_mask[cpu][w] = !0u32;
            }
        }
    }

    /// Post (assert) a shared interrupt.
    ///
    /// Returns the CPU index that should receive the interrupt, or -1 if
    /// the interrupt is not deliverable (not enabled or no CPU available).
    pub fn post(&mut self, intno: u32) -> i32 {
        if intno >= self.num_ints {
            return -1;
        }
        let word = (intno >> 5) as usize;
        let bit = intno & 0x1f;
        let mask = 1u32 << bit;

        // Already pending
        if self.pending[word] & mask != 0 {
            return -1;
        }
        self.pending[word] |= mask;

        // Not globally enabled
        if self.enable[word] & mask == 0 {
            return -1;
        }

        // Find a CPU to deliver to
        self.find_target_cpu(intno)
    }

    /// Clear a shared interrupt.
    ///
    /// Returns true if the interrupt was pending before clearing.
    pub fn clear(&mut self, intno: u32) -> bool {
        if intno >= self.num_ints {
            return false;
        }
        let word = (intno >> 5) as usize;
        let bit = intno & 0x1f;
        let mask = 1u32 << bit;
        let was_pending = self.pending[word] & mask != 0;
        self.pending[word] &= !mask;
        was_pending
    }

    /// Enable a shared interrupt globally.
    ///
    /// Returns the CPU to deliver to if the interrupt is already pending, or -1.
    pub fn enable(&mut self, intno: u32) -> i32 {
        if intno >= self.num_ints {
            return -1;
        }
        let word = (intno >> 5) as usize;
        let bit = intno & 0x1f;
        let mask = 1u32 << bit;

        if self.enable[word] & mask != 0 {
            return -1; // Already enabled
        }
        self.enable[word] |= mask;

        if self.pending[word] & mask != 0 {
            return self.find_target_cpu(intno);
        }
        -1
    }

    /// Disable a shared interrupt globally.
    pub fn disable(&mut self, intno: u32) {
        if intno >= self.num_ints {
            return;
        }
        let word = (intno >> 5) as usize;
        let bit = intno & 0x1f;
        let mask = 1u32 << bit;

        if self.enable[word] & mask == 0 {
            return; // Already disabled
        }
        self.enable[word] &= !mask;
    }

    /// Enable a shared interrupt locally for a specific CPU.
    ///
    /// Returns the CPU index if the interrupt is now deliverable, or -1.
    pub fn local_enable(&mut self, intno: u32, cpu: u32) -> i32 {
        if intno >= self.num_ints || cpu >= self.max_cpus {
            return -1;
        }
        let word = (intno >> 5) as usize;
        let bit = intno & 0x1f;
        let mask = 1u32 << bit;

        if self.percpu_mask[cpu as usize][word] & mask != 0 {
            return -1; // Already enabled
        }
        self.percpu_mask[cpu as usize][word] |= mask;

        if self.enable[word] & self.pending[word] & mask != 0 {
            return cpu as i32;
        }
        -1
    }

    /// Disable a shared interrupt locally for a specific CPU.
    pub fn local_disable(&mut self, intno: u32, cpu: u32) {
        if intno >= self.num_ints || cpu >= self.max_cpus {
            return;
        }
        let word = (intno >> 5) as usize;
        let bit = intno & 0x1f;
        let mask = 1u32 << bit;

        if self.percpu_mask[cpu as usize][word] & mask == 0 {
            return; // Already disabled
        }
        self.percpu_mask[cpu as usize][word] &= !mask;
    }

    /// Set interrupt affinity: disable for all CPUs, then enable for the target.
    pub fn set_affinity(&mut self, intno: u32, target_cpu: u32) {
        for cpu in 0..self.max_cpus {
            self.local_disable(intno, cpu);
        }
        self.local_enable(intno, target_cpu);
    }

    /// Get the best pending+enabled+locally-enabled interrupt for a CPU.
    ///
    /// Clears both pending and enable bits (acknowledges the interrupt).
    /// Returns the interrupt number + offset, or -1 if none.
    pub fn get(&mut self, cpu: u32, offset: u32) -> i32 {
        if cpu >= self.max_cpus {
            return -1;
        }
        let words = self.num_ints.div_ceil(32) as usize;
        for j in 0..words {
            let deliverable = self.pending[j] & self.enable[j] & self.percpu_mask[cpu as usize][j];
            if deliverable != 0 {
                let bit = deliverable.trailing_zeros();
                self.enable[j] &= !(1u32 << bit);
                self.pending[j] &= !(1u32 << bit);
                return (offset + j as u32 * 32 + bit) as i32;
            }
        }
        -1
    }

    /// Peek at the best pending+enabled+locally-enabled interrupt for a CPU.
    ///
    /// Returns the interrupt number + offset, or -1 if none.
    pub fn peek(&self, cpu: u32, offset: u32) -> i32 {
        if cpu >= self.max_cpus {
            return -1;
        }
        let words = self.num_ints.div_ceil(32) as usize;
        for j in 0..words {
            let deliverable = self.pending[j] & self.enable[j] & self.percpu_mask[cpu as usize][j];
            if deliverable != 0 {
                let bit = deliverable.trailing_zeros();
                return (offset + j as u32 * 32 + bit) as i32;
            }
        }
        -1
    }

    /// Get the status of a specific shared interrupt for a CPU.
    ///
    /// Returns: bit 0 = pending, bit 1 = local enable, bit 2 = global enable.
    pub fn status(&self, intno: u32, cpu: u32) -> u32 {
        if intno >= self.num_ints || cpu >= self.max_cpus {
            return 0;
        }
        let word = (intno >> 5) as usize;
        let bit = intno & 0x1f;
        let mut ret = 0u32;
        ret |= (self.pending[word] >> bit) & 1;
        ret |= ((self.percpu_mask[cpu as usize][word] >> bit) & 1) << 1;
        ret |= ((self.enable[word] >> bit) & 1) << 2;
        ret
    }

    /// Find a CPU to deliver an interrupt to.
    ///
    /// Checks the per-CPU local mask to find an eligible CPU.
    /// Returns CPU index or -1 if none found.
    fn find_target_cpu(&self, intno: u32) -> i32 {
        let word = (intno >> 5) as usize;
        let bit = intno & 0x1f;
        let mask = 1u32 << bit;

        for cpu in 0..self.max_cpus as usize {
            if self.percpu_mask[cpu][word] & mask != 0 {
                return cpu as i32;
            }
        }
        -1
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_shint_initial() {
        let s = SharedIntState::new();
        assert_eq!(s.num_ints, 0);
        assert_eq!(s.peek(0, 0), -1);
    }

    #[test]
    fn test_shint_post_and_get() {
        let mut s = SharedIntState::new();
        s.init(32, 2);

        // Enable interrupt 5 globally
        s.enable(5);
        // Post it
        let cpu = s.post(5);
        assert!(cpu >= 0); // Should find a target CPU

        // Get should return it
        let intno = s.get(0, 16); // offset 16 (after percpu)
        assert_eq!(intno, 16 + 5);
    }

    #[test]
    fn test_shint_not_enabled() {
        let mut s = SharedIntState::new();
        s.init(32, 1);

        // Post without enabling
        let cpu = s.post(5);
        assert_eq!(cpu, -1);

        // Peek should find nothing deliverable
        assert_eq!(s.peek(0, 0), -1);
    }

    #[test]
    fn test_shint_local_disable() {
        let mut s = SharedIntState::new();
        s.init(32, 2);
        s.enable(5);

        // Locally disable on CPU 0
        s.local_disable(5, 0);
        s.post(5);

        // CPU 0 should not see it
        assert_eq!(s.peek(0, 0), -1);
        // CPU 1 should see it
        assert_eq!(s.peek(1, 0), 5);
    }

    #[test]
    fn test_shint_affinity() {
        let mut s = SharedIntState::new();
        s.init(32, 4);
        s.enable(10);

        // Set affinity to CPU 2
        s.set_affinity(10, 2);
        s.post(10);

        // Only CPU 2 should see it
        assert_eq!(s.peek(0, 0), -1);
        assert_eq!(s.peek(1, 0), -1);
        assert_eq!(s.peek(2, 0), 10);
        assert_eq!(s.peek(3, 0), -1);
    }

    #[test]
    fn test_shint_clear() {
        let mut s = SharedIntState::new();
        s.init(32, 1);
        s.enable(5);
        s.post(5);
        assert!(s.peek(0, 0) >= 0);

        s.clear(5);
        assert_eq!(s.peek(0, 0), -1);
    }

    #[test]
    fn test_shint_disable() {
        let mut s = SharedIntState::new();
        s.init(32, 1);
        s.enable(5);
        s.post(5);
        assert!(s.peek(0, 0) >= 0);

        s.disable(5);
        assert_eq!(s.peek(0, 0), -1);
    }

    #[test]
    fn test_shint_status() {
        let mut s = SharedIntState::new();
        s.init(32, 1);

        assert_eq!(s.status(5, 0), 0b010); // Only local enable (from init)

        s.enable(5);
        assert_eq!(s.status(5, 0), 0b110); // Global + local enable

        s.post(5);
        assert_eq!(s.status(5, 0), 0b111); // Pending + global + local
    }

    #[test]
    fn test_shint_multiple_interrupts() {
        let mut s = SharedIntState::new();
        s.init(64, 1);
        s.enable(5);
        s.enable(10);
        s.enable(33); // Second word
        s.post(5);
        s.post(10);
        s.post(33);

        // Get returns lowest first
        assert_eq!(s.get(0, 0), 5);
        assert_eq!(s.get(0, 0), 10);
        assert_eq!(s.get(0, 0), 33);
        assert_eq!(s.get(0, 0), -1);
    }
}
