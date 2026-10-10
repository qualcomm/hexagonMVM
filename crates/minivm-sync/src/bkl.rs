/*
 * Copyright (c) Qualcomm Technologies, Inc. and/or its subsidiaries.
 * SPDX-License-Identifier: BSD-3-Clause-Clear
 */

//! Big Kernel Lock (BKL).
//!
//! A single global spinlock used to protect kernel-wide critical sections.

use spin::Mutex;

/// The Big Kernel Lock (BKL).
///
/// Used for protecting kernel-wide state that doesn't have a more
/// fine-grained lock yet. Acquire/release should be as brief as possible.
static BKL: Mutex<()> = Mutex::new(());

/// Acquire the Big Kernel Lock.
#[inline]
pub fn bkl_lock() {
    core::mem::forget(BKL.lock());
}

/// Release the Big Kernel Lock.
#[inline]
pub fn bkl_unlock() {
    // Safety: caller guarantees the BKL is held.
    unsafe {
        BKL.force_unlock();
    }
}

/// Try to acquire the Big Kernel Lock without blocking.
#[inline]
pub fn bkl_try_lock() -> bool {
    BKL.try_lock().map(core::mem::forget).is_some()
}
