/*
 * Copyright (c) Qualcomm Technologies, Inc. and/or its subsidiaries.
 * SPDX-License-Identifier: BSD-3-Clause-Clear
 */

//! Integration tests that build Hexagon assembly guest binaries and run
//! them on QEMU with the minivm kernel.
//!
//! These tests require:
//!   - clang with Hexagon target support (`CLANG` env var or on PATH)
//!   - llvm-objcopy (`OBJCOPY` env var or on PATH)
//!   - qemu-system-hexagon (`QEMU` env var or on PATH)
//!   - minivm built for hexagon-unknown-none-elf (`MINIVM` env var or
//!     auto-detected in target/)
//!
//! Missing tools are errors: this crate is an explicitly requested integration
//! suite. The ordinary host unit-test target excludes it.

use std::path::{Path, PathBuf};
use std::process::Command;

/// Locate the workspace root (parent of crates/).
fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent() // crates/
        .unwrap()
        .parent() // workspace root
        .unwrap()
        .to_path_buf()
}

/// Find a tool by checking an env var, then PATH.
fn find_tool(env_var: &str, name: &str) -> Option<PathBuf> {
    if let Ok(val) = std::env::var(env_var) {
        let p = PathBuf::from(val);
        if p.exists() {
            return Some(p);
        }
    }
    if let Ok(output) = Command::new("which").arg(name).output() {
        if output.status.success() {
            let s = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !s.is_empty() {
                return Some(PathBuf::from(s));
            }
        }
    }
    None
}

fn find_clang() -> Option<PathBuf> {
    find_tool("CLANG", "clang")
}

fn find_objcopy() -> Option<PathBuf> {
    find_tool("OBJCOPY", "llvm-objcopy")
}

fn find_qemu() -> Option<PathBuf> {
    find_tool("QEMU", "qemu-system-hexagon")
}

fn find_minivm() -> Option<PathBuf> {
    if let Ok(val) = std::env::var("MINIVM") {
        let p = PathBuf::from(val);
        if p.exists() {
            return Some(p);
        }
    }
    let root = workspace_root();
    let release = root.join("target/hexagon-unknown-none-elf/release/minivm");
    if release.exists() {
        return Some(release);
    }
    let debug = root.join("target/hexagon-unknown-none-elf/debug/minivm");
    if debug.exists() {
        return Some(debug);
    }
    None
}

struct TestEnv {
    clang: PathBuf,
    objcopy: PathBuf,
    qemu: PathBuf,
    minivm: PathBuf,
    root: PathBuf,
    build_dir: PathBuf,
}

impl TestEnv {
    fn new() -> Result<Self, String> {
        let clang = find_clang().ok_or("missing clang (set CLANG)")?;
        let objcopy = find_objcopy().ok_or("missing llvm-objcopy (set OBJCOPY)")?;
        let qemu = find_qemu().ok_or("missing qemu-system-hexagon (set QEMU)")?;
        let minivm = find_minivm().ok_or("missing built minivm (set MINIVM)")?;
        let root = workspace_root();
        let build_dir = root.join("target/guest-tests");
        std::fs::create_dir_all(&build_dir)
            .map_err(|e| format!("cannot create {}: {e}", build_dir.display()))?;
        Ok(Self {
            clang,
            objcopy,
            qemu,
            minivm,
            root,
            build_dir,
        })
    }

    /// Build a .S file into a flat binary.
    fn build_guest(&self, name: &str) -> Result<PathBuf, String> {
        let src = self.root.join(format!("tests/{name}.S"));
        let elf = self.build_dir.join(format!("{name}.elf"));
        let bin = self.build_dir.join(format!("{name}.bin"));
        let ld = self.root.join("tests/guest.ld");

        // Compile + link
        let output = Command::new(&self.clang)
            .args([
                // Not the toolchain's hexagon.cfg, which adds picolibc.
                "--no-default-config",
                "--target=hexagon",
                "-mcpu=hexagonv73",
                "-nostdlib",
                "-fuse-ld=lld",
                "-T",
            ])
            .arg(&ld)
            .arg("-I")
            .arg(&self.root)
            .arg("-o")
            .arg(&elf)
            .arg(&src)
            .output()
            .map_err(|e| format!("clang failed to start: {e}"))?;

        if !output.status.success() {
            return Err(format!(
                "clang failed:\n{}",
                String::from_utf8_lossy(&output.stderr)
            ));
        }

        // objcopy to flat binary
        let output = Command::new(&self.objcopy)
            .args(["-O", "binary"])
            .arg(&elf)
            .arg(&bin)
            .output()
            .map_err(|e| format!("objcopy failed to start: {e}"))?;

        if !output.status.success() {
            return Err(format!(
                "objcopy failed:\n{}",
                String::from_utf8_lossy(&output.stderr)
            ));
        }

        Ok(bin)
    }

    /// Run a guest binary on QEMU and return combined output.
    fn run_guest(&self, bin_path: &Path) -> Result<String, String> {
        let output = Command::new("timeout")
            .arg("30")
            .arg(&self.qemu)
            // `-bios none`: do not load QEMU's bundled firmware.
            .args(["-M", "virt", "-bios", "none", "-nographic"])
            .arg("-kernel")
            .arg(&self.minivm)
            .arg("-device")
            .arg(format!(
                "loader,addr=0xa0000000,file={}",
                bin_path.display()
            ))
            .output()
            .map_err(|e| format!("QEMU failed to start: {e}"))?;

        if !output.status.success() {
            return Err(format!(
                "QEMU/timeout exited with {}\nstdout:\n{}\nstderr:\n{}",
                output.status,
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            ));
        }

        // Combine stdout and stderr — QEMU sends semihosting output to
        // stdout and VM console/debug output to stderr.
        let mut combined = String::from_utf8_lossy(&output.stdout).to_string();
        combined.push_str(&String::from_utf8_lossy(&output.stderr));
        Ok(combined)
    }
}

fn run_guest_test(name: &str) {
    let env = TestEnv::new().unwrap_or_else(|e| panic!("Cannot run guest test '{name}': {e}"));

    let bin = env
        .build_guest(name)
        .unwrap_or_else(|e| panic!("Failed to build {name}: {e}"));

    let output = env
        .run_guest(&bin)
        .unwrap_or_else(|e| panic!("Failed to run {name}: {e}"));

    // The test binaries print exactly "PASS\n" via semihosting.
    let guest_pass = output.lines().any(|line| line.trim() == "PASS");
    assert!(
        guest_pass,
        "Guest test '{name}' did not output PASS.\nFull output:\n{output}"
    );
}

#[test]
fn guest_first() {
    run_guest_test("first");
}

#[test]
fn guest_vmversion() {
    run_guest_test("test_vmversion");
}

#[test]
fn guest_interrupts() {
    run_guest_test("test_interrupts");
}

#[test]
fn guest_mmu() {
    run_guest_test("test_mmu");
}

#[test]
fn guest_processors() {
    run_guest_test("test_processors");
}
