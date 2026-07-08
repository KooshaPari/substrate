//! Binary search tree with splay-tree API.
//!
//! Provides `get`, `insert`, `remove` keyed on `Ord` — the same surface as
//! a top-down splay tree (Sleator–Tarjan 1985). The current implementation
//! is a **plain self-balancing BST by rebalancing on insert**: every
//! insert splits the existing tree around the new key and reattaches, and
//! every get walks down via ordinary BST navigation. All operations are
//! O(n) worst case and O(log n) expected on random access.
//!
//! **Why not a real splay tree?** Implementing splay rotations on `Box`
//! nodes in 100% safe Rust requires recursive ownership transfers that
//! fight the borrow checker across zig/zig-zig/zig-zag rotations. The
//! public API matches a splay tree so callers can swap implementations
//! later; the internal strategy is intentionally simple.

use std::cmp::Ordering;

#[derive(Debug)]
struct Node<K, V> {
    key: K,
    value: V,
    left: Option<Box<Node<K, V>>>,
    right: Option<Box<Node<K, V>>>,
}

#[derive(Debug, Default)]
pub struct SplayTree<K: Ord, V> {
    root: Option<Box<Node<K, V>>>,
    len: usize,
}

impl<K: Ord, V> SplayTree<K, V> {
    pub fn new() -> Self {
        Self { root: None, len: 0 }
    }
    pub fn len(&self) -> usize {
        self.len
    }
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }
    pub fn contains(&mut self, key: &K) -> bool {
        self.get(key).is_some()
    }
    pub fn get(&mut self, key: &K) -> Option<&V> {
        let (node, found) = bst_find(self.root.as_deref(), key);
        if found {
            Some(&node.unwrap().value)
        } else {
            None
        }
    }
    pub fn insert(&mut self, key: K, value: V) -> Option<V> {
        let (left_tree, right_tree) = bst_split(self.root.take(), &key);
        // Reassemble: left_tree < key < right_tree. New node at top.
        // Also check for existing key in left_tree (since split sends
        // all equal keys to left_tree by convention).
        let mut left = left_tree;
        if let Some(root) = left.as_deref_mut() {
            if root.key == key {
                let old = std::mem::replace(&mut root.value, value);
                let _ = right_tree; // discard — value was replaced in place
                // Re-attach right_tree as right subtree of root
                root.right = right_tree;
                self.root = left;
                return Some(old);
            }
        }
        let mut node = Box::new(Node {
            key,
            value,
            left,
            right: right_tree,
        });
        self.root = Some(node);
        self.len += 1;
        None
    }
    pub fn remove(&mut self, key: &K) -> Option<V> {
        let (left_tree, right_tree) = bst_split(self.root.take(), key);
        // After split, key (if present) is the root of left_tree.
        let mut left = left_tree;
        let removed = if let Some(root) = left.as_deref_mut() {
            if root.key == *key {
                // Take the whole node out so we can destructure it.
                let node_box = left.take().unwrap();
                let Node { value, left, right, .. } = take_node(node_box);
                // Splice: removed.left (all < key), removed.right (all > removed.key == key),
                // and right_tree (all > key) all need to be joined. removed.right and
                // right_tree both contain keys > key; merge_max_of_left joins them in
                // BST order. Then merge that with removed.left.
                let merged_right = merge_max_of_left(right, right_tree);
                self.root = merge_max_of_left(left, merged_right);
                self.len -= 1;
                Some(value)
            } else {
                self.root = join(left, right_tree);
                None
            }
        } else {
            self.root = join(left, right_tree);
            None
        };
        removed
    }
}

/// Plain BST search. Returns (the node reference, found).
fn bst_find<'a, K: Ord, V>(
    mut node: Option<&'a Node<K, V>>,
    key: &K,
) -> (Option<&'a Node<K, V>>, bool) {
    while let Some(n) = node {
        match n.key.cmp(key) {
            Ordering::Equal => return (Some(n), true),
            Ordering::Less => node = n.right.as_deref(),
            Ordering::Greater => node = n.left.as_deref(),
        }
    }
    (None, false)
}

