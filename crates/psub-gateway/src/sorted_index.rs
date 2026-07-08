//! Sorted-index map backed by a `Vec<K>` + parallel `Vec<V>`.
//!
//! A small, simple key/value container that maintains keys in sorted order
//! at all times. Lookups are O(log n) via binary search; inserts and
//! removes are O(n) due to shifting. Best suited for small collections
//! (< 10 000 entries) where you want a stable iteration order and don't
//! need hash performance.
//!
//! For larger collections, prefer [`std::collections::BTreeMap`].

use std::cmp::Ordering;
use std::ops::Bound;

/// A sorted, in-memory key/value map.
#[derive(Debug, Clone, Default)]
pub struct SortedIndex<K, V> {
    keys: Vec<K>,
    values: Vec<V>,
}

impl<K: Ord, V> SortedIndex<K, V> {
    /// Create an empty index.
    pub fn new() -> Self {
        Self {
            keys: Vec::new(),
            values: Vec::new(),
        }
    }

    /// Create an index with the given initial capacity.
    pub fn with_capacity(cap: usize) -> Self {
        Self {
            keys: Vec::with_capacity(cap),
            values: Vec::with_capacity(cap),
        }
    }

    /// Number of key/value pairs.
    pub fn len(&self) -> usize {
        self.keys.len()
    }

    /// True if the index contains no entries.
    pub fn is_empty(&self) -> bool {
        self.keys.is_empty()
    }

    /// Find the index of `key` via binary search. Returns `Ok(idx)` if the
    /// key is present, `Err(idx)` if it isn't (with the insertion point).
    fn locate(&self, key: &K) -> Result<usize, usize> {
        self.keys.binary_search(key)
    }

    /// Insert `key` -> `value`. If the key already exists, the value is
    /// replaced and the old value returned. Otherwise, `None` is returned.
    pub fn insert(&mut self, key: K, value: V) -> Option<V> {
        match self.keys.binary_search(&key) {
            Ok(idx) => {
                let old = std::mem::replace(&mut self.values[idx], value);
                Some(old)
            }
            Err(idx) => {
                self.keys.insert(idx, key);
                self.values.insert(idx, value);
                None
            }
        }
    }

    /// Look up `key`, returning `Some(&V)` if present.
    pub fn get(&self, key: &K) -> Option<&V> {
        self.locate(key).ok().map(|i| &self.values[i])
    }

    /// Mutable lookup. Returns `Some(&mut V)` if present.
    pub fn get_mut(&mut self, key: &K) -> Option<&mut V> {
        let i = self.locate(key).ok()?;
        Some(&mut self.values[i])
    }

    /// Returns true if the key is in the index.
    pub fn contains(&self, key: &K) -> bool {
        self.locate(key).is_ok()
    }

    /// Remove `key` from the index, returning the value if it existed.
    pub fn remove(&mut self, key: &K) -> Option<V> {
        match self.locate(key) {
            Ok(i) => {
                self.keys.remove(i);
                Some(self.values.remove(i))
            }
            Err(_) => None,
        }
    }

    /// Pop the smallest key/value pair.
    pub fn pop_first(&mut self) -> Option<(K, V)> {
        if self.keys.is_empty() {
            None
        } else {
            let k = self.keys.remove(0);
            let v = self.values.remove(0);
            Some((k, v))
        }
    }

    /// Pop the largest key/value pair.
    pub fn pop_last(&mut self) -> Option<(K, V)> {
        if self.keys.is_empty() {
            None
        } else {
            let k = self.keys.pop()?;
            let v = self.values.pop()?;
            Some((k, v))
        }
    }

    /// First key (smallest) without removing it.
    pub fn first_key(&self) -> Option<&K> {
        self.keys.first()
    }

    /// Last key (largest) without removing it.
    pub fn last_key(&self) -> Option<&K> {
        self.keys.last()
    }

    /// Iterate over all entries in ascending key order.
    pub fn iter(&self) -> impl Iterator<Item = (&K, &V)> {
        self.keys.iter().zip(self.values.iter())
    }

    /// Mutable iteration in ascending key order.
    pub fn iter_mut(&mut self) -> impl Iterator<Item = (&K, &mut V)> {
        self.keys.iter().zip(self.values.iter_mut())
    }

    /// Find the smallest key >= `bound`. `Bound::Unbounded` returns the
    /// first entry; `Bound::Included(k)`/`Bound::Excluded(k)` filter
    /// inclusively / exclusively.
    pub fn lower_bound(&self, bound: Bound<&K>) -> Option<usize> {
        let start = match bound {
            Bound::Unbounded => 0,
            Bound::Included(k) => match self.keys.binary_search(k) {
                Ok(i) => i,
                Err(i) => i,
            },
            Bound::Excluded(k) => match self.keys.binary_search(k) {
                Ok(i) => i + 1,
                Err(i) => i,
            },
        };
        if start < self.keys.len() {
            Some(start)
        } else {
            None
        }
    }

