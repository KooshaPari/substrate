//! LRU (least-recently-used) cache with O(1) get/put.
//!
//! Backed by a `HashMap<K, NodeId>` for O(1) lookup and a doubly-linked
//! list (via indices into a `Vec<Node>`) for O(1) recency update. On
//! insert past capacity, evicts the least-recently-used entry.
//!
//! `get` and `put` both mark the touched entry as most-recently-used.
//! Use [`peek`] for read-only lookups that don't update recency.

use std::collections::HashMap;
use std::hash::Hash;

struct Node<K, V> {
    key: K,
    value: V,
    prev: Option<usize>,
    next: Option<usize>,
}

/// An LRU cache with a fixed capacity.
pub struct LruCache<K, V> {
    map: HashMap<K, usize>,
    nodes: Vec<Option<Node<K, V>>>,
    free: Vec<usize>,
    head: Option<usize>, // most-recently-used
    tail: Option<usize>, // least-recently-used
    capacity: usize,
}

impl<K: Hash + Eq + Clone, V> LruCache<K, V> {
    /// Create a new cache with the given capacity (panics if 0).
    pub fn new(capacity: usize) -> Self {
        assert!(capacity > 0, "lru cache capacity must be > 0");
        Self {
            map: HashMap::with_capacity(capacity),
            nodes: Vec::with_capacity(capacity),
            free: Vec::new(),
            head: None,
            tail: None,
            capacity,
        }
    }

    pub fn len(&self) -> usize {
        self.map.len()
    }

    pub fn capacity(&self) -> usize {
        self.capacity
    }

    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }

    pub fn is_full(&self) -> bool {
        self.map.len() >= self.capacity
    }

    /// Returns true if `key` is in the cache.
    pub fn contains(&self, key: &K) -> bool {
        self.map.contains_key(key)
    }

    /// Look up a key without updating recency.
    pub fn peek(&self, key: &K) -> Option<&V> {
        let id = *self.map.get(key)?;
        self.nodes.get(id)?.as_ref().map(|n| &n.value)
    }

    /// Look up a key, marking it as most-recently-used.
    pub fn get(&mut self, key: &K) -> Option<&V> {
        let id = *self.map.get(key)?;
        self.touch(id);
        self.nodes[id].as_ref().map(|n| &n.value)
    }

    /// Mutable `get` variant.
    pub fn get_mut(&mut self, key: &K) -> Option<&mut V> {
        let id = *self.map.get(key)?;
        self.touch(id);
        self.nodes[id].as_mut().map(|n| &mut n.value)
    }

    /// Insert (key, value). If the key already exists, replaces the
    /// value and marks it as most-recently-used. If the cache is at
    /// capacity, evicts the least-recently-used entry first.
    /// Returns the evicted (key, value) pair if any.
    pub fn put(&mut self, key: K, value: V) -> Option<(K, V)> {
        if let Some(&id) = self.map.get(&key) {
            // Replace existing.
            if let Some(n) = self.nodes[id].as_mut() {
                n.value = value;
            }
            self.touch(id);
            return None;
        }
        let evicted = if self.map.len() >= self.capacity {
            self.evict_tail()
        } else {
            None
        };
        let id = if let Some(reused) = self.free.pop() {
            self.nodes[reused] = Some(Node {
                key: key.clone(),
                value,
                prev: None,
                next: None,
            });
            reused
        } else {
            self.nodes.push(Some(Node {
                key: key.clone(),
                value,
                prev: None,
                next: None,
            }));
            self.nodes.len() - 1
        };
        self.map.insert(key, id);
        self.push_front(id);
        evicted
    }

    /// Remove a key from the cache. Returns the removed value if the
    /// key was present.
    pub fn remove(&mut self, key: &K) -> Option<V> {
        let id = *self.map.get(key)?;
        self.unlink(id);
        self.map.remove(key);
        let node = self.nodes[id].take()?;
        self.free.push(id);
        Some(node.value)
    }

    /// Remove all entries.
    pub fn clear(&mut self) {
        self.map.clear();
        self.free.clear();
        // Reset each slot back to None without dropping the allocations.
        for slot in self.nodes.iter_mut() {
            *slot = None;
        }
        self.head = None;
        self.tail = None;
    }

    /// Iterate (key, value) pairs from most- to least-recently-used.
    pub fn iter(&self) -> LruIter<'_, K, V> {
        LruIter {
            nodes: &self.nodes,
            next: self.head,
        }
    }

    /// Return the least-recently-used (key, value) pair without modifying order.
    pub fn peek_lru(&self) -> Option<(&K, &V)> {
        let id = self.tail?;
        self.nodes[id].as_ref().map(|n| (&n.key, &n.value))
    }

    fn touch(&mut self, id: usize) {
        if Some(id) == self.head {
            return;
        }
        self.unlink(id);
        self.push_front(id);
    }

    fn push_front(&mut self, id: usize) {
        if let Some(slot) = self.nodes.get_mut(id) {
            if let Some(n) = slot.as_mut() {
                n.prev = None;
                n.next = self.head;
            }
        }
        if let Some(h) = self.head {
            if let Some(head) = self.nodes.get_mut(h) {
                if let Some(hn) = head.as_mut() {
                    hn.prev = Some(id);
                }
            }
        }
        self.head = Some(id);
        if self.tail.is_none() {
            self.tail = Some(id);
        }
    }

    fn unlink(&mut self, id: usize) {
        let prev = self.nodes[id].as_ref().and_then(|n| n.prev);
        let next = self.nodes[id].as_ref().and_then(|n| n.next);
        if let Some(p) = prev {
            if let Some(node) = self.nodes.get_mut(p) {
                if let Some(n) = node.as_mut() {
                    n.next = next;
                }
            }
        } else {
            self.head = next;
        }
        if let Some(n) = next {
            if let Some(node) = self.nodes.get_mut(n) {
                if let Some(nn) = node.as_mut() {
                    nn.prev = prev;
                }
            }
        } else {
            self.tail = prev;
        }
        if let Some(slot) = self.nodes.get_mut(id) {
            if let Some(n) = slot.as_mut() {
                n.prev = None;
                n.next = None;
            }
        }
    }

    fn evict_tail(&mut self) -> Option<(K, V)> {
        let id = self.tail?;
        self.unlink(id);
        self.map.remove(&self.nodes[id].as_ref()?.key.clone());
        let node = self.nodes[id].take()?;
        self.free.push(id);
        Some((node.key, node.value))
    }
}

