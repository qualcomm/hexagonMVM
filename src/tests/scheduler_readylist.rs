/*
 * Copyright (c) Qualcomm Technologies, Inc. and/or its subsidiaries.
 * SPDX-License-Identifier: BSD-3-Clause-Clear
 */

use crate::debug;
use minivm_sched::context::ThreadContext;
use minivm_sched::readylist::{ReadyList, IDX_NONE};

pub fn run() {
    debug::write0(b"  [test] scheduler readylist\n\0");
    {
        // Allocate thread contexts on the stack (index 0 is null sentinel)
        let mut threads: [ThreadContext; 8] = [
            ThreadContext::zeroed(),
            ThreadContext::zeroed(),
            ThreadContext::zeroed(),
            ThreadContext::zeroed(),
            ThreadContext::zeroed(),
            ThreadContext::zeroed(),
            ThreadContext::zeroed(),
            ThreadContext::zeroed(),
        ];

        let mut rl = ReadyList::new();
        assert!(!rl.any_valid());
        assert_eq!(rl.best_prio(), minivm_types::consts::MAX_PRIOS);
        assert_eq!(rl.getbest(&mut threads), IDX_NONE);

        // Add threads at different priorities
        threads[1].prio = 50;
        threads[2].prio = 10;
        threads[3].prio = 30;
        rl.append(&mut threads, 1);
        rl.append(&mut threads, 2);
        rl.append(&mut threads, 3);

        assert!(rl.any_valid());
        assert_eq!(rl.best_prio(), 10);
        assert!(rl.prio_valid(10));
        assert!(rl.prio_valid(30));
        assert!(rl.prio_valid(50));
        assert!(!rl.prio_valid(9));

        // getbest returns highest priority (lowest number) first
        let best = rl.getbest(&mut threads);
        assert_eq!(best, 2); // prio 10
        assert_eq!(rl.best_prio(), 30);

        let best = rl.getbest(&mut threads);
        assert_eq!(best, 3); // prio 30

        let best = rl.getbest(&mut threads);
        assert_eq!(best, 1); // prio 50

        assert!(!rl.any_valid());
        assert_eq!(rl.getbest(&mut threads), IDX_NONE);

        // Test same-priority FIFO ordering
        threads[4].prio = 5;
        threads[5].prio = 5;
        threads[6].prio = 5;
        rl.append(&mut threads, 4);
        rl.append(&mut threads, 5);
        rl.append(&mut threads, 6);
        assert_eq!(rl.getbest(&mut threads), 4); // FIFO
        assert_eq!(rl.getbest(&mut threads), 5);
        assert_eq!(rl.getbest(&mut threads), 6);

        // Test insert (front of ring) vs append (back)
        threads[1].prio = 20;
        threads[2].prio = 20;
        threads[3].prio = 20;
        rl.append(&mut threads, 1);
        rl.append(&mut threads, 2);
        rl.insert(&mut threads, 3); // at front
        assert_eq!(rl.getbest(&mut threads), 3); // front first
        assert_eq!(rl.getbest(&mut threads), 1);
        assert_eq!(rl.getbest(&mut threads), 2);

        // Test remove from middle
        threads[1].prio = 15;
        threads[2].prio = 15;
        threads[3].prio = 15;
        rl.append(&mut threads, 1);
        rl.append(&mut threads, 2);
        rl.append(&mut threads, 3);
        rl.remove(&mut threads, 2); // remove middle
        assert_eq!(rl.getbest(&mut threads), 1);
        assert_eq!(rl.getbest(&mut threads), 3);
        assert_eq!(rl.getbest(&mut threads), IDX_NONE);

        // Test remove head
        threads[1].prio = 25;
        threads[2].prio = 25;
        rl.append(&mut threads, 1);
        rl.append(&mut threads, 2);
        rl.remove(&mut threads, 1); // remove head
        assert_eq!(rl.getbest(&mut threads), 2);

        // Test remove last element clears validity
        threads[1].prio = 99;
        rl.append(&mut threads, 1);
        assert!(rl.prio_valid(99));
        rl.remove(&mut threads, 1);
        assert!(!rl.prio_valid(99));
        assert!(!rl.any_valid());
    }
    debug::write0(b"  [test] scheduler readylist OK\n\0");
}
