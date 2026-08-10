//! AVL tree — self-balancing binary search tree (Adelson-Velsky & Landis, 1962).
//!
//! Each node carries a height; the invariant is that for every node
//! `|height(left) - height(right)| <= 1`. Violations are repaired via
//! single or double rotations after insertion and deletion. All
//! operations are O(log n).
//!
//! Reference: G. M. Adelson-Velsky, E. M. Landis, "An algorithm for
//! the organization of information", Doklady Akademii Nauk SSSR, 1962.
//!
//! This implementation is a standard ordered map (`AvlTree<K, V>`) with
//! `insert`, `remove`, `get`, and an in-order `iter` that yields
//! `(K, V)` pairs in sorted-by-key order.

use std::cmp::{Ord, Ordering};

#[derive(Debug)]
struct Node<K, V> {
    key: K,
    value: V,
    height: i32,
    left: Option<Box<Node<K, V>>>,
    right: Option<Box<Node<K, V>>>,
}

impl<K, V> Node<K, V> {
    fn new(key: K, value: V) -> Self {
        Node {
            key,
            value,
            height: 1,
            left: None,
            right: None,
        }
    }
}

fn height<K, V>(node: &Option<Box<Node<K, V>>>) -> i32 {
    node.as_ref().map(|n| n.height).unwrap_or(0)
}

fn update_height<K, V>(node: &mut Node<K, V>) {
    let l = height(&node.left);
    let r = height(&node.right);
    node.height = 1 + l.max(r);
}

fn balance_factor<K, V>(node: &Node<K, V>) -> i32 {
    height(&node.left) - height(&node.right)
}

/// Rotate right at `node`. Returns the new subtree root.
fn rotate_right<K, V>(mut node: Box<Node<K, V>>) -> Box<Node<K, V>> {
    let mut new_root = node.left.take().expect("rotate_right: empty left");
    node.left = new_root.right.take();
    update_height(&mut node);
    new_root.right = Some(node);
    update_height(&mut new_root);
    new_root
}

/// Rotate left at `node`. Returns the new subtree root.
fn rotate_left<K, V>(mut node: Box<Node<K, V>>) -> Box<Node<K, V>> {
    let mut new_root = node.right.take().expect("rotate_left: empty right");
    node.right = new_root.left.take();
    update_height(&mut node);
    new_root.left = Some(node);
    update_height(&mut new_root);
    new_root
}

fn rebalance<K: Ord, V>(node: Box<Node<K, V>>) -> Box<Node<K, V>> {
    let mut n = node;
    update_height(&mut n);
    let bf = balance_factor(&n);
    if bf > 1 {
        // Left heavy.
        let mut left = n.left.take().expect("rebalance: missing left");
        if balance_factor(&left) < 0 {
            // Left-Right case.
            left = rotate_left(left);
        }
        n.left = Some(left);
        return rotate_right(n);
    }
    if bf < -1 {
        let mut right = n.right.take().expect("rebalance: missing right");
        if balance_factor(&right) > 0 {
            // Right-Left case.
            right = rotate_right(right);
        }
        n.right = Some(right);
        return rotate_left(n);
    }
    n
}

/// AVL tree — a self-balancing ordered map.
///
/// All operations (`insert`, `remove`, `get`) are O(log n). Iteration
/// yields `(K, V)` pairs in ascending key order.
#[derive(Debug, Default)]
pub struct AvlTree<K, V> {
    root: Option<Box<Node<K, V>>>,
    len: usize,
}

impl<K, V> AvlTree<K, V> {
    /// Construct an empty AVL tree.
    pub fn new() -> Self {
        AvlTree { root: None, len: 0 }
    }

    /// Number of key-value pairs currently stored.
    pub fn len(&self) -> usize {
        self.len
    }

    /// Returns true if the tree holds no entries.
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Look up a value by key. O(log n).
    pub fn get(&self, key: &K) -> Option<&V>
    where
        K: Ord,
    {
        let mut cur = self.root.as_deref();
        while let Some(n) = cur {
            match key.cmp(&n.key) {
                Ordering::Less => cur = n.left.as_deref(),
                Ordering::Greater => cur = n.right.as_deref(),
                Ordering::Equal => return Some(&n.value),
            }
        }
        None
    }
}