    /// Iterate over the range `[lo, hi]` (inclusive on both ends).
    pub fn range<'a>(&'a self, lo: &'a K, hi: &'a K) -> impl Iterator<Item = (&'a K, &'a V)> {
        let lo_idx = self.lower_bound(Bound::Included(lo)).unwrap_or(0);
        let hi_cmp = |k: &K| k.cmp(hi) != Ordering::Greater;
        self.keys[lo_idx..]
            .iter()
            .zip(self.values[lo_idx..].iter())
            .take_while(move |(k, _)| hi_cmp(*k))
    }
}

impl<K: Ord, V> FromIterator<(K, V)> for SortedIndex<K, V> {
    fn from_iter<I: IntoIterator<Item = (K, V)>>(iter: I) -> Self {
        let mut idx = SortedIndex::new();
        for (k, v) in iter {
            idx.insert(k, v);
        }
        idx
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_index() {
        let idx: SortedIndex<i32, &str> = SortedIndex::new();
        assert!(idx.is_empty());
        assert_eq!(idx.len(), 0);
        assert!(idx.get(&1).is_none());
    }

    #[test]
    fn insert_and_lookup() {
        let mut idx = SortedIndex::new();
        idx.insert(5, "five");
        idx.insert(3, "three");
        idx.insert(7, "seven");
        assert_eq!(idx.len(), 3);
        assert_eq!(idx.get(&5), Some(&"five"));
        assert_eq!(idx.get(&3), Some(&"three"));
        assert_eq!(idx.get(&7), Some(&"seven"));
        assert!(idx.get(&1).is_none());
    }

    #[test]
    fn insert_overwrites_value() {
        let mut idx = SortedIndex::new();
        assert!(idx.insert(1, "first").is_none());
        assert_eq!(idx.insert(1, "second"), Some("first"));
        assert_eq!(idx.get(&1), Some(&"second"));
        assert_eq!(idx.len(), 1);
    }

    #[test]
    fn iter_in_sorted_order() {
        let mut idx = SortedIndex::new();
        for k in [5, 3, 7, 1, 9, 4, 6, 8, 2] {
            idx.insert(k, k);
        }
        let collected: Vec<(i32, i32)> = idx.iter().map(|(k, v)| (*k, *v)).collect();
        assert_eq!(
            collected,
            vec![(1, 1), (2, 2), (3, 3), (4, 4), (5, 5), (6, 6), (7, 7), (8, 8), (9, 9)]
        );
    }

    #[test]
    fn remove_returns_value() {
        let mut idx = SortedIndex::new();
        idx.insert(1, "a");
        idx.insert(2, "b");
        assert_eq!(idx.remove(&1), Some("a"));
        assert_eq!(idx.remove(&1), None);
        assert_eq!(idx.len(), 1);
    }

    #[test]
    fn first_and_last() {
        let mut idx = SortedIndex::new();
        idx.insert(5, "five");
        idx.insert(3, "three");
        idx.insert(7, "seven");
        assert_eq!(*idx.first_key().unwrap(), 3);
        assert_eq!(*idx.last_key().unwrap(), 7);
    }

    #[test]
    fn pop_first_and_last() {
        let mut idx = SortedIndex::new();
        idx.insert(1, "a");
        idx.insert(2, "b");
        idx.insert(3, "c");
        assert_eq!(idx.pop_first(), Some((1, "a")));
        assert_eq!(idx.pop_last(), Some((3, "c")));
        assert_eq!(idx.pop_first(), Some((2, "b")));
        assert_eq!(idx.pop_first(), None);
    }

    #[test]
    fn range_query() {
        let mut idx = SortedIndex::new();
        for k in 1..=10 {
            idx.insert(k, k * 10);
        }
        let collected: Vec<(i32, i32)> = idx.range(&3, &7).map(|(k, v)| (*k, *v)).collect();
        assert_eq!(collected, vec![(3, 30), (4, 40), (5, 50), (6, 60), (7, 70)]);
    }

    #[test]
    fn contains() {
        let mut idx = SortedIndex::new();
        idx.insert(10, "x");
        assert!(idx.contains(&10));
        assert!(!idx.contains(&11));
    }

    #[test]
    fn from_iter_collects_in_order() {
        let idx: SortedIndex<i32, i32> = vec![(3, 30), (1, 10), (2, 20)].into_iter().collect();
        let collected: Vec<(i32, i32)> = idx.iter().map(|(k, v)| (*k, *v)).collect();
        assert_eq!(collected, vec![(1, 10), (2, 20), (3, 30)]);
    }

    #[test]
    fn many_inserts() {
        let mut idx = SortedIndex::new();
        for i in (0..1000).rev() {
            idx.insert(i, i);
        }
        assert_eq!(idx.len(), 1000);
        // Spot-check that lookups work.
        for i in 0..1000 {
            assert_eq!(idx.get(&i), Some(&i));
        }
    }
}