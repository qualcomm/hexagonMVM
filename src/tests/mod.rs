/*
 * Copyright (c) Qualcomm Technologies, Inc. and/or its subsidiaries.
 * SPDX-License-Identifier: BSD-3-Clause-Clear
 */

//! On-target integration tests for minivm.
//!
//! Each submodule contains one test section that runs sequentially on
//! Hexagon/QEMU. The `run_all_tests` function calls them in order.

mod asid_table;
#[cfg(target_arch = "hexagon")]
mod boot_vm_traps;
#[cfg(target_arch = "hexagon")]
mod context_switch;
pub mod guest_entries;
#[cfg(target_arch = "hexagon")]
mod guest_mode;
#[cfg(target_arch = "hexagon")]
mod hardware_trap1;
mod percpu_interrupts;
mod scheduler_readylist;
mod scheduler_runlist;
mod shared_interrupts;
mod thread_creation;
mod timeout_tree;
mod tlb_fill_pipeline;
#[cfg(target_arch = "hexagon")]
mod trap0_config_vmop;
mod vm_block_ops;
mod vm_trap_dispatch;
#[cfg(target_arch = "hexagon")]
mod vmboot_test;
#[cfg(target_arch = "hexagon")]
mod vmop_boot_e2e;

use minivm_init::globals::KernelGlobals;
use minivm_mem::asid::AsidTable;

pub fn run_all_tests(kg: &KernelGlobals, asid_table: &mut AsidTable) {
    vm_trap_dispatch::run();
    #[cfg(target_arch = "hexagon")]
    hardware_trap1::run();
    thread_creation::run();
    percpu_interrupts::run();
    shared_interrupts::run();
    scheduler_readylist::run();
    asid_table::run();
    timeout_tree::run();
    vm_block_ops::run();
    scheduler_runlist::run();
    #[cfg(target_arch = "hexagon")]
    context_switch::run();
    #[cfg(target_arch = "hexagon")]
    guest_mode::run();
    #[cfg(target_arch = "hexagon")]
    boot_vm_traps::run(kg);
    tlb_fill_pipeline::run();
    #[cfg(target_arch = "hexagon")]
    vmboot_test::run(asid_table, kg);
    #[cfg(target_arch = "hexagon")]
    trap0_config_vmop::run(kg, asid_table);
    #[cfg(target_arch = "hexagon")]
    vmop_boot_e2e::run();
}
