//! Probabilistic balanced key/value store: a real skip list.
//!
//! Pugh's skip list (1990) — multiple parallel linked lists, each level
//! skipping over exponentially more elements. Search/insert/delete all
//! run in expected O(log n) time. Simpler than red-black trees and
//! uses less code.
//!
//! Reference: William Pugh, "Skip Lists: A Probabilistic Alternative to
//! Balanced Trees" (CACM 1990).

use std::cmp::Ordering;
use std::time::{SystemTime, UNIX_EPOCH};

const MAX_LEVEL: usize = 16;
const PROMOTION_PROB: f64 = 0.5;

/// A single node in the skip list.
struct Node<K, V> {
    key: K,
    value: V,
    /// Forward pointers, indexed by level. `forward[0]` is the bottom
    /// (full) level, `forward[level-1]` is the top.
    forward: Vec<Option<usize>>,
}

/// A skip list mapping `K: Ord` to `V`.
pub struct SkipList<K, V> {
    /// Indexed nodes (index 0 is the head sentinel, indices 1.. are real nodes).
    /// The head sentinel shares the same `Node` struct but its key/value are
    /// unused; they're stored as `Option<Node>` only to enable the same
    /// forward-pointer layout. We hide the head with a wrapper.
    nodes: Vec<Option<Node<K, V>>>,
    head_forward: Vec<Option<usize>>,
    level: usize, // current top level (1..=MAX_LEVEL)
    rng: u64,
}

impl<K: Ord, V> SkipList<K, V> {
    /// Create an empty skip list with a time-derived seed.
    pub fn new() -> Self {
        Self::with_seed(seed_from_time())
    }

    /// Create an empty skip list with the given seed (for reproducible tests).
    pub fn with_seed(seed: u64) -> Self {
        // Allocate an empty slot for the head sentinel at index 0.
        // We store head-forward separately so we don't need a key/value
        // sentinel that would require `unsafe` or `Default`.
        Self {
            nodes: vec![None],
            head_forward: vec![None; MAX_LEVEL],
            level: 1,
            rng: seed.wrapping_add(1) | 1,
        }
    }