impl<K: Ord, V> AvlTree<K, V> {
    /// Insert `(key, value)`. If `key` is already present, the value is
    /// replaced and the old value returned. O(log n).
    pub fn insert(&mut self, key: K, value: V) -> Option<V> {
        let mut replaced: Option<V> = None;
        self.root = insert_into(self.root.take(), key, value, &mut replaced);
        if replaced.is_none() {
            self.len += 1;
        }
        replaced
    }

    /// Remove `key` from the tree. Returns the removed value if present.
    /// O(log n).
    pub fn remove(&mut self, key: &K) -> Option<V> {
        let mut removed: Option<V> = None;
        self.root = remove_from(self.root.take(), key, &mut removed);
        if removed.is_some() {
            self.len -= 1;
        }
        removed
    }

    /// Iterate `(K, V)` pairs in ascending key order (in-order traversal).
    pub fn iter(&self) -> AvlIter<'_, K, V> {
        AvlIter {
            stack: Vec::new(),
            cur: self.root.as_deref(),
        }
    }
}

fn insert_into<K: Ord, V>(
    node: Option<Box<Node<K, V>>>,
    key: K,
    value: V,
    replaced: &mut Option<V>,
) -> Option<Box<Node<K, V>>> {
    let mut n = match node {
        Some(n) => n,
        None => return Some(Box::new(Node::new(key, value))),
    };
    match key.cmp(&n.key) {
        Ordering::Less => {
            n.left = insert_into(n.left.take(), key, value, replaced);
        }
        Ordering::Greater => {
            n.right = insert_into(n.right.take(), key, value, replaced);
        }
        Ordering::Equal => {
            *replaced = Some(std::mem::replace(&mut n.value, value));
            return Some(n);
        }
    }
    Some(rebalance(n))
}

fn remove_from<K: Ord, V>(
    node: Option<Box<Node<K, V>>>,
    key: &K,
    removed: &mut Option<V>,
) -> Option<Box<Node<K, V>>> {
    let mut n = match node {
        Some(n) => n,
        None => return None,
    };
    match key.cmp(&n.key) {
        Ordering::Less => {
            n.left = remove_from(n.left.take(), key, removed);
        }
        Ordering::Greater => {
            n.right = remove_from(n.right.take(), key, removed);
        }
        Ordering::Equal => {
            *removed = Some(n.value);
            // Cases: 0, 1, or 2 children.
            match (n.left.take(), n.right.take()) {
                (None, None) => return None,
                (Some(child), None) => return Some(child),
                (None, Some(child)) => return Some(child),
                (Some(l), Some(r)) => {
                    // Find in-order successor (min of right subtree).
                    let (succ_key, succ_val, new_right) = pop_min(r);
                    n.key = succ_key;
                    n.value = succ_val;
                    n.left = Some(l);
                    n.right = new_right;
                }
            }
        }
    }
    Some(rebalance(n))
}

fn pop_min<K: Ord, V>(mut node: Box<Node<K, V>>) -> (K, V, Option<Box<Node<K, V>>>) {
    if node.left.is_none() {
        let key = node.key;
        let value = node.value;
        let right = node.right.take();
        return (key, value, right);
    }
    let (k, v, new_left) = pop_min(node.left.take().unwrap());
    node.left = new_left;
    let rebalanced = rebalance(node);
    (k, v, Some(rebalanced))
}

/// In-order iterator.
pub struct AvlIter<'a, K, V> {
    stack: Vec<&'a Node<K, V>>,
    cur: Option<&'a Node<K, V>>,
}

