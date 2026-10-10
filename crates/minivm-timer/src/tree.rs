/*
 * Copyright (c) Qualcomm Technologies, Inc. and/or its subsidiaries.
 * SPDX-License-Identifier: BSD-3-Clause-Clear
 */

//! Binary search tree for timeout management.
//!
//! The tree stores timeout nodes keyed by expiration time (ticks).
//! Key operations:
//! - `add`: Insert a node by key
//! - `remove`: Remove a node by key
//! - `bisect`: Split tree into expired (≤ key) and pending (> key) subtrees
//! - `destructive_iterate`: In-order traversal with callback, destroys tree
//! - `min`: Find minimum key node (soonest timeout)

/// Index of a tree node, or `IDX_NONE` for null.
pub type TreeIdx = u32;

/// Sentinel value for "no node".
pub const IDX_NONE: TreeIdx = 0;

/// A node in the timeout tree.
///
/// Each thread context embeds a TreeNode for its timeout.
#[derive(Clone, Copy, Debug, Default)]
pub struct TreeNode {
    pub left: TreeIdx,
    pub right: TreeIdx,
    pub key: u64,
}

/// A timeout tree (BST) keyed by expiration ticks.
///
/// Uses index-based nodes stored in an external array.
/// The `root` field is the index of the root node, or `IDX_NONE`.
#[derive(Debug, Default)]
pub struct TimeoutTree {
    pub root: TreeIdx,
}

impl TimeoutTree {
    pub const fn new() -> Self {
        Self { root: IDX_NONE }
    }

    /// Insert a node into the tree with the given key.
    pub fn add(&mut self, nodes: &mut [TreeNode], idx: TreeIdx, key: u64) {
        nodes[idx as usize].key = key;
        nodes[idx as usize].left = IDX_NONE;
        nodes[idx as usize].right = IDX_NONE;
        self.root = Self::insert(nodes, self.root, idx);
    }

    fn insert(nodes: &mut [TreeNode], root: TreeIdx, idx: TreeIdx) -> TreeIdx {
        if root == IDX_NONE {
            return idx;
        }
        if nodes[idx as usize].key <= nodes[root as usize].key {
            let left = nodes[root as usize].left;
            nodes[root as usize].left = Self::insert(nodes, left, idx);
        } else {
            let right = nodes[root as usize].right;
            nodes[root as usize].right = Self::insert(nodes, right, idx);
        }
        root
    }

    /// Remove a node with the given key from the tree.
    pub fn remove(&mut self, nodes: &mut [TreeNode], idx: TreeIdx, key: u64) {
        self.root = Self::remove_impl(nodes, self.root, idx, key);
    }

    fn remove_impl(nodes: &mut [TreeNode], root: TreeIdx, idx: TreeIdx, key: u64) -> TreeIdx {
        if root == IDX_NONE {
            return IDX_NONE;
        }
        if root == idx {
            // Found the node to remove. Promote left, insert right subtree.
            let left = nodes[root as usize].left;
            let right = nodes[root as usize].right;
            nodes[root as usize].left = IDX_NONE;
            nodes[root as usize].right = IDX_NONE;
            if right == IDX_NONE {
                return left;
            }
            if left == IDX_NONE {
                return right;
            }
            // Insert right subtree (intact, with its children) into left
            return Self::insert(nodes, left, right);
        }
        if key <= nodes[root as usize].key {
            let left = nodes[root as usize].left;
            nodes[root as usize].left = Self::remove_impl(nodes, left, idx, key);
        } else {
            let right = nodes[root as usize].right;
            nodes[root as usize].right = Self::remove_impl(nodes, right, idx, key);
        }
        root
    }

    /// Split the tree into two subtrees:
    /// - `le_tree`: nodes with key ≤ split_key (expired)
    /// - `gt_tree`: nodes with key > split_key (pending)
    ///
    /// Returns (le_tree, gt_tree).
    pub fn bisect(&mut self, nodes: &mut [TreeNode], split_key: u64) -> (TimeoutTree, TimeoutTree) {
        let mut le = TimeoutTree::new();
        let mut gt = TimeoutTree::new();
        let root = self.root;
        self.root = IDX_NONE;
        Self::bisect_impl(nodes, root, split_key, &mut le, &mut gt);
        (le, gt)
    }

