/*
 * Copyright (c) Qualcomm Technologies, Inc. and/or its subsidiaries.
 * SPDX-License-Identifier: BSD-3-Clause-Clear
 */

//! Hardware thread management (MODECTL, wait, start).

use crate::hexagon;

/// Start a hardware thread by setting its bit in MODECTL.
pub fn start_hthread(tid: u32) {
    let modectl = hexagon::read_modectl();
    hexagon::write_modectl(modectl | (1 << tid));
}

/// Stop a hardware thread by clearing its bit in MODECTL.
pub fn stop_hthread(tid: u32) {
    let modectl = hexagon::read_modectl();
    hexagon::write_modectl(modectl & !(1 << tid));
}

/// Check if a hardware thread is enabled.
pub fn is_hthread_enabled(tid: u32) -> bool {
    (hexagon::read_modectl() & (1 << tid)) != 0
}

/// Enter wait mode (idle the current hardware thread until next interrupt).
#[inline(always)]
pub fn enter_wait_mode() {
    #[cfg(target_arch = "hexagon")]
    unsafe {
        core::arch::asm!("wait(r0)", options(nomem, nostack))
    };
}

/// Resume a hardware thread (set its NMI pending).
#[inline(always)]
pub fn resume_hthread(tid: u32) {
    #[cfg(target_arch = "hexagon")]
    unsafe {
        core::arch::asm!("resume({tid})", tid = in(reg) tid, options(nomem, nostack))
    };
    #[cfg(not(target_arch = "hexagon"))]
    {
        let _ = tid;
    }
}
