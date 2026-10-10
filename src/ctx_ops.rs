/*
 * Copyright (c) Qualcomm Technologies, Inc. and/or its subsidiaries.
 * SPDX-License-Identifier: BSD-3-Clause-Clear
 */

//! Register accessor helpers for ThreadContext 64-bit register pairs.
//!
//! Only called from `#[cfg(target_arch = "hexagon")]` handler code.
//! Non-hexagon builds get stubs so the functions are never reported as dead code.

use minivm_sched::context::ThreadContext;
#[cfg(target_arch = "hexagon")]
use minivm_types::vm::VMSTATUS_IE;

#[cfg(target_arch = "hexagon")]
pub fn ctx_r0(ctx: &ThreadContext) -> u32 {
    ctx.r0100 as u32
}
#[cfg(target_arch = "hexagon")]
pub fn ctx_r1(ctx: &ThreadContext) -> u32 {
    (ctx.r0100 >> 32) as u32
}
#[cfg(target_arch = "hexagon")]
pub fn ctx_r2(ctx: &ThreadContext) -> u32 {
    ctx.r0302 as u32
}
#[cfg(target_arch = "hexagon")]
pub fn ctx_r3(ctx: &ThreadContext) -> u32 {
    (ctx.r0302 >> 32) as u32
}
#[cfg(target_arch = "hexagon")]
pub fn ctx_r4(ctx: &ThreadContext) -> u32 {
    ctx.r0504 as u32
}
#[cfg(target_arch = "hexagon")]
pub fn ctx_r5(ctx: &ThreadContext) -> u32 {
    (ctx.r0504 >> 32) as u32
}
#[cfg(target_arch = "hexagon")]
pub fn ctx_r29(ctx: &ThreadContext) -> u32 {
    (ctx.r2928 >> 32) as u32
}
#[cfg(target_arch = "hexagon")]
pub fn set_r0(ctx: &mut ThreadContext, val: u32) {
    ctx.r0100 = (ctx.r0100 & 0xFFFF_FFFF_0000_0000) | val as u64;
}
#[cfg(target_arch = "hexagon")]
pub fn set_r29(ctx: &mut ThreadContext, val: u32) {
    ctx.r2928 = (val as u64) << 32 | (ctx.r2928 & 0xFFFF_FFFF);
}
#[cfg(target_arch = "hexagon")]
pub fn ie_enabled(ctx: &ThreadContext) -> bool {
    ctx.vmstatus & VMSTATUS_IE != 0
}
#[cfg(target_arch = "hexagon")]
pub fn set_ie(ctx: &mut ThreadContext, enable: bool) {
    if enable {
        ctx.vmstatus |= VMSTATUS_IE;
    } else {
        ctx.vmstatus &= !VMSTATUS_IE;
    }
    // Also update SSR.IE (bit 18) so hardware interrupts fire in guest mode.
    // When context_restore_rte restores SSR, the CPU's IE reflects the virtual state.
    const SSR_IE: u32 = 1 << 18;
    if enable {
        ctx.ssr |= SSR_IE;
    } else {
        ctx.ssr &= !SSR_IE;
    }
}

#[cfg(not(target_arch = "hexagon"))]
pub fn ctx_r0(_: &ThreadContext) -> u32 {
    unreachable!()
}
#[cfg(not(target_arch = "hexagon"))]
pub fn ctx_r1(_: &ThreadContext) -> u32 {
    unreachable!()
}
#[cfg(not(target_arch = "hexagon"))]
pub fn ctx_r2(_: &ThreadContext) -> u32 {
    unreachable!()
}
#[cfg(not(target_arch = "hexagon"))]
pub fn ctx_r3(_: &ThreadContext) -> u32 {
    unreachable!()
}
#[cfg(not(target_arch = "hexagon"))]
pub fn ctx_r4(_: &ThreadContext) -> u32 {
    unreachable!()
}
#[cfg(not(target_arch = "hexagon"))]
pub fn ctx_r5(_: &ThreadContext) -> u32 {
    unreachable!()
}
#[cfg(not(target_arch = "hexagon"))]
pub fn ctx_r29(_: &ThreadContext) -> u32 {
    unreachable!()
}
#[cfg(not(target_arch = "hexagon"))]
pub fn set_r0(_: &mut ThreadContext, _: u32) {
    unreachable!()
}
#[cfg(not(target_arch = "hexagon"))]
pub fn set_r29(_: &mut ThreadContext, _: u32) {
    unreachable!()
}
#[cfg(not(target_arch = "hexagon"))]
pub fn ie_enabled(_: &ThreadContext) -> bool {
    unreachable!()
}
#[cfg(not(target_arch = "hexagon"))]
pub fn set_ie(_: &mut ThreadContext, _: bool) {
    unreachable!()
}
