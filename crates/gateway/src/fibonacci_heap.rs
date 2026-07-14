//! Fibonacci heap (Fredman & Tarjan, 1984) — simplified implementation.
//!
//! A priority queue with amortized O(1) insert and amortized O(log n)
//! extract-min. This is a pedagogical, single-threaded implementation
//! for `i64` keys; payloads are arbitrary `T`.
//!
//! Reference: Michael L. Fredman, Robert Endre Tarjan, "Fibonacci heaps
//! and their uses in improved network optimization algorithms", Journal
//! of the ACM, 34(3):596-615, July 1987.
//!
//! The implementation tracks a root list (doubly-linked circular list of
//! trees obeying the min-heap property). On `extract_min`, children of
//! the removed root are added to the root list, then trees of equal
//! rank are linked (consolidation) so the root list stays shallow.

use std::collections::HashMap;

/// Stable handle to a heap node, returned by `insert`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct NodeId(pub usize);

#[derive(Debug)]
struct Node<T> {
    key: i64,
    value: T,
    rank: u32,
    child: Option<NodeId>,
    /// Doubly-linked circular sibling list — `left` and `right` always
    /// point to valid nodes (we are never the only node in a list with
    /// pointers to ourselves; the encapsulating list owns that property).
    left: NodeId,
    right: NodeId,
    parent: Option<NodeId>,
    /// True iff the node has lost a child since becoming a child itself.
    /// Used by cascading cuts.
    marked: bool,
}

#[derive(Debug)]
pub struct FibonacciHeap<T> {
    nodes: Vec<Option<Node<T>>>,
    free: Vec<usize>,
    /// Pointer to a root with the minimum key.
    min: Option<NodeId>,
    len: usize,
    next_id: usize,
}

impl<T> FibonacciHeap<T> {
    pub fn new() -> Self {
        Self {
            nodes: Vec::new(),
            free: Vec::new(),
            min: None,
            len: 0,
            next_id: 0,
        }
    }

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    pub fn key_of(&self, id: NodeId) -> Option<i64> {
        self.nodes.get(id.0).and_then(|s| s.as_ref()).map(|n| n.key)
    }

    pub fn value_of(&self, id: NodeId) -> Option<&T> {
        self.nodes
            .get(id.0)
            .and_then(|s| s.as_ref())
            .map(|n| &n.value)
    }

    fn alloc(&mut self, key: i64, value: T) -> NodeId {
        let id = if let Some(idx) = self.free.pop() {
            NodeId(idx)
        } else {
            let id = self.next_id;
            self.next_id += 1;
            self.nodes.push(None);
            NodeId(id)
        };
        let node = Node {
            key,
            value,
            rank: 0,
            child: None,
            // Will be wired by the caller.
            left: id,
            right: id,
            parent: None,
            marked: false,
        };
        self.nodes[id.0] = Some(node);
        id
    }

    /// Insert `(key, value)` and return a stable handle.
    pub fn insert(&mut self, key: i64, value: T) -> NodeId {
        let id = self.alloc(key, value);
        // Splice id into the root list, in singleton form.
        match self.min {
            None => {
                // First node: only member of the root list.
                let node = self.nodes[id.0].as_mut().unwrap();
                node.left = id;
                node.right = id;
                self.min = Some(id);
            }
            Some(m) => {
                // Splice id next to m in the circular root list.
                let m_left = self.nodes[m.0].as_ref().unwrap().left;
                // id.right = m, id.left = m_left
                // m_left.right = id, m.left = id
                self.nodes[id.0].as_mut().unwrap().right = m;
                self.nodes[id.0].as_mut().unwrap().left = m_left;
                self.nodes[m_left.0].as_mut().unwrap().right = id;
                self.nodes[m.0].as_mut().unwrap().left = id;
                if key < self.nodes[m.0].as_ref().unwrap().key {
                    self.min = Some(id);
                }
            }
        }
        self.len += 1;
        id
    }

