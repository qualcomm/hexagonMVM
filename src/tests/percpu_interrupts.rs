/*
 * Copyright (c) Qualcomm Technologies, Inc. and/or its subsidiaries.
 * SPDX-License-Identifier: BSD-3-Clause-Clear
 */

use crate::debug;
use minivm_intc::percpu::CpuIntState as CpuInt;

pub fn run() {
    debug::write0(b"  [test] per-cpu interrupts\n\0");
    {
        let mut cpu = CpuInt::new();
        assert!(!cpu.any_deliverable());
        assert_eq!(cpu.peek(), -1);

        // Post interrupt 3 (auto-enables globally, not locally → not deliverable)
        cpu.post(3);
        assert_eq!(cpu.status(3), 5); // pending + global_en
        assert_eq!(cpu.peek(), 3); // peek shows pending+global_en
        assert!(!cpu.any_deliverable()); // not locally enabled

        // Enable locally - now deliverable
        cpu.local_enable(3);
        assert_eq!(cpu.status(3), 7); // pending + local_en + global_en
        assert!(cpu.any_deliverable());
        assert_eq!(cpu.peek(), 3);

        // Clear pending
        cpu.clear(3);
        assert_eq!(cpu.status(3), 6); // local_en + global_en
        assert!(!cpu.any_deliverable());

        // Test get() with priority ordering
        let mut cpu2 = CpuInt::new();
        cpu2.local_enable(5);
        cpu2.local_enable(3);
        cpu2.post(5);
        cpu2.post(3);
        // get() returns lowest-numbered first and clears
        assert_eq!(cpu2.get(), 3);
        assert_eq!(cpu2.get(), 5);
        assert_eq!(cpu2.get(), -1);

        // Test disable
        let mut cpu3 = CpuInt::new();
        cpu3.local_enable(7);
        cpu3.post(7);
        assert!(cpu3.any_deliverable());
        cpu3.disable(7);
        assert!(!cpu3.any_deliverable());
        assert_eq!(cpu3.status(7), 3); // pending + local_en

        // Out-of-range operations are no-ops
        let mut cpu4 = CpuInt::new();
        assert!(!cpu4.post(16));
        assert!(!cpu4.enable(20));
        assert_eq!(cpu4.status(16), 0);
    }
    debug::write0(b"  [test] per-cpu interrupts OK\n\0");
}
