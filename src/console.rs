/*
 * Copyright (c) Qualcomm Technologies, Inc. and/or its subsidiaries.
 * SPDX-License-Identifier: BSD-3-Clause-Clear
 */

//! The monitor's own console output.
//!
//! When the device tree names a PL011, the monitor writes to its data
//! register through a wired mapping, which works from any context. Without
//! one it falls back to semihosting, which only works before the guest runs:
//! `trap0` from inside an exception handler is a double exception.

use core::sync::atomic::{AtomicBool, AtomicU32, Ordering};

/// Virtual address of the PL011 data register, or 0.
static UART: AtomicU32 = AtomicU32::new(0);
/// Whether semihosting may still be used.
static EARLY: AtomicBool = AtomicBool::new(true);

/// Send output to the PL011 mapped at `va`.
pub fn use_uart(va: u32) {
    UART.store(va, Ordering::Relaxed);
}

/// The guest is about to run: no more semihosting.
pub fn end_early() {
    EARLY.store(false, Ordering::Relaxed);
}

pub fn puts(s: &[u8]) {
    let uart = UART.load(Ordering::Relaxed);
    if uart != 0 {
        for &b in s {
            unsafe { core::ptr::write_volatile(uart as *mut u32, b as u32) };
        }
    } else if EARLY.load(Ordering::Relaxed) {
        // write0 wants a terminated string.
        let mut buf = [0u8; 96];
        for chunk in s.chunks(buf.len() - 1) {
            buf[..chunk.len()].copy_from_slice(chunk);
            buf[chunk.len()] = 0;
            crate::semihosting::write0(&buf[..=chunk.len()]);
        }
    }
}

pub fn hex(v: u32) {
    let digits = b"0123456789abcdef";
    let mut buf = [0u8; 10];
    buf[0] = b'0';
    buf[1] = b'x';
    for i in 0..8 {
        buf[2 + i] = digits[((v >> (28 - 4 * i)) & 0xf) as usize];
    }
    puts(&buf);
}

pub fn dec(mut v: u32) {
    let mut buf = [0u8; 10];
    let mut i = buf.len();
    loop {
        i -= 1;
        buf[i] = b'0' + (v % 10) as u8;
        v /= 10;
        if v == 0 {
            break;
        }
    }
    puts(&buf[i..]);
}

/// Print `label`, a value in hex, and a newline.
pub fn field(label: &[u8], v: u32) {
    puts(label);
    hex(v);
    puts(b"\n");
}
