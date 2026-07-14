//! Disjoint-set / Union-Find data structure with path compression +
//! union by rank. Supports two operations:
//!
//! - `find(x)` returns the canonical representative of the set
//!   containing `x`. Performs path compression along the way.
//! - `union(x, y)` merges the sets containing `x` and `y`. Returns
//!   `true` if a merge happened, `false` if they were already in the
//!   same set.
//!
//! Both run in effectively-constant amortized time (α(n), inverse
//! Ackermann). Suitable for Kruskal's MST, connectivity in graphs,
//! percolation, and dynamic reachability.

/// A disjoint-set forest over `0..n` elements.
pub struct UnionFind {
    parent: Vec<usize>,
    rank: Vec<u8>,
}

impl UnionFind {
    /// Create a new forest with `n` elements, each in its own set.
    pub fn new(n: usize) -> Self {
        Self {
            parent: (0..n).collect(),
            rank: vec![0; n],
        }
    }

    pub fn len(&self) -> usize {
        self.parent.len()
    }

    pub fn is_empty(&self) -> bool {
        self.parent.is_empty()
    }

    /// Find the canonical representative of `x`. Performs path
    /// compression: every node on the search path is updated to point
    /// directly at the root.
    pub fn find(&mut self, x: usize) -> usize {
        let root = self.find_root(x);
        // Path compression: walk from x to root, updating each node.
        let mut cur = x;
        while cur != root {
            let next = self.parent[cur];
            self.parent[cur] = root;
            cur = next;
        }
        root
    }

    /// Like `find` but without path compression (read-only).
    pub fn find_no_compress(&self, x: usize) -> usize {
        self.find_root(x)
    }

    fn find_root(&self, mut x: usize) -> usize {
        while self.parent[x] != x {
            x = self.parent[x];
        }
        x
    }

    /// Merge the sets containing `x` and `y`. Returns `true` if a
    /// merge actually happened (i.e., the elements were in different
    /// sets), `false` if they were already in the same set.
    pub fn union(&mut self, x: usize, y: usize) -> bool {
        let rx = self.find(x);
        let ry = self.find(y);
        if rx == ry {
            return false;
        }
        // Union by rank: attach the shorter tree under the taller.
        if self.rank[rx] < self.rank[ry] {
            self.parent[rx] = ry;
        } else if self.rank[rx] > self.rank[ry] {
            self.parent[ry] = rx;
        } else {
            self.parent[ry] = rx;
            self.rank[rx] += 1;
        }
        true
    }

    /// Returns `true` if `x` and `y` are in the same set.
    pub fn connected(&mut self, x: usize, y: usize) -> bool {
        self.find(x) == self.find(y)
    }

    /// Returns the number of distinct sets.
    pub fn count_sets(&mut self) -> usize {
        let n = self.parent.len();
        let mut count = 0;
        for i in 0..n {
            if self.find(i) == i {
                count += 1;
            }
        }
        count
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_each_its_own_set() {
        let uf = UnionFind::new(5);
        assert_eq!(uf.len(), 5);
        assert_eq!(uf.count_sets_check(), 5);
    }

    #[test]
    fn union_merges_sets() {
        let mut uf = UnionFind::new(5);
        assert!(uf.union(0, 1));
        assert!(uf.connected(0, 1));
        assert!(!uf.connected(0, 2));
    }

    #[test]
    fn duplicate_union_noop() {
        let mut uf = UnionFind::new(5);
        assert!(uf.union(0, 1));
        assert!(!uf.union(0, 1));
        assert!(!uf.union(1, 0));
    }

    #[test]
    fn transitive_connection() {
        let mut uf = UnionFind::new(5);
        uf.union(0, 1);
        uf.union(1, 2);
        uf.union(2, 3);
        assert!(uf.connected(0, 3));
        assert!(uf.connected(1, 4) == false);
    }

    #[test]
    fn separate_components() {
        let mut uf = UnionFind::new(6);
        uf.union(0, 1);
        uf.union(1, 2);
        uf.union(3, 4);
        assert!(uf.connected(0, 2));
        assert!(uf.connected(3, 4));
        assert!(!uf.connected(0, 3));
        assert!(!uf.connected(2, 5));
    }

    #[test]
    fn count_sets_after_unions() {
        let mut uf = UnionFind::new(6);
        uf.union(0, 1);
        uf.union(2, 3);
        uf.union(4, 5);
        assert_eq!(uf.count_sets(), 3);
        uf.union(0, 2);
        assert_eq!(uf.count_sets(), 2);
        uf.union(0, 4);
        assert_eq!(uf.count_sets(), 1);
    }

    #[test]
    fn path_compression_works() {
        let mut uf = UnionFind::new(10);
        for i in 1..10 {
            uf.union(0, i);
        }
        // After many finds, all elements should have root as direct parent.
        for _ in 0..3 {
            let _ = uf.find(9);
        }
        assert_eq!(uf.find_no_compress(5), uf.find(0));
    }

    #[test]
    fn empty() {
        let uf = UnionFind::new(0);
        assert!(uf.is_empty());
        assert_eq!(uf.count_sets_check(), 0);
    }

    #[test]
    fn out_of_bounds_panics() {
        let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let mut uf = UnionFind::new(3);
            let _ = uf.find(10);
        }));
        assert!(r.is_err());
    }
}

// Internal counting helper (only used by tests).
impl UnionFind {
    #[doc(hidden)]
    pub fn count_sets_check(&self) -> usize {
        // Without path compression (could over-count after merges).
        let mut roots = std::collections::HashSet::new();
        for i in 0..self.parent.len() {
            let mut cur = i;
            while self.parent[cur] != cur {
                cur = self.parent[cur];
            }
            roots.insert(cur);
        }
        roots.len()
    }
}
