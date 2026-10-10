/*
 * Copyright (c) Qualcomm Technologies, Inc. and/or its subsidiaries.
 * SPDX-License-Identifier: BSD-3-Clause-Clear
 */

use crate::debug;
use minivm_intc::shared::SharedIntState;

pub fn run() {
    debug::write0(b"  [test] shared interrupts\n\0");
    {
        // Test post + enable + get
        let mut sh = SharedIntState::new();
        sh.init(64, 4);

        // Not enabled yet - post returns -1
        let cpu = sh.post(5);
        assert_eq!(cpu, -1);
        sh.clear(5);

        // Enable first, then post
        sh.enable(5);
        let cpu = sh.post(5);
        assert!(cpu >= 0); // should find a target CPU
        let intno = sh.get(0, 16); // offset 16
        assert_eq!(intno, 16 + 5);

        // Test local disable/enable
        let mut sh2 = SharedIntState::new();
        sh2.init(32, 2);
        sh2.enable(5);
        sh2.local_disable(5, 0);
        sh2.post(5);
        // CPU 0 should not see it
        assert_eq!(sh2.peek(0, 0), -1);
        // CPU 1 should see it
        assert_eq!(sh2.peek(1, 0), 5);

        // Test affinity
        let mut sh3 = SharedIntState::new();
        sh3.init(32, 4);
        sh3.enable(10);
        sh3.set_affinity(10, 2);
        sh3.post(10);
        assert_eq!(sh3.peek(0, 0), -1);
        assert_eq!(sh3.peek(1, 0), -1);
        assert_eq!(sh3.peek(2, 0), 10);
        assert_eq!(sh3.peek(3, 0), -1);

        // Test status bits
        let mut sh4 = SharedIntState::new();
        sh4.init(32, 1);
        assert_eq!(sh4.status(5, 0), 0b010); // only local enable (from init)
        sh4.enable(5);
        assert_eq!(sh4.status(5, 0), 0b110); // global + local
        sh4.post(5);
        assert_eq!(sh4.status(5, 0), 0b111); // pending + global + local

        // Test global disable
        let mut sh5 = SharedIntState::new();
        sh5.init(32, 1);
        sh5.enable(3);
        sh5.post(3);
        assert!(sh5.peek(0, 0) >= 0);
        sh5.disable(3);
        assert_eq!(sh5.peek(0, 0), -1);

        // Test multiple interrupts in priority order
        let mut sh6 = SharedIntState::new();
        sh6.init(64, 1);
        sh6.enable(5);
        sh6.enable(10);
        sh6.enable(33); // second word
        sh6.post(5);
        sh6.post(10);
        sh6.post(33);
        assert_eq!(sh6.get(0, 0), 5);
        assert_eq!(sh6.get(0, 0), 10);
        assert_eq!(sh6.get(0, 0), 33);
        assert_eq!(sh6.get(0, 0), -1);
    }
    debug::write0(b"  [test] shared interrupts OK\n\0");
}
