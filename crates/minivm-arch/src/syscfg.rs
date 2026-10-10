/*
 * Copyright (c) Qualcomm Technologies, Inc. and/or its subsidiaries.
 * SPDX-License-Identifier: BSD-3-Clause-Clear
 */

//! SYSCFG register convenience operations.

use crate::hexagon;
use minivm_types::regs::Syscfg;

/// Enable the MMU via SYSCFG.
pub fn enable_mmu() {
    let cfg = hexagon::read_syscfg();
    hexagon::write_syscfg(cfg.with_mmu(true));
}

/// Disable the MMU via SYSCFG.
pub fn disable_mmu() {
    let cfg = hexagon::read_syscfg();
    hexagon::write_syscfg(cfg.with_mmu(false));
}

/// Enable guest mode via SYSCFG.
pub fn enable_guest_mode() {
    let cfg = hexagon::read_syscfg();
    hexagon::write_syscfg(cfg.with_guest(true));
}

/// Enable DMT (Dynamic Multi-Threading) via SYSCFG.
pub fn enable_dmt() {
    let cfg = hexagon::read_syscfg();
    hexagon::write_syscfg(cfg.with_dmt(true));
}

/// Read current SYSCFG value.
pub fn read() -> Syscfg {
    hexagon::read_syscfg()
}

/// Write a new SYSCFG value.
pub fn write(cfg: Syscfg) {
    hexagon::write_syscfg(cfg);
}