    /// Decrease key of `id` to `new_key`. Panics if `new_key > current`.
    pub fn decrease_key(&mut self, id: NodeId, new_key: i64) {
        {
            let node = self.nodes[id.0].as_ref().expect("decrease_key: bad id");
            assert!(
                new_key <= node.key,
                "decrease_key: new_key ({}) must be <= current ({})",
                new_key,
                node.key
            );
        }
        self.nodes[id.0].as_mut().unwrap().key = new_key;
        let parent = self.nodes[id.0].as_ref().unwrap().parent;
        if let Some(p) = parent {
            let node_key = self.nodes[id.0].as_ref().unwrap().key;
            let p_key = self.nodes[p.0].as_ref().unwrap().key;
            if node_key < p_key {
                self.cut(id);
                self.cascading_cut(p);
            }
        }
        // If `id` is a root, update min if needed.
        if self.nodes[id.0].as_ref().unwrap().parent.is_none() {
            let m = self.min.unwrap();
            if self.nodes[id.0].as_ref().unwrap().key < self.nodes[m.0].as_ref().unwrap().key {
                self.min = Some(id);
            }
        }
    }

    /// Remove `id` from its parent's child list (or from the root list)
    /// and re-insert it as a root.
    fn cut(&mut self, id: NodeId) {
        let parent = self.nodes[id.0].as_mut().unwrap().parent.take();
        // Detach id from its sibling list.
        let left = self.nodes[id.0].as_ref().unwrap().left;
        let right = self.nodes[id.0].as_ref().unwrap().right;
        self.nodes[left.0].as_mut().unwrap().right = right;
        self.nodes[right.0].as_mut().unwrap().left = left;
        // Update parent's child pointer if needed.
        if let Some(p) = parent {
            let pc = self.nodes[p.0].as_ref().unwrap().child;
            if let Some(c) = pc {
                if c == id {
                    // `id` was the first child. The new first child is
                    // its sibling `right` (which could be itself if
                    // there was only one child).
                    if right == id {
                        self.nodes[p.0].as_mut().unwrap().child = None;
                        self.nodes[p.0].as_mut().unwrap().rank = 0;
                    } else {
                        self.nodes[p.0].as_mut().unwrap().child = Some(right);
                        self.nodes[p.0].as_mut().unwrap().rank -= 1;
                    }
                }
            }
        }
        // Reset id's state as a singleton root.
        self.nodes[id.0].as_mut().unwrap().left = id;
        self.nodes[id.0].as_mut().unwrap().right = id;
        self.nodes[id.0].as_mut().unwrap().marked = false;
        self.nodes[id.0].as_mut().unwrap().parent = None;
        self.nodes[id.0].as_mut().unwrap().rank = 0;
        // Splice id into the root list, next to min.
        if let Some(m) = self.min {
            let m_left = self.nodes[m.0].as_ref().unwrap().left;
            self.nodes[id.0].as_mut().unwrap().right = m;
            self.nodes[id.0].as_mut().unwrap().left = m_left;
            self.nodes[m_left.0].as_mut().unwrap().right = id;
            self.nodes[m.0].as_mut().unwrap().left = id;
            if self.nodes[id.0].as_ref().unwrap().key < self.nodes[m.0].as_ref().unwrap().key {
                self.min = Some(id);
            }
        } else {
            self.min = Some(id);
        }
    }

    fn cascading_cut(&mut self, id: NodeId) {
        let parent = self.nodes[id.0].as_ref().unwrap().parent;
        if let Some(p) = parent {
            let marked = self.nodes[id.0].as_ref().unwrap().marked;
            if !marked {
                self.nodes[id.0].as_mut().unwrap().marked = true;
            } else {
                self.cut(id);
                self.cascading_cut(p);
            }
        }
    }

