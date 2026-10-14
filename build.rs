/*
 * Copyright (c) Qualcomm Technologies, Inc. and/or its subsidiaries.
 * SPDX-License-Identifier: BSD-3-Clause-Clear
 */

//! Build script: links `libclang_rt.builtins.a` for the hexagon target.
//!
//! The hexagon calling convention uses compiler builtins for callee-saved
//! register save/restore sequences. These live in `libclang_rt.builtins.a`
//! from the Hexagon toolchain.
//!
//! Set `HEXAGON_CLANG_RT` to the path of `libclang_rt.builtins.a`, or
//! ensure `hexagon-unknown-none-elf-ld.lld` can find it via library path.

fn main() {
    if std::env::var("CARGO_CFG_TARGET_ARCH").as_deref() != Ok("hexagon") {
        return;
    }

    if let Ok(path) = std::env::var("HEXAGON_CLANG_RT") {
        println!("cargo:rustc-link-arg={path}");
    }
}