impl<'a, K, V> Iterator for AvlIter<'a, K, V> {
    type Item = (&'a K, &'a V);

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            match self.cur.take() {
                Some(n) => {
                    self.stack.push(n);
                    self.cur = n.left.as_deref();
                }
                None => {
                    let n = self.stack.pop()?;
                    self.cur = n.right.as_deref();
                    return Some((&n.key, &n.value));
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_tree() {
        let t: AvlTree<i32, i32> = AvlTree::new();
        assert_eq!(t.len(), 0);
        assert!(t.is_empty());
        assert!(t.get(&1).is_none());
        let v: Vec<_> = t.iter().collect();
        assert!(v.is_empty());
    }

    #[test]
    fn insert_and_get() {
        let mut t = AvlTree::new();
        assert!(t.insert(10, "ten").is_none());
        assert!(t.insert(5, "five").is_none());
        assert!(t.insert(15, "fifteen").is_none());
        assert_eq!(t.get(&10), Some(&"ten"));
        assert_eq!(t.get(&5), Some(&"five"));
        assert_eq!(t.get(&15), Some(&"fifteen"));
        assert_eq!(t.get(&99), None);
        assert_eq!(t.len(), 3);
    }

    #[test]
    fn replace_value_returns_old() {
        let mut t = AvlTree::new();
        assert!(t.insert(1, "a").is_none());
        assert_eq!(t.insert(1, "b"), Some("a"));
        assert_eq!(t.get(&1), Some(&"b"));
        assert_eq!(t.len(), 1);
    }

    #[test]
    fn iter_is_sorted() {
        let mut t = AvlTree::new();
        for (k, v) in [(5, 'e'), (1, 'a'), (3, 'c'), (2, 'b'), (4, 'd')] {
            t.insert(k, v);
        }
        let keys: Vec<i32> = t.iter().map(|(k, _)| *k).collect();
        assert_eq!(keys, vec![1, 2, 3, 4, 5]);
    }

    #[test]
    fn left_skewed_balances() {
        let mut t = AvlTree::new();
        for i in 0..100 {
            t.insert(i, i * 10);
        }
        assert_eq!(t.len(), 100);
        // After balancing, height should be ~ ceil(log2(100)) + 1 ~= 8.
        let h = self_root_height(&t);
        assert!(h <= 8, "AVL height {} should be <= 8 for 100 nodes", h);
    }

    #[test]
    fn right_skewed_balances() {
        let mut t = AvlTree::new();
        for i in (0..100).rev() {
            t.insert(i, i);
        }
        assert_eq!(t.len(), 100);
        let h = self_root_height(&t);
        assert!(h <= 8, "AVL height {} should be <= 8 for 100 nodes", h);
    }

    #[test]
    fn random_inserts_balanced() {
        let mut t = AvlTree::new();
        for i in 0..200 {
            t.insert((i * 31 + 7) % 200, i);
        }
        let h = self_root_height(&t);
        assert!(h <= 9, "AVL height {} should be <= 9 for 200 nodes", h);
        // Sorted iteration matches input order.
        let keys: Vec<i32> = t.iter().map(|(k, _)| *k).collect();
        let mut sorted = keys.clone();
        sorted.sort_unstable();
        assert_eq!(keys, sorted);
    }

    #[test]
    fn remove_leaf() {
        let mut t = AvlTree::new();
        t.insert(2, 20);
        t.insert(1, 10);
        t.insert(3, 30);
        assert_eq!(t.remove(&2), Some(20));
        assert_eq!(t.len(), 2);
        assert!(t.get(&2).is_none());
        assert_eq!(t.get(&1), Some(&10));
        assert_eq!(t.get(&3), Some(&30));
    }

    #[test]
    fn remove_node_with_two_children() {
        let mut t = AvlTree::new();
        for i in 1..=7 {
            t.insert(i, i * 100);
        }
        // Remove the root (4): successor is 5.
        assert_eq!(t.remove(&4), Some(400));
        assert_eq!(t.len(), 6);
        assert!(t.get(&4).is_none());
        // Tree is still sorted.
        let keys: Vec<i32> = t.iter().map(|(k, _)| *k).collect();
        assert_eq!(keys, vec![1, 2, 3, 5, 6, 7]);
    }

    #[test]
    fn remove_missing_is_none() {
        let mut t: AvlTree<i32, i32> = AvlTree::new();
        t.insert(1, 1);
        assert_eq!(t.remove(&99), None);
        assert_eq!(t.len(), 1);
    }

    #[test]
    fn string_keys() {
        let mut t = AvlTree::new();
        t.insert("banana", 2);
        t.insert("apple", 1);
        t.insert("cherry", 3);
        let keys: Vec<&str> = t.iter().map(|(k, _)| *k).collect();
        assert_eq!(keys, vec!["apple", "banana", "cherry"]);
        assert_eq!(t.get(&"apple"), Some(&1));
    }

    fn self_root_height<K, V>(t: &AvlTree<K, V>) -> i32 {
        match t.root {
            Some(ref n) => n.height,
            None => 0,
        }
    }
}