    /// Remove and return the (key, value) pair with the smallest key.
    /// Requires `T: Default` so we can free the slot without
    /// reconstructing the moved-out value.
    pub fn extract_min(&mut self) -> Option<(i64, T)>
    where
        T: Default,
    {
        let z = self.min?;
        // Collect z's children into a separate vector of root ids.
        let children: Vec<NodeId> = if let Some(c0) = self.nodes[z.0].as_ref().unwrap().child {
            let mut out = vec![c0];
            let mut cur = c0;
            loop {
                let next = self.nodes[cur.0].as_ref().unwrap().right;
                if next == c0 {
                    break;
                }
                out.push(next);
                cur = next;
            }
            out
        } else {
            Vec::new()
        };
        // Detach z from the root list.
        let z_left = self.nodes[z.0].as_ref().unwrap().left;
        let z_right = self.nodes[z.0].as_ref().unwrap().right;
        if z_left == z {
            // z was the only root.
            self.min = None;
        } else {
            self.nodes[z_left.0].as_mut().unwrap().right = z_right;
            self.nodes[z_right.0].as_mut().unwrap().left = z_left;
            // `self.min` previously pointed to z; redirect to a
            // guaranteed-valid root (z_left is still in the list).
            self.min = Some(z_left);
        }
        // Pull z's key + value out before freeing the slot.
        let key = self.nodes[z.0].as_ref().unwrap().key;
        let value = std::mem::replace(&mut self.nodes[z.0].as_mut().unwrap().value, T::default());
        self.nodes[z.0] = None;
        self.free.push(z.0);
        self.len -= 1;
        // Add each of z's children to the root list.
        for c in &children {
            let cid = *c;
            self.nodes[cid.0].as_mut().unwrap().parent = None;
            self.nodes[cid.0].as_mut().unwrap().marked = false;
            self.nodes[cid.0].as_mut().unwrap().left = cid;
            self.nodes[cid.0].as_mut().unwrap().right = cid;
            // Splice into the root list next to current min (if any).
            match self.min {
                None => {
                    self.min = Some(cid);
                }
                Some(m) => {
                    let m_left = self.nodes[m.0].as_ref().unwrap().left;
                    self.nodes[cid.0].as_mut().unwrap().right = m;
                    self.nodes[cid.0].as_mut().unwrap().left = m_left;
                    self.nodes[m_left.0].as_mut().unwrap().right = cid;
                    self.nodes[m.0].as_mut().unwrap().left = cid;
                    if self.nodes[cid.0].as_ref().unwrap().key
                        < self.nodes[m.0].as_ref().unwrap().key
                    {
                        self.min = Some(cid);
                    }
                }
            }
        }
        if self.len > 0 {
            // Consolidate to keep the root list shallow; consolidate also
            // updates self.min to a surviving root with the smallest key.
            self.consolidate();
        }
        Some((key, value))
    }

    /// Helper: pick the first available root (any non-None in `nodes`).
    fn first_root(&self) -> NodeId {
        for (i, slot) in self.nodes.iter().enumerate() {
            if slot.is_some() {
                return NodeId(i);
            }
        }
        panic!("first_root on empty heap");
    }

    /// Walk the root list to find the node with the smallest key.
    /// Starts at `self.min` (which must already point at a root) and
    /// walks the doubly-linked root list.
    fn find_min_root(&self) -> Option<NodeId> {
        let start = self.min?;
        let mut best = start;
        let mut best_key = self.nodes[best.0].as_ref().unwrap().key;
        let mut cur = start;
        loop {
            let next = self.nodes[cur.0].as_ref().unwrap().right;
            if next == start {
                break;
            }
            let k = self.nodes[next.0].as_ref().unwrap().key;
            if k < best_key {
                best = next;
                best_key = k;
            }
            cur = next;
        }
        Some(best)
    }

