/*
 * Copyright (c) Qualcomm Technologies, Inc. and/or its subsidiaries.
 * SPDX-License-Identifier: BSD-3-Clause-Clear
 */

//! Panic handler for bare-metal Hexagon target.

use core::panic::PanicInfo;

use crate::console;

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    console::puts(b"minivm: PANIC at ");
    if let Some(loc) = info.location() {
        console::puts(loc.file().as_bytes());
        console::puts(b":");
        console::dec(loc.line());
    }
    console::puts(b"\n");
    crate::die();
}
