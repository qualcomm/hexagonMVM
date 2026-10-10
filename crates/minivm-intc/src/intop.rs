/*
 * Copyright (c) Qualcomm Technologies, Inc. and/or its subsidiaries.
 * SPDX-License-Identifier: BSD-3-Clause-Clear
 */

//! Interrupt operations dispatch.
//!
//! The `intop` function dispatches interrupt operations (enable, disable,
//! post, clear, get, peek, status, etc.) to the appropriate handler based
//! on the operation type and whether the interrupt number falls in the
//! per-CPU or shared range.

use crate::percpu::CpuIntState;
use crate::shared::SharedIntState;
use minivm_types::consts::PERCPU_INTERRUPTS;
use minivm_types::vmint::IntOp;

/// Result of an intop dispatch.
#[derive(Debug, PartialEq, Eq)]
pub enum IntOpResult {
    /// Operation succeeded, return value to guest.
    Ok(i32),
    /// Operation failed (invalid op, bad target, etc.).
    Fail,
    /// Interrupt needs delivery to a CPU (cpu index).
    Deliver(u32),
}

/// Dispatch an interrupt operation.
///
/// `cpu_state` is the per-CPU interrupt state of the target thread.
/// `shared` is the VM's shared interrupt state.
/// `op` is the operation to perform.
/// `intno` is the interrupt number.
/// `cpu` is the CPU index of the calling/target thread.
pub fn intop_dispatch(
    cpu_state: &mut CpuIntState,
    shared: &mut SharedIntState,
    op: IntOp,
    intno: u32,
    cpu: u32,
) -> IntOpResult {
    match op {
        IntOp::Nop => IntOpResult::Ok(0),

        IntOp::GlobEn => {
            if intno < PERCPU_INTERRUPTS {
                let needs_deliver = cpu_state.enable(intno);
                if needs_deliver {
                    IntOpResult::Deliver(cpu)
                } else {
                    IntOpResult::Ok(0)
                }
            } else {
                let target = shared.enable(intno - PERCPU_INTERRUPTS);
                if target >= 0 {
                    IntOpResult::Deliver(target as u32)
                } else {
                    IntOpResult::Ok(0)
                }
            }
        }

        IntOp::GlobDis => {
            if intno < PERCPU_INTERRUPTS {
                cpu_state.disable(intno);
            } else {
                shared.disable(intno - PERCPU_INTERRUPTS);
            }
            IntOpResult::Ok(0)
        }

        IntOp::LocEn => {
            if intno < PERCPU_INTERRUPTS {
                let needs_deliver = cpu_state.local_enable(intno);
                if needs_deliver {
                    IntOpResult::Deliver(cpu)
                } else {
                    IntOpResult::Ok(0)
                }
            } else {
                let target = shared.local_enable(intno - PERCPU_INTERRUPTS, cpu);
                if target >= 0 {
                    IntOpResult::Deliver(target as u32)
                } else {
                    IntOpResult::Ok(0)
                }
            }
        }

        IntOp::LocDis => {
            if intno < PERCPU_INTERRUPTS {
                cpu_state.local_disable(intno);
            } else {
                shared.local_disable(intno - PERCPU_INTERRUPTS, cpu);
            }
            IntOpResult::Ok(0)
        }

        IntOp::Affinity => {
            if intno < PERCPU_INTERRUPTS {
                IntOpResult::Ok(-1) // Not supported for per-CPU
            } else {
                shared.set_affinity(intno - PERCPU_INTERRUPTS, cpu);
                IntOpResult::Ok(0)
            }
        }

        IntOp::Get => {
            // Try per-CPU first
            let cpuint = cpu_state.get();
            if cpuint >= 0 {
                return IntOpResult::Ok(cpuint);
            }
            // Then shared
            let shint = shared.get(cpu, PERCPU_INTERRUPTS);
            if shint >= 0 {
                return IntOpResult::Ok(shint);
            }
            IntOpResult::Ok(-1)
        }

        IntOp::Peek => {
            // Try per-CPU first
            let cpuint = cpu_state.peek();
            if cpuint >= 0 {
                return IntOpResult::Ok(cpuint);
            }
            // Then shared
            let shint = shared.peek(cpu, PERCPU_INTERRUPTS);
            if shint >= 0 {
                return IntOpResult::Ok(shint);
            }
            IntOpResult::Ok(-1)
        }

        IntOp::Status => {
            if intno < PERCPU_INTERRUPTS {
                IntOpResult::Ok(cpu_state.status(intno) as i32)
            } else {
                IntOpResult::Ok(shared.status(intno - PERCPU_INTERRUPTS, cpu) as i32)
            }
        }

        IntOp::Post => {
            if intno < PERCPU_INTERRUPTS {
                let needs_deliver = cpu_state.post(intno);
                if needs_deliver {
                    IntOpResult::Deliver(cpu)
                } else {
                    IntOpResult::Ok(0)
                }
            } else {
                let target = shared.post(intno - PERCPU_INTERRUPTS);
                if target >= 0 {
                    IntOpResult::Deliver(target as u32)
                } else {
                    IntOpResult::Ok(0)
                }
            }
        }

        IntOp::Clear => {
            let was_pending = if intno < PERCPU_INTERRUPTS {
                cpu_state.clear(intno)
            } else {
                shared.clear(intno - PERCPU_INTERRUPTS)
            };
            IntOpResult::Ok(was_pending as i32)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_intop_nop() {
        let mut cpu = CpuIntState::new();
        let mut shared = SharedIntState::new();
        assert_eq!(
            intop_dispatch(&mut cpu, &mut shared, IntOp::Nop, 0, 0),
            IntOpResult::Ok(0)
        );
    }

    #[test]
    fn test_intop_cpuint_post_get() {
        let mut cpu = CpuIntState::new();
        let mut shared = SharedIntState::new();

        // Locally enable interrupt 3
        intop_dispatch(&mut cpu, &mut shared, IntOp::LocEn, 3, 0);
        // Post interrupt 3 (auto-enables globally)
        let result = intop_dispatch(&mut cpu, &mut shared, IntOp::Post, 3, 0);
        assert_eq!(result, IntOpResult::Deliver(0));

        // Get should return 3
        let result = intop_dispatch(&mut cpu, &mut shared, IntOp::Get, 0, 0);
        assert_eq!(result, IntOpResult::Ok(3));
    }

    #[test]
    fn test_intop_cpuint_post_locen() {
        let mut cpu = CpuIntState::new();
        let mut shared = SharedIntState::new();

        // Post interrupt 13 (auto-enables globally, but not locally)
        let result = intop_dispatch(&mut cpu, &mut shared, IntOp::Post, 13, 0);
        assert_eq!(result, IntOpResult::Ok(0)); // Not deliverable yet

        // Status: pending + global_en = 5
        let result = intop_dispatch(&mut cpu, &mut shared, IntOp::Status, 13, 0);
        assert_eq!(result, IntOpResult::Ok(5));

        // LocEn triggers delivery
        let result = intop_dispatch(&mut cpu, &mut shared, IntOp::LocEn, 13, 0);
        assert_eq!(result, IntOpResult::Deliver(0));
    }

    #[test]
    fn test_intop_shint_post_get() {
        let mut cpu = CpuIntState::new();
        let mut shared = SharedIntState::new();
        shared.init(32, 1);

        // Enable shared interrupt 20 (= shint 4)
        intop_dispatch(&mut cpu, &mut shared, IntOp::GlobEn, 20, 0);
        // Post it
        let result = intop_dispatch(&mut cpu, &mut shared, IntOp::Post, 20, 0);
        assert_eq!(result, IntOpResult::Deliver(0));

        // Get: no cpuint, should return shared at offset 16
        let result = intop_dispatch(&mut cpu, &mut shared, IntOp::Get, 0, 0);
        assert_eq!(result, IntOpResult::Ok(20));
    }

    #[test]
    fn test_intop_status() {
        let mut cpu = CpuIntState::new();
        let mut shared = SharedIntState::new();
        shared.init(32, 1);

        // Per-CPU status after POST: pending + global_en = 5
        cpu.post(5);
        let result = intop_dispatch(&mut cpu, &mut shared, IntOp::Status, 5, 0);
        assert_eq!(result, IntOpResult::Ok(5));

        // Shared status
        shared.enable(10);
        let result = intop_dispatch(
            &mut cpu,
            &mut shared,
            IntOp::Status,
            10 + PERCPU_INTERRUPTS,
            0,
        );
        assert_eq!(result, IntOpResult::Ok(0b110)); // Global + local enable
    }

    #[test]
    fn test_intop_disable() {
        let mut cpu = CpuIntState::new();
        let mut shared = SharedIntState::new();

        cpu.local_enable(3);
        cpu.post(3);
        assert!(cpu.any_deliverable());

        intop_dispatch(&mut cpu, &mut shared, IntOp::GlobDis, 3, 0);
        assert!(!cpu.any_deliverable());
    }

    #[test]
    fn test_intop_clear_returns_was_pending() {
        let mut cpu = CpuIntState::new();
        let mut shared = SharedIntState::new();

        cpu.post(3);

        let result = intop_dispatch(&mut cpu, &mut shared, IntOp::Clear, 3, 0);
        assert_eq!(result, IntOpResult::Ok(1)); // Was pending

        let result = intop_dispatch(&mut cpu, &mut shared, IntOp::Clear, 3, 0);
        assert_eq!(result, IntOpResult::Ok(0)); // Not pending anymore
    }

    #[test]
    fn test_intop_locdis_percpu() {
        let mut cpu = CpuIntState::new();
        let mut shared = SharedIntState::new();

        cpu.local_enable(3);
        cpu.post(3);
        assert!(cpu.any_deliverable());

        intop_dispatch(&mut cpu, &mut shared, IntOp::LocDis, 3, 0);
        assert!(!cpu.any_deliverable());
    }
}