    /// Walk the root list to find the node with the smallest key,
    /// starting from any valid root. Use this after `consolidate` to
    /// ensure we find the right min even if `self.min` is stale.
    ///
    /// This is O(n) and is the simpler fallback. The fast path is
    /// `find_min_root` which uses `self.min`.
    fn find_min_root_from(&self, start: NodeId) -> Option<NodeId> {
        let mut best = start;
        let mut best_key = self.nodes[best.0].as_ref().unwrap().key;
        let mut cur = start;
        loop {
            let next = self.nodes[cur.0].as_ref().unwrap().right;
            if next == start {
                break;
            }
            let k = self.nodes[next.0].as_ref().unwrap().key;
            if k < best_key {
                best = next;
                best_key = k;
            }
            cur = next;
        }
        Some(best)
    }

    /// Consolidate: re-link trees so the root list contains at most one
    /// tree of each rank. This is O(log n) amortized.
    fn consolidate(&mut self) {
        // Gather all roots into a Vec. The caller must have set self.min
        // to a valid root.
        let mut roots: Vec<NodeId> = Vec::new();
        let start = match self.min {
            Some(m) => m,
            None => return,
        };
        roots.push(start);
        let mut cur = start;
        loop {
            let next = self.nodes[cur.0].as_ref().unwrap().right;
            if next == start {
                break;
            }
            roots.push(next);
            cur = next;
        }
        // For each root, link with any other root of the same rank.
        let mut by_rank: HashMap<u32, NodeId> = HashMap::new();
        for &r in &roots {
            let mut cur = r;
            let mut rank = self.nodes[cur.0].as_ref().unwrap().rank;
            loop {
                if let Some(other) = by_rank.remove(&rank) {
                    let cur_key = self.nodes[cur.0].as_ref().unwrap().key;
                    let other_key = self.nodes[other.0].as_ref().unwrap().key;
                    let (parent, child) = if cur_key <= other_key {
                        (cur, other)
                    } else {
                        (other, cur)
                    };
                    self.link(parent, child);
                    cur = parent;
                    rank = self.nodes[cur.0].as_ref().unwrap().rank;
                } else {
                    by_rank.insert(rank, cur);
                    break;
                }
            }
        }
        // The root list now contains exactly the trees in `by_rank`. Find
        // the new min by scanning by_rank's values.
        let mut new_min: Option<NodeId> = None;
        let mut new_min_key: i64 = i64::MAX;
        for (_, root_id) in by_rank.iter() {
            let k = self.nodes[root_id.0].as_ref().unwrap().key;
            if k < new_min_key {
                new_min_key = k;
                new_min = Some(*root_id);
            }
        }
        self.min = new_min;
    }

    /// Make `child` a child of `parent`. Both are roots in the root list.
    fn link(&mut self, parent: NodeId, child: NodeId) {
        // Detach child from the root list.
        let c_left = self.nodes[child.0].as_ref().unwrap().left;
        let c_right = self.nodes[child.0].as_ref().unwrap().right;
        self.nodes[c_left.0].as_mut().unwrap().right = c_right;
        self.nodes[c_right.0].as_mut().unwrap().left = c_left;
        // Insert child into parent's child list (a circular list, or
        // empty).
        self.nodes[child.0].as_mut().unwrap().parent = Some(parent);
        self.nodes[child.0].as_mut().unwrap().marked = false;
        match self.nodes[parent.0].as_ref().unwrap().child {
            None => {
                // child is its own circular list.
                self.nodes[child.0].as_mut().unwrap().left = child;
                self.nodes[child.0].as_mut().unwrap().right = child;
                self.nodes[parent.0].as_mut().unwrap().child = Some(child);
            }
            Some(first) => {
                let f_left = self.nodes[first.0].as_ref().unwrap().left;
                self.nodes[child.0].as_mut().unwrap().left = f_left;
                self.nodes[child.0].as_mut().unwrap().right = first;
                self.nodes[f_left.0].as_mut().unwrap().right = child;
                self.nodes[first.0].as_mut().unwrap().left = child;
            }
        }
        self.nodes[parent.0].as_mut().unwrap().rank += 1;
    }
}

