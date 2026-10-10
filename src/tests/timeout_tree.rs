/*
 * Copyright (c) Qualcomm Technologies, Inc. and/or its subsidiaries.
 * SPDX-License-Identifier: BSD-3-Clause-Clear
 */

use crate::debug;
use minivm_timer::tree::{TimeoutTree, TreeNode, IDX_NONE};

pub fn run() {
    debug::write0(b"  [test] timeout tree\n\0");
    {
        let mut nodes: [TreeNode; 8] = [TreeNode::default(); 8];
        let mut tree = TimeoutTree::new();
        assert!(tree.is_empty());
        assert_eq!(tree.min(&nodes), IDX_NONE);

        // Add nodes with different keys
        tree.add(&mut nodes, 1, 100);
        tree.add(&mut nodes, 2, 50);
        tree.add(&mut nodes, 3, 200);
        tree.add(&mut nodes, 4, 75);

        assert!(!tree.is_empty());
        assert_eq!(tree.min(&nodes), 2); // key=50 is min

        // Remove the minimum
        tree.remove(&mut nodes, 2, 50);
        assert_eq!(tree.min(&nodes), 4); // key=75 is new min

        // Remove root-ish node
        tree.remove(&mut nodes, 1, 100);
        assert_eq!(tree.min(&nodes), 4); // key=75 still min

        // Remove all
        tree.remove(&mut nodes, 4, 75);
        tree.remove(&mut nodes, 3, 200);
        assert!(tree.is_empty());

        // Test bisect: split into expired vs pending
        tree.add(&mut nodes, 1, 10);
        tree.add(&mut nodes, 2, 20);
        tree.add(&mut nodes, 3, 30);
        tree.add(&mut nodes, 4, 40);
        tree.add(&mut nodes, 5, 50);

        // Split at key=25: le={10,20}, gt={30,40,50}
        let (le, gt) = tree.bisect(&mut nodes, 25);
        assert!(tree.is_empty()); // original tree emptied

        // le tree has keys <= 25
        assert!(!le.is_empty());
        let min_le = le.min(&nodes);
        assert!(nodes[min_le as usize].key <= 25);

        // gt tree has keys > 25
        assert!(!gt.is_empty());
        assert_eq!(gt.min(&nodes), 3); // key=30 is min of gt

        // Test bisect with all expired
        let mut tree2 = TimeoutTree::new();
        tree2.add(&mut nodes, 6, 5);
        tree2.add(&mut nodes, 7, 15);
        let (le2, gt2) = tree2.bisect(&mut nodes, 100);
        assert!(!le2.is_empty());
        assert!(gt2.is_empty());
    }
    debug::write0(b"  [test] timeout tree OK\n\0");
}