    fn head(&self) -> SkipListHead<'_, K, V> {
        SkipListHead {
            forward: &self.head_forward,
            _phantom: std::marker::PhantomData,
        }
    }

    fn head_mut(&mut self) -> SkipListHeadMut<'_, K, V> {
        SkipListHeadMut {
            forward: &mut self.head_forward,
            _phantom: std::marker::PhantomData,
        }
    }

    /// Number of stored key/value pairs.
    pub fn len(&self) -> usize {
        self.nodes.iter().filter(|n| n.is_some()).count()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    fn node_forward(&self, id: usize) -> &Vec<Option<usize>> {
        &self.nodes[id].as_ref().expect("non-head slot").forward
    }

    fn node_forward_mut(&mut self, id: usize) -> &mut Vec<Option<usize>> {
        &mut self.nodes[id]
            .as_mut()
            .expect("non-head slot")
            .forward
    }

    /// Look up `key`, returning `Some(&V)` if present.
    pub fn get(&self, key: &K) -> Option<&V> {
        let mut cur = 0; // index 0 = head sentinel
        for lvl in (0..self.level).rev() {
            while let Some(next) = self.forward_at(cur, lvl) {
                match self.nodes[next].as_ref()?.key.cmp(key) {
                    Ordering::Less => cur = next,
                    _ => break,
                }
            }
        }
        if let Some(next) = self.forward_at(cur, 0) {
            if self.nodes[next].as_ref()?.key == *key {
                return self.nodes[next].as_ref().map(|n| &n.value);
            }
        }
        None
    }

    /// Insert (key, value). If `key` already exists, the value is
    /// replaced and the old value returned.
    pub fn insert(&mut self, key: K, value: V) -> Option<V> {
        let mut cur = 0usize;
        let mut update: Vec<usize> = vec![0; MAX_LEVEL];
        for lvl in (0..self.level).rev() {
            while let Some(next) = self.forward_at(cur, lvl) {
                match self.nodes[next].as_ref().unwrap().key.cmp(&key) {
                    Ordering::Less => cur = next,
                    Ordering::Equal => {
                        let old = std::mem::replace(
                            &mut self.nodes[next].as_mut().unwrap().value,
                            value,
                        );
                        return Some(old);
                    }
                    Ordering::Greater => break,
                }
            }
            update[lvl] = cur;
        }
        if let Some(next) = self.forward_at(cur, 0) {
            if self.nodes[next].as_ref().unwrap().key == key {
                let old = std::mem::replace(
                    &mut self.nodes[next].as_mut().unwrap().value,
                    value,
                );
                return Some(old);
            }
        }
        update[0] = cur;

        let new_level = self.random_level();
        if new_level > self.level {
            for lvl in self.level..new_level {
                update[lvl] = 0;
                self.head_mut().forward[lvl] = None;
            }
            self.level = new_level;
        }

        let id = self.nodes.len();
        self.nodes.push(Some(Node {
            key,
            value,
            forward: vec![None; MAX_LEVEL],
        }));

        for lvl in 0..new_level {
            let prev = update[lvl];
            let next = self.forward_at(prev, lvl);
            self.nodes[id].as_mut().unwrap().forward[lvl] = next;
            if prev == 0 {
                self.head_mut().forward[lvl] = Some(id);
            } else {
                self.node_forward_mut(prev)[lvl] = Some(id);
            }
        }
        None
    }

    /// Remove `key` from the list. Returns the value if it was present.
    pub fn remove(&mut self, key: &K) -> Option<V> {
        let mut cur = 0usize;
        let mut update: Vec<usize> = vec![0; MAX_LEVEL];
        for lvl in (0..self.level).rev() {
            while let Some(next) = self.forward_at(cur, lvl) {
                match self.nodes[next].as_ref().unwrap().key.cmp(key) {
                    Ordering::Less => cur = next,
                    _ => break,
                }
            }
            update[lvl] = cur;
        }
        let target = match self.forward_at(cur, 0) {
            Some(next) if self.nodes[next].as_ref().unwrap().key == *key => next,
            _ => return None,
        };
        for lvl in 0..self.level {
            let is_linked = self.forward_at(update[lvl], lvl) == Some(target);
            if is_linked {
                let next = self.nodes[target].as_ref().unwrap().forward[lvl];
                if update[lvl] == 0 {
                    self.head_mut().forward[lvl] = next;
                } else {
                    self.node_forward_mut(update[lvl])[lvl] = next;
                }
            }
        }
        while self.level > 1 && self.head().forward[self.level - 1].is_none() {
            self.level -= 1;
        }
        self.nodes[target].take().map(|n| n.value)
    }

    /// Iterate over (key, value) pairs in ascending key order.
    pub fn iter(&self) -> SkipListIter<'_, K, V> {
        let head_next = self.head().forward[0];
        SkipListIter {
            nodes: &self.nodes,
            next: head_next,
        }
    }

    fn forward_at(&self, id: usize, lvl: usize) -> Option<usize> {
        if id == 0 {
            self.head_forward[lvl]
        } else {
            self.node_forward(id)[lvl]
        }
    }

    fn random_level(&mut self) -> usize {
        let mut lvl = 1;
        while lvl < MAX_LEVEL && self.next_rand() < PROMOTION_PROB {
            lvl += 1;
        }
        lvl
    }

    fn next_rand(&mut self) -> f64 {
        let mut x = self.rng;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.rng = x;
        ((x >> 11) as f64) / ((1u64 << 53) as f64)
    }
}

/// Read-only view of the head sentinel's forward pointers.
struct SkipListHead<'a, K, V> {
    forward: &'a Vec<Option<usize>>,
    _phantom: std::marker::PhantomData<(K, V)>,
}

/// Mutable view of the head sentinel's forward pointers.
struct SkipListHeadMut<'a, K, V> {
    forward: &'a mut Vec<Option<usize>>,
    _phantom: std::marker::PhantomData<(K, V)>,
}

