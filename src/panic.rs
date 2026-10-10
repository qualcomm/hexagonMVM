/*
 * Copyright (c) Qualcomm Technologies, Inc. and/or its subsidiaries.
 * SPDX-License-Identifier: BSD-3-Clause-Clear
 */

//! Panic handler for bare-metal Hexagon target.

#[cfg(not(test))]
use core::panic::PanicInfo;

#[panic_handler]
#[cfg(not(test))]
fn panic(info: &PanicInfo) -> ! {
    crate::debug::write0(b"PANIC at \0");
    if let Some(loc) = info.location() {
        let file = loc.file().as_bytes();
        let mut buf = [0u8; 128];
        let len = file.len().min(120);
        buf[..len].copy_from_slice(&file[..len]);
        buf[len] = b':';
        // Print line number as decimal
        let mut line = loc.line();
        let mut pos = len + 1;
        if line == 0 {
            buf[pos] = b'0';
            pos += 1;
        } else {
            let start = pos;
            while line > 0 && pos < 126 {
                buf[pos] = b'0' + (line % 10) as u8;
                line /= 10;
                pos += 1;
            }
            // Reverse the digits
            let mut a = start;
            let mut b = pos - 1;
            while a < b {
                buf.swap(a, b);
                a += 1;
                b -= 1;
            }
        }
        buf[pos] = b'\n';
        buf[pos + 1] = 0;
        crate::debug::write0(&buf);
    } else {
        crate::debug::write0(b"unknown\n\0");
    }
    crate::semihosting::exit(1);
}