    /// Efficient O(n) top-down "unzip" split.
    ///
    /// Splits the BST in a single pass: when a node's key <= split_key,
    /// the node (and its entire left subtree) goes to `le`, and we continue
    /// splitting its right subtree. Vice versa for nodes > split_key.
    fn bisect_impl(
        nodes: &mut [TreeNode],
        mut cur: TreeIdx,
        split_key: u64,
        le: &mut TimeoutTree,
        gt: &mut TimeoutTree,
    ) {
        // le_attach / gt_attach track which node's child pointer to update next.
        // `None` means update the tree root; `Some((idx, is_right))` means update
        // nodes[idx].right (if is_right) or nodes[idx].left.
        let mut le_attach: Option<(usize, bool)> = None;
        let mut gt_attach: Option<(usize, bool)> = None;

        while cur != IDX_NONE {
            if nodes[cur as usize].key <= split_key {
                // This node (and its left subtree) go to le
                match le_attach {
                    None => le.root = cur,
                    Some((idx, true)) => nodes[idx].right = cur,
                    Some((idx, false)) => nodes[idx].left = cur,
                }
                // Continue splitting the right subtree
                let right = nodes[cur as usize].right;
                nodes[cur as usize].right = IDX_NONE;
                le_attach = Some((cur as usize, true)); // next le node attaches to cur.right
                cur = right;
            } else {
                // This node (and its right subtree) go to gt
                match gt_attach {
                    None => gt.root = cur,
                    Some((idx, true)) => nodes[idx].right = cur,
                    Some((idx, false)) => nodes[idx].left = cur,
                }
                // Continue splitting the left subtree
                let left = nodes[cur as usize].left;
                nodes[cur as usize].left = IDX_NONE;
                gt_attach = Some((cur as usize, false)); // next gt node attaches to cur.left
                cur = left;
            }
        }
    }

    /// Find the minimum key node (soonest timeout).
    ///
    /// Returns the node index, or `IDX_NONE` if empty.
    pub fn min(&self, nodes: &[TreeNode]) -> TreeIdx {
        let mut cur = self.root;
        if cur == IDX_NONE {
            return IDX_NONE;
        }
        while nodes[cur as usize].left != IDX_NONE {
            cur = nodes[cur as usize].left;
        }
        cur
    }

    /// Iterate all nodes in-order, collecting indices.
    ///
    /// This destroys the tree (sets root to IDX_NONE after iteration).
    pub fn collect_and_clear(&mut self, nodes: &[TreeNode], out: &mut impl FnMut(TreeIdx)) {
        Self::iterate_inorder(nodes, self.root, out);
        self.root = IDX_NONE;
    }

    fn iterate_inorder(nodes: &[TreeNode], root: TreeIdx, out: &mut impl FnMut(TreeIdx)) {
        if root == IDX_NONE {
            return;
        }
        Self::iterate_inorder(nodes, nodes[root as usize].left, out);
        out(root);
        Self::iterate_inorder(nodes, nodes[root as usize].right, out);
    }