impl<K: Ord, V> Default for SkipList<K, V> {
    fn default() -> Self {
        Self::new()
    }
}

/// Iterator over a skip list's key/value pairs in ascending order.
pub struct SkipListIter<'a, K, V> {
    nodes: &'a Vec<Option<Node<K, V>>>,
    next: Option<usize>,
}

impl<'a, K, V> Iterator for SkipListIter<'a, K, V> {
    type Item = (&'a K, &'a V);
    fn next(&mut self) -> Option<Self::Item> {
        let id = self.next?;
        let n = self.nodes[id].as_ref()?;
        self.next = n.forward[0];
        Some((&n.key, &n.value))
    }
}

fn seed_from_time() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0xDEADBEEF)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_list() {
        let s: SkipList<i32, &str> = SkipList::new();
        assert!(s.is_empty());
        assert!(s.get(&1).is_none());
    }

    #[test]
    fn insert_and_get() {
        let mut s = SkipList::new();
        s.insert(5, "five");
        s.insert(3, "three");
        s.insert(7, "seven");
        assert_eq!(s.get(&5), Some(&"five"));
        assert_eq!(s.get(&3), Some(&"three"));
        assert_eq!(s.get(&7), Some(&"seven"));
        assert!(s.get(&1).is_none());
    }

    #[test]
    fn insert_replaces_value() {
        let mut s = SkipList::new();
        assert!(s.insert(1, "first").is_none());
        assert_eq!(s.insert(1, "second"), Some("first"));
        assert_eq!(s.get(&1), Some(&"second"));
    }

    #[test]
    fn remove_returns_value() {
        let mut s = SkipList::new();
        s.insert(1, "a");
        s.insert(2, "b");
        assert_eq!(s.remove(&1), Some("a"));
        assert!(s.get(&1).is_none());
        assert!(s.get(&2).is_some());
    }

    #[test]
    fn remove_missing_returns_none() {
        let mut s = SkipList::new();
        s.insert(1, "a");
        assert_eq!(s.remove(&2), None);
    }

    #[test]
    fn iter_in_sorted_order() {
        let mut s = SkipList::new();
        let keys = [5, 3, 7, 1, 9, 4, 6, 8, 2];
        for &k in &keys {
            s.insert(k, k);
        }
        let collected: Vec<(i32, i32)> = s.iter().map(|(k, v)| (*k, *v)).collect();
        let mut expected = keys.to_vec();
        expected.sort();
        let expected: Vec<(i32, i32)> = expected.iter().map(|&k| (k, k)).collect();
        assert_eq!(collected, expected);
    }

    #[test]
    fn many_inserts() {
        let mut s = SkipList::new();
        for i in 0..1000 {
            s.insert(i, i * 10);
        }
        for i in 0..1000 {
            assert_eq!(s.get(&i), Some(&(i * 10)));
        }
        assert_eq!(s.len(), 1000);
    }

    #[test]
    fn many_removes() {
        let mut s = SkipList::new();
        for i in 0..100 {
            s.insert(i, i);
        }
        for i in (0..100).step_by(2) {
            assert_eq!(s.remove(&i), Some(i));
        }
        assert_eq!(s.len(), 50);
        for i in (0..100).step_by(2) {
            assert!(s.get(&i).is_none());
        }
        for i in 1..100 {
            if i % 2 == 1 {
                assert_eq!(s.get(&i), Some(&i));
            }
        }
    }

    #[test]
    fn deterministic_with_seed() {
        let mut a = SkipList::<i32, i32>::with_seed(42);
        let mut b = SkipList::<i32, i32>::with_seed(42);
        for i in 0..100 {
            a.insert(i, i);
            b.insert(i, i);
        }
        let va: Vec<_> = a.iter().map(|(k, v)| (*k, *v)).collect();
        let vb: Vec<_> = b.iter().map(|(k, v)| (*k, *v)).collect();
        assert_eq!(va, vb);
    }
}