impl<T> Default for FibonacciHeap<T> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_heap() {
        let mut h: FibonacciHeap<&'static str> = FibonacciHeap::new();
        assert!(h.is_empty());
        assert_eq!(h.len(), 0);
        assert!(h.extract_min().is_none());
    }

    #[test]
    fn single_insert_extract() {
        let mut h: FibonacciHeap<&'static str> = FibonacciHeap::new();
        h.insert(5, "five");
        assert_eq!(h.len(), 1);
        let (k, v) = h.extract_min().expect("some");
        assert_eq!(k, 5);
        assert_eq!(v, "five");
        assert!(h.is_empty());
    }

    #[test]
    fn extracts_in_sorted_order_small() {
        let mut h: FibonacciHeap<i32> = FibonacciHeap::new();
        h.insert(7, 7);
        h.insert(2, 2);
        h.insert(5, 5);
        h.insert(1, 1);
        let order: Vec<(i64, i32)> = (0..4).map(|_| h.extract_min().expect("some")).collect();
        assert_eq!(order, vec![(1, 1), (2, 2), (5, 5), (7, 7)]);
        assert!(h.is_empty());
    }

    #[test]
    fn extracts_in_sorted_order_larger() {
        let mut h: FibonacciHeap<u32> = FibonacciHeap::new();
        for v in [7u32, 2, 5, 1, 9, 4, 8, 3, 6] {
            h.insert(v as i64, v);
        }
        let order: Vec<(i64, u32)> = (0..9).map(|_| h.extract_min().expect("some")).collect();
        let expected: Vec<(i64, u32)> = (1..=9u32).map(|v| (v as i64, v)).collect();
        assert_eq!(order, expected);
    }

    #[test]
    fn key_of_reports_current_value() {
        let mut h: FibonacciHeap<&'static str> = FibonacciHeap::new();
        let id = h.insert(8, "x");
        assert_eq!(h.key_of(id), Some(8));
        h.decrease_key(id, 3);
        assert_eq!(h.key_of(id), Some(3));
    }

    #[test]
    fn value_of_returns_payload() {
        let mut h: FibonacciHeap<&'static str> = FibonacciHeap::new();
        let id = h.insert(1, "hello");
        assert_eq!(h.value_of(id), Some(&"hello"));
    }

    #[test]
    fn len_tracks_inserts_and_extracts() {
        let mut h: FibonacciHeap<u32> = FibonacciHeap::new();
        for i in 0..20u32 {
            h.insert(i as i64, i);
        }
        assert_eq!(h.len(), 20);
        for _ in 0..5 {
            h.extract_min();
        }
        assert_eq!(h.len(), 15);
    }

    #[test]
    fn extracts_after_decrease_key() {
        let mut h: FibonacciHeap<&'static str> = FibonacciHeap::new();
        let a = h.insert(10, "a");
        let b = h.insert(20, "b");
        let c = h.insert(30, "c");
        h.decrease_key(c, 5);
        h.decrease_key(a, 1);
        // Should extract in order: 1, 5, 20.
        let (k1, v1) = h.extract_min().expect("some");
        let (k2, v2) = h.extract_min().expect("some");
        let (k3, v3) = h.extract_min().expect("some");
        assert_eq!((k1, v1), (1, "a"));
        assert_eq!((k2, v2), (5, "c"));
        assert_eq!((k3, v3), (20, "b"));
        let _ = b;
    }

    #[test]
    fn handles_duplicate_keys() {
        let mut h: FibonacciHeap<&'static str> = FibonacciHeap::new();
        h.insert(7, "first");
        h.insert(7, "second");
        h.insert(7, "third");
        let mut out = Vec::new();
        while let Some((k, v)) = h.extract_min() {
            out.push((k, v));
        }
        assert_eq!(out.len(), 3);
        for (k, _) in &out {
            assert_eq!(*k, 7);
        }
    }
}