    /// Check if the tree is empty.
    pub fn is_empty(&self) -> bool {
        self.root == IDX_NONE
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec::Vec;

    fn make_nodes(n: usize) -> Vec<TreeNode> {
        let mut v = Vec::new();
        v.resize(n, TreeNode::default());
        v
    }

    #[test]
    fn test_empty_tree() {
        let tree = TimeoutTree::new();
        let nodes = make_nodes(4);
        assert!(tree.is_empty());
        assert_eq!(tree.min(&nodes), IDX_NONE);
    }

    #[test]
    fn test_add_and_min() {
        let mut tree = TimeoutTree::new();
        let mut nodes = make_nodes(8);

        tree.add(&mut nodes, 1, 100);
        tree.add(&mut nodes, 2, 50);
        tree.add(&mut nodes, 3, 200);
        tree.add(&mut nodes, 4, 75);

        assert_eq!(tree.min(&nodes), 2); // key=50 is min
    }

    #[test]
    fn test_remove() {
        let mut tree = TimeoutTree::new();
        let mut nodes = make_nodes(8);

        tree.add(&mut nodes, 1, 100);
        tree.add(&mut nodes, 2, 50);
        tree.add(&mut nodes, 3, 200);

        // Remove min
        tree.remove(&mut nodes, 2, 50);
        assert_eq!(tree.min(&nodes), 1); // key=100 is now min

        // Remove root
        tree.remove(&mut nodes, 1, 100);
        assert_eq!(tree.min(&nodes), 3); // only key=200 left

        // Remove last
        tree.remove(&mut nodes, 3, 200);
        assert!(tree.is_empty());
    }

    #[test]
    fn test_bisect() {
        let mut tree = TimeoutTree::new();
        let mut nodes = make_nodes(8);

        tree.add(&mut nodes, 1, 10);
        tree.add(&mut nodes, 2, 20);
        tree.add(&mut nodes, 3, 30);
        tree.add(&mut nodes, 4, 40);
        tree.add(&mut nodes, 5, 50);

        // Split at key=25: le={10,20}, gt={30,40,50}
        let (mut le, gt) = tree.bisect(&mut nodes, 25);
        assert!(tree.is_empty());

        // Check le
        let mut le_items = Vec::new();
        le.collect_and_clear(&nodes, &mut |idx| le_items.push(idx));
        assert_eq!(le_items.len(), 2);
        // Values should be 10 and 20 (in order)
        assert!(le_items.iter().all(|&i| nodes[i as usize].key <= 25));

        // Check gt
        assert_eq!(gt.min(&nodes), 3); // key=30 is min of gt
    }

    #[test]
    fn test_collect_and_clear() {
        let mut tree = TimeoutTree::new();
        let mut nodes = make_nodes(8);

        tree.add(&mut nodes, 3, 30);
        tree.add(&mut nodes, 1, 10);
        tree.add(&mut nodes, 2, 20);

        let mut items = Vec::new();
        tree.collect_and_clear(&nodes, &mut |idx| items.push(idx));

        // Should be in-order by key
        assert_eq!(items.len(), 3);
        assert_eq!(nodes[items[0] as usize].key, 10);
        assert_eq!(nodes[items[1] as usize].key, 20);
        assert_eq!(nodes[items[2] as usize].key, 30);
        assert!(tree.is_empty());
    }

    #[test]
    fn test_duplicate_keys() {
        let mut tree = TimeoutTree::new();
        let mut nodes = make_nodes(8);

        tree.add(&mut nodes, 1, 100);
        tree.add(&mut nodes, 2, 100);
        tree.add(&mut nodes, 3, 100);

        // All three should be present
        let mut items = Vec::new();
        tree.collect_and_clear(&nodes, &mut |idx| items.push(idx));
        assert_eq!(items.len(), 3);
    }

    #[test]
    fn test_bisect_all_expired() {
        let mut tree = TimeoutTree::new();
        let mut nodes = make_nodes(8);

        tree.add(&mut nodes, 1, 10);
        tree.add(&mut nodes, 2, 20);

        let (mut le, gt) = tree.bisect(&mut nodes, 100);

        let mut le_items = Vec::new();
        le.collect_and_clear(&nodes, &mut |idx| le_items.push(idx));
        assert_eq!(le_items.len(), 2);
        assert!(gt.is_empty());
    }

    #[test]
    fn test_bisect_none_expired() {
        let mut tree = TimeoutTree::new();
        let mut nodes = make_nodes(8);

        tree.add(&mut nodes, 1, 100);
        tree.add(&mut nodes, 2, 200);

        let (le, gt) = tree.bisect(&mut nodes, 50);

        assert!(le.is_empty());
        assert!(!gt.is_empty());
        assert_eq!(gt.min(&nodes), 1); // key=100
    }
}
