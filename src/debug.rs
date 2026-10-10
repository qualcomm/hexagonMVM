/*
 * Copyright (c) Qualcomm Technologies, Inc. and/or its subsidiaries.
 * SPDX-License-Identifier: BSD-3-Clause-Clear
 */

//! Debug output indirection.
//!
//! Selects a backend at compile time via feature flags:
//! - *(default)*: semihosting (`trap0(#0)` op 4)
//! - `debug-uart`: PL011 UART at 0x1000_0000 (QEMU hexagon-virt)
//! - `debug-circ`: circular buffer (ring, readable post-mortem)

// ── UART backend ──────────────────────────────────────────────────
#[cfg(feature = "debug-uart")]
mod inner {
    const PL011_BASE: *mut u32 = 0x1000_0000 as *mut u32;

    pub fn init() {}

    pub fn write0(s: &[u8]) {
        for &b in s {
            if b == 0 {
                break;
            }
            unsafe {
                core::ptr::write_volatile(PL011_BASE, b as u32);
            }
        }
    }
}

// ── Circular buffer backend ───────────────────────────────────────
#[cfg(feature = "debug-circ")]
mod inner {
    use core::fmt::Write;

    const BUF_SIZE: usize = 4096;
    static mut BUF: [u8; BUF_SIZE] = [0u8; BUF_SIZE];
    static mut LOGGER: Option<log_buffer::LogBuffer<&'static mut [u8]>> = None;

    pub fn init() {
        unsafe {
            LOGGER = Some(log_buffer::LogBuffer::new(&mut BUF));
        }
    }

    pub fn write0(s: &[u8]) {
        unsafe {
            if let Some(ref mut lb) = LOGGER {
                for &b in s {
                    if b == 0 {
                        break;
                    }
                    let _ = lb.write_char(b as char);
                }
            }
        }
    }
}

// ── Semihosting backend (default) ────────────────────────────────
#[cfg(not(any(feature = "debug-uart", feature = "debug-circ")))]
mod inner {
    pub fn init() {}

    pub fn write0(s: &[u8]) {
        crate::semihosting::write0(s);
    }
}

/// Initialize the debug output backend. Call once before first `write0`.
pub fn init() {
    inner::init();
}

/// Write a null-terminated (or length-bounded) byte string to the debug output.
pub fn write0(s: &[u8]) {
    inner::write0(s);
}