/// Iterates the LRU list from most- to least-recently-used.
pub struct LruIter<'a, K, V> {
    nodes: &'a Vec<Option<Node<K, V>>>,
    next: Option<usize>,
}

impl<'a, K, V> Iterator for LruIter<'a, K, V> {
    type Item = (&'a K, &'a V);
    fn next(&mut self) -> Option<Self::Item> {
        let id = self.next?;
        let n = self.nodes[id].as_ref()?;
        self.next = n.next;
        Some((&n.key, &n.value))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn put_and_get() {
        let mut c = LruCache::new(2);
        assert!(c.put(1, "a").is_none());
        assert!(c.put(2, "b").is_none());
        assert_eq!(c.get(&1), Some(&"a"));
        assert_eq!(c.get(&2), Some(&"b"));
        assert_eq!(c.len(), 2);
    }

    #[test]
    fn peek_does_not_update_recency() {
        let mut c = LruCache::new(2);
        c.put(1, "a");
        c.put(2, "b");
        assert_eq!(c.peek(&1), Some(&"a"));
        // After peek, key 1 is still LRU; touching 2 then inserting 3 evicts 1.
        c.get(&2);
        let evicted = c.put(3, "c");
        assert_eq!(evicted, Some((1, "a")));
    }

    #[test]
    fn get_updates_recency() {
        let mut c = LruCache::new(2);
        c.put(1, "a");
        c.put(2, "b");
        c.get(&1); // promote 1 to MRU
        // Now insert 3 — should evict 2 (LRU), not 1.
        let evicted = c.put(3, "c");
        assert_eq!(evicted, Some((2, "b")));
        assert_eq!(c.get(&1), Some(&"a"));
    }

    #[test]
    fn replace_existing() {
        let mut c = LruCache::new(2);
        c.put(1, "a");
        c.put(1, "A");
        assert_eq!(c.len(), 1);
        assert_eq!(c.get(&1), Some(&"A"));
    }

    #[test]
    fn remove_returns_value() {
        let mut c = LruCache::new(2);
        c.put(1, "a");
        c.put(2, "b");
        assert_eq!(c.remove(&1), Some("a"));
        assert_eq!(c.len(), 1);
        assert!(c.get(&1).is_none());
    }

    #[test]
    fn clear_resets() {
        let mut c = LruCache::new(2);
        c.put(1, "a");
        c.put(2, "b");
        c.clear();
        assert!(c.is_empty());
        assert_eq!(c.len(), 0);
    }

    #[test]
    fn iter_mru_to_lru() {
        let mut c = LruCache::new(3);
        c.put(1, "a");
        c.put(2, "b");
        c.put(3, "c");
        c.get(&1); // promote 1 to MRU
        let order: Vec<i32> = c.iter().map(|(k, _)| *k).collect();
        assert_eq!(order, vec![1, 3, 2]);
    }

    #[test]
    fn peek_lru_returns_tail() {
        let mut c = LruCache::new(3);
        c.put(1, "a");
        c.put(2, "b");
        c.put(3, "c");
        // LRU is 1 (oldest).
        let (k, v) = c.peek_lru().unwrap();
        assert_eq!(*k, 1);
        assert_eq!(*v, "a");
    }

    #[test]
    fn get_mut_works() {
        let mut c = LruCache::new(2);
        c.put(1, vec![1, 2, 3]);
        if let Some(v) = c.get_mut(&1) {
            v.push(4);
        }
        assert_eq!(c.get(&1), Some(&vec![1, 2, 3, 4]));
    }

    #[test]
    fn capacity_zero_panics() {
        let r = std::panic::catch_unwind(|| {
            let _ = LruCache::<i32, i32>::new(0);
        });
        assert!(r.is_err());
    }

    #[test]
    fn evict_at_capacity() {
        let mut c = LruCache::new(2);
        c.put(1, "a");
        c.put(2, "b");
        let evicted = c.put(3, "c");
        assert_eq!(evicted, Some((1, "a")));
        assert_eq!(c.len(), 2);
    }

    #[test]
    fn slot_reuse_after_remove() {
        let mut c = LruCache::new(2);
        c.put(1, "a");
        c.put(2, "b");
        c.remove(&1);
        // Slot freed; we should be able to insert 3 without evicting 2.
        c.put(3, "c");
        assert_eq!(c.len(), 2);
        assert!(c.contains(&2));
        assert!(c.contains(&3));
    }
}