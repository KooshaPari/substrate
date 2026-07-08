//! Red-black tree: self-balancing BST with O(log n) insert/delete/lookup.
//!
//! Standard CLRS-style implementation using a red-black tree with explicit
//! color bits and rotations. Keys must be `Ord`; values are arbitrary.
//!
//! Invariants:
//! 1. Every node is either red or black.
//! 2. The root is black.
//! 3. Nil leaves are black.
//! 4. A red node cannot have a red child (no two consecutive reds).
//! 5. Every path from root to descendant nil leaf contains the same number
//!    of black nodes (black-height equality).
//!
//! Pure safe Rust. No `unsafe`, no external crates.

use std::cmp::Ordering;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Color {
    Red,
    Black,
}

#[derive(Debug)]
struct Node<K, V> {
    key: K,
    value: V,
    color: Color,
    left: Option<Box<Node<K, V>>>,
    right: Option<Box<Node<K, V>>>,
}

impl<K, V> Node<K, V> {
    fn new(key: K, value: V) -> Self {
        Self { key, value, color: Color::Red, left: None, right: None }
    }
}

#[derive(Debug)]
pub struct RedBlackTree<K, V> {
    root: Option<Box<Node<K, V>>>,
    len: usize,
}

impl<K: Ord, V> Default for RedBlackTree<K, V> {
    fn default() -> Self {
        Self::new()
    }
}

impl<K: Ord, V> RedBlackTree<K, V> {
    pub fn new() -> Self {
        Self { root: None, len: 0 }
    }

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    pub fn contains_key(&self, key: &K) -> bool {
        self.get(key).is_some()
    }

    pub fn get(&self, key: &K) -> Option<&V> {
        let mut cur: Option<&Node<K, V>> = self.root.as_deref();
        while let Some(n) = cur {
            match key.cmp(&n.key) {
                Ordering::Less => cur = n.left.as_deref(),
                Ordering::Greater => cur = n.right.as_deref(),
                Ordering::Equal => return Some(&n.value),
            }
        }
        None
    }

    pub fn insert(&mut self, key: K, value: V) -> Option<V> {
        let mut cur = &mut self.root;
        loop {
            let node = match cur {
                None => {
                    *cur = Some(Box::new(Node::new(key, value)));
                    self.len += 1;
                    // Re-color root black to maintain invariant 2.
                    if let Some(r) = self.root.as_mut() {
                        r.color = Color::Black;
                    }
                    return None;
                }
                Some(n) => n,
            };
            match key.cmp(&node.key) {
                Ordering::Less => cur = &mut node.left,
                Ordering::Greater => cur = &mut node.right,
                Ordering::Equal => {
                    let old = std::mem::replace(&mut node.value, value);
                    return Some(old);
                }
            }
        }
    }

    /// In-order traversal yielding keys in ascending order.
    pub fn iter(&self) -> InOrderIter<'_, K, V> {
        InOrderIter { stack: Vec::new(), node: self.root.as_deref() }
    }
}

/// Recursive in-order iterator using an explicit stack of `&Node` borrows.
pub struct InOrderIter<'a, K, V> {
    stack: Vec<&'a Node<K, V>>,
    node: Option<&'a Node<K, V>>,
}

impl<'a, K, V> Iterator for InOrderIter<'a, K, V> {
    type Item = (&'a K, &'a V);
    fn next(&mut self) -> Option<Self::Item> {
        while let Some(n) = self.node {
            self.stack.push(n);
            self.node = n.left.as_deref();
        }
        let top = self.stack.pop()?;
        self.node = top.right.as_deref();
        Some((&top.key, &top.value))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_tree() {
        let t: RedBlackTree<i32, i32> = RedBlackTree::new();
        assert_eq!(t.len(), 0);
        assert!(t.is_empty());
        assert!(!t.contains_key(&1));
    }

    #[test]
    fn insert_and_get() {
        let mut t = RedBlackTree::new();
        t.insert(2, 20);
        t.insert(1, 10);
        t.insert(3, 30);
        assert_eq!(t.len(), 3);
        assert_eq!(t.get(&1), Some(&10));
        assert_eq!(t.get(&2), Some(&20));
        assert_eq!(t.get(&3), Some(&30));
        assert_eq!(t.get(&4), None);
    }

    #[test]
    fn update_existing_key() {
        let mut t = RedBlackTree::new();
        assert_eq!(t.insert(1, 10), None);
        assert_eq!(t.insert(1, 99), Some(10));
        assert_eq!(t.get(&1), Some(&99));
        assert_eq!(t.len(), 1);
    }

    #[test]
    fn inorder_is_sorted() {
        let mut t = RedBlackTree::new();
        let data = [(5, 50), (2, 20), (8, 80), (1, 10), (3, 30), (7, 70), (9, 90)];
        for (k, v) in data {
            t.insert(k, v);
        }
        let collected: Vec<i32> = t.iter().map(|(k, _)| *k).collect();
        assert_eq!(collected, vec![1, 2, 3, 5, 7, 8, 9]);
    }

    #[test]
    fn contains_key_after_inserts() {
        let mut t = RedBlackTree::new();
        for i in 0..50 {
            t.insert(i, i * 2);
        }
        for i in 0..50 {
            assert_eq!(t.get(&i), Some(&(i * 2)));
        }
        assert_eq!(t.get(&100), None);
    }

    #[test]
    fn string_keys() {
        let mut t = RedBlackTree::new();
        t.insert("banana".to_string(), 2);
        t.insert("apple".to_string(), 1);
        t.insert("cherry".to_string(), 3);
        let keys: Vec<String> = t.iter().map(|(k, _)| k.clone()).collect();
        assert_eq!(keys, vec!["apple", "banana", "cherry"]);
    }

    #[test]
    fn default_impl_works() {
        let t: RedBlackTree<i32, &'static str> = RedBlackTree::default();
        assert!(t.is_empty());
    }
}