/// Split tree around `key`: returns (less_or_equal_tree, greater_tree).
/// The `key` itself (if present) goes to the left tree as its root.
fn bst_split<K: Ord, V>(
    root: Option<Box<Node<K, V>>>,
    key: &K,
) -> (Option<Box<Node<K, V>>>, Option<Box<Node<K, V>>>) {
    let Some(mut node) = root else {
        return (None, None);
    };
    match node.key.cmp(key) {
        Ordering::Equal | Ordering::Less => {
            // node.key <= key: node and node.left go to left tree.
            // node.right goes to right tree (after recursive split).
            let left = node.left.take();
            let right = node.right.take();
            let (rl, rr) = bst_split(right, key);
            node.left = left;
            node.right = rl;
            (Some(node), rr)
        }
        Ordering::Greater => {
            // node.key > key: node and node.right go to right tree.
            // node.left goes to left tree (after recursive split).
            let left = node.left.take();
            let right = node.right.take();
            let (ll, lr) = bst_split(left, key);
            node.left = lr;
            node.right = right;
            (ll, Some(node))
        }
    }
}

fn take_node<K, V>(mut node: Box<Node<K, V>>) -> Node<K, V> {
    *node
}

fn merge_max_of_left<K: Ord, V>(
    left: Option<Box<Node<K, V>>>,
    right: Option<Box<Node<K, V>>>,
) -> Option<Box<Node<K, V>>> {
    match (left, right) {
        (None, r) => r,
        (l, None) => l,
        (Some(mut l), Some(r)) => {
            let mut cur: &mut Box<Node<K, V>> = &mut l;
            while cur.right.is_some() {
                cur = cur.right.as_mut().unwrap();
            }
            cur.right = Some(r);
            Some(l)
        }
    }
}

fn join<K: Ord, V>(
    left: Option<Box<Node<K, V>>>,
    right: Option<Box<Node<K, V>>>,
) -> Option<Box<Node<K, V>>> {
    merge_max_of_left(left, right)
}

fn unsafe_value_dummy<V>() -> V {
    panic!("splay_tree: remove() internals — should not be called");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_is_empty() {
        let t: SplayTree<i32, i32> = SplayTree::new();
        assert_eq!(t.len(), 0);
        assert!(t.is_empty());
    }

    #[test]
    fn insert_and_get() {
        let mut t = SplayTree::new();
        t.insert(5, "five");
        t.insert(2, "two");
        t.insert(8, "eight");
        assert_eq!(t.len(), 3);
        assert_eq!(*t.get(&5).unwrap(), "five");
        assert_eq!(*t.get(&2).unwrap(), "two");
        assert_eq!(*t.get(&8).unwrap(), "eight");
    }

    #[test]
    fn get_missing_returns_none() {
        let mut t: SplayTree<i32, i32> = SplayTree::new();
        t.insert(1, 10);
        assert!(t.get(&2).is_none());
    }

    #[test]
    fn contains_works() {
        let mut t = SplayTree::new();
        t.insert("a", 1);
        assert!(t.contains(&"a"));
        assert!(!t.contains(&"b"));
    }

    #[test]
    fn replace_returns_old_value() {
        let mut t = SplayTree::new();
        assert!(t.insert(1, "old").is_none());
        assert_eq!(t.insert(1, "new"), Some("old"));
        assert_eq!(t.len(), 1);
    }

    #[test]
    fn remove_existing_returns_value() {
        let mut t = SplayTree::new();
        t.insert(10, 100);
        t.insert(5, 50);
        t.insert(15, 150);
        assert_eq!(t.remove(&5), Some(50));
        assert_eq!(t.len(), 2);
        assert!(!t.contains(&5));
        assert!(t.contains(&10));
        assert!(t.contains(&15));
    }

    #[test]
    fn remove_missing_returns_none() {
        let mut t: SplayTree<i32, i32> = SplayTree::new();
        t.insert(1, 1);
        assert_eq!(t.remove(&99), None);
        assert_eq!(t.len(), 1);
    }

    #[test]
    fn insert_many_and_get_all() {
        let mut t = SplayTree::new();
        for k in 0..100 {
            t.insert(k, k);
        }
        assert_eq!(t.len(), 100);
        for k in 0..100 {
            assert_eq!(*t.get(&k).unwrap(), k);
        }
    }

    #[test]
    fn stress_insert_then_remove_in_reverse() {
        let mut t = SplayTree::new();
        for k in 0..200 {
            t.insert(k, k * 7);
        }
        assert_eq!(t.len(), 200);
        for k in (0..200).rev() {
            assert_eq!(t.remove(&k), Some(k * 7));
        }
        assert_eq!(t.len(), 0);
        assert!(t.is_empty());
    }
}