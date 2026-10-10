/*
 * Copyright (c) Qualcomm Technologies, Inc. and/or its subsidiaries.
 * SPDX-License-Identifier: BSD-3-Clause-Clear
 */

//! Atomic bitmask operations (set/clear/test).

use core::sync::atomic::{AtomicU32, Ordering};

/// Atomically set bits in a bitmask.
#[inline]
pub fn atomic_set_mask(target: &AtomicU32, mask: u32) {
    target.fetch_or(mask, Ordering::AcqRel);
}

/// Atomically clear bits in a bitmask.
#[inline]
pub fn atomic_clear_mask(target: &AtomicU32, mask: u32) {
    target.fetch_and(!mask, Ordering::AcqRel);
}

/// Atomically test if any bits in mask are set, then set them.
/// Returns the old value.
#[inline]
pub fn atomic_test_set_mask(target: &AtomicU32, mask: u32) -> u32 {
    target.fetch_or(mask, Ordering::AcqRel)
}

/// Atomically test if any bits in mask are set, then clear them.
/// Returns the old value.
#[inline]
pub fn atomic_test_clear_mask(target: &AtomicU32, mask: u32) -> u32 {
    target.fetch_and(!mask, Ordering::AcqRel)
}

/// Atomically swap a value. Returns the old value.
#[inline]
pub fn atomic_swap(target: &AtomicU32, new_val: u32) -> u32 {
    target.swap(new_val, Ordering::AcqRel)
}

/// Atomically compare and swap. Returns the old value.
#[inline]
pub fn atomic_cas(target: &AtomicU32, expected: u32, new_val: u32) -> u32 {
    match target.compare_exchange(expected, new_val, Ordering::AcqRel, Ordering::Relaxed) {
        Ok(old) | Err(old) => old,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::sync::atomic::AtomicU32;

    #[test]
    fn test_set_mask() {
        let val = AtomicU32::new(0);
        atomic_set_mask(&val, 0x0F);
        assert_eq!(val.load(Ordering::Relaxed), 0x0F);
        atomic_set_mask(&val, 0xF0);
        assert_eq!(val.load(Ordering::Relaxed), 0xFF);
    }

    #[test]
    fn test_clear_mask() {
        let val = AtomicU32::new(0xFF);
        atomic_clear_mask(&val, 0x0F);
        assert_eq!(val.load(Ordering::Relaxed), 0xF0);
    }

    #[test]
    fn test_test_set_mask() {
        let val = AtomicU32::new(0x0F);
        let old = atomic_test_set_mask(&val, 0xF0);
        assert_eq!(old, 0x0F);
        assert_eq!(val.load(Ordering::Relaxed), 0xFF);
    }

    #[test]
    fn test_cas() {
        let val = AtomicU32::new(42);
        let old = atomic_cas(&val, 42, 99);
        assert_eq!(old, 42);
        assert_eq!(val.load(Ordering::Relaxed), 99);

        let old = atomic_cas(&val, 42, 100);
        assert_eq!(old, 99); // CAS failed, value unchanged
        assert_eq!(val.load(Ordering::Relaxed), 99);
    }
}
