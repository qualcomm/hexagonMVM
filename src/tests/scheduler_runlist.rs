/*
 * Copyright (c) Qualcomm Technologies, Inc. and/or its subsidiaries.
 * SPDX-License-Identifier: BSD-3-Clause-Clear
 */

use crate::debug;
use minivm_sched::context::ThreadContext;
use minivm_sched::lowprio::LowPrio;
use minivm_sched::readylist::IDX_NONE;
use minivm_sched::runlist::RunList;

pub fn run() {
    debug::write0(b"  [test] scheduler runlist\n\0");
    {
        let mut rl = RunList::new(4);
        assert_eq!(rl.hthreads, 4);

        // All entries should start empty (IDX_NONE=0, prio=-1)
        for i in 0..4 {
            assert_eq!(rl.get(i), IDX_NONE);
            assert_eq!(rl.get_prio(i), -1);
        }

        // worst_prio should be MAX_PRIOS when nothing running
        assert_eq!(rl.worst_prio(), minivm_types::consts::MAX_PRIOS);
        assert_eq!(rl.worst_prio_hthread(), u32::MAX);

        // Push threads onto hardware threads
        let mut threads2: [ThreadContext; 4] = [
            ThreadContext::zeroed(),
            ThreadContext::zeroed(),
            ThreadContext::zeroed(),
            ThreadContext::zeroed(),
        ];
        threads2[1].hthread = 0;
        threads2[1].prio = 10;
        threads2[2].hthread = 1;
        threads2[2].prio = 50;
        threads2[3].hthread = 2;
        threads2[3].prio = 30;

        rl.push(&mut threads2, 1);
        rl.push(&mut threads2, 2);
        rl.push(&mut threads2, 3);

        assert_eq!(rl.get(0), 1);
        assert_eq!(rl.get_prio(0), 10);
        assert_eq!(rl.worst_prio(), 50);
        assert_eq!(rl.worst_prio_hthread(), 1);

        // Remove worst
        rl.remove(&threads2, 2);
        assert_eq!(rl.get(1), IDX_NONE);
        assert_eq!(rl.worst_prio(), 30);

        // LowPrio tests
        let mut lp = LowPrio::new();
        assert!(!lp.is_low(0));
        assert!(!lp.is_low(1));

        lp.mark_low(2);
        assert!(lp.is_low(2));
        assert!(!lp.is_low(1));

        lp.unmark_low(2);
        assert!(!lp.is_low(2));

        // Test raise
        lp.mark_low(3);
        let ht = lp.raise();
        assert_eq!(ht, Some(3));
        assert_eq!(lp.priomask, 0);

        // raise with wait set returns None
        lp.mark_low(1);
        lp.mark_wait(0);
        assert_eq!(lp.raise(), None);
        lp.unmark_wait(0);
        assert_eq!(lp.raise(), Some(1));
    }
    debug::write0(b"  [test] scheduler runlist OK\n\0");
}
