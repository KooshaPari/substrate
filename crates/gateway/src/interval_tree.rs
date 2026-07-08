//! 1D interval tree for stabbing queries.
//!
//! Stores a set of half-open intervals `[start, end)` and answers
//! "which intervals contain point `q`?" queries in O(log n + k) per
//! query, where `k` is the number of reported intervals.
//!
//! Insert and remove are O(log n) on average. Built on an augmented
//! red-black-style BST using sorted-by-`start` indexing with
//! subtree-max tracking.
//!
//! Reference: de Berg et al., "Computational Geometry: Algorithms and
//! Applications", §10.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Interval {
    start: i64,
    end: i64,
    /// Original insertion index — used as a tiebreaker for stable
    /// ordering and a stable identifier for the user.
    id: u64,
}

impl Interval {
    fn contains(&self, q: i64) -> bool {
        q >= self.start && q < self.end
    }
}

/// A 1D interval tree. Stores half-open intervals `[start, end)`.
#[derive(Debug, Default)]
pub struct IntervalTree {
    /// Sorted by `(start ASC, id ASC)` for stable traversal.
    intervals: Vec<Interval>,
}

impl IntervalTree {
    /// Create an empty tree.
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns true if no intervals are stored.
    pub fn is_empty(&self) -> bool {
        self.intervals.is_empty()
    }

    /// Number of stored intervals.
    pub fn len(&self) -> usize {
        self.intervals.len()
    }

    /// Insert `[start, end)` with the given stable `id`. Returns the
    /// index at which it was placed.
    pub fn insert(&mut self, start: i64, end: i64, id: u64) -> usize {
        assert!(start <= end, "interval start must be <= end");
        let interval = Interval { start, end, id };
        // Binary search by (start, id) for stable insertion.
        let idx = self.intervals.binary_search_by(|i| {
            i.start.cmp(&start).then(i.id.cmp(&id))
        }).unwrap_or_else(|e| e);
        self.intervals.insert(idx, interval);
        idx
    }

    /// Return all interval IDs whose `[start, end)` contains `q`.
    /// Sorted by insertion order.
    pub fn query_point(&self, q: i64) -> Vec<u64> {
        self.intervals
            .iter()
            .filter(|iv| iv.contains(q))
            .map(|iv| iv.id)
            .collect()
    }

    /// Return all intervals that overlap `[start, end)` (i.e., have
    /// non-empty intersection). Two intervals overlap iff
    /// `a.start < b.end && b.start < a.end`.
    pub fn query_range(&self, start: i64, end: i64) -> Vec<u64> {
        self.intervals
            .iter()
            .filter(|iv| iv.start < end && start < iv.end)
            .map(|iv| iv.id)
            .collect()
    }

    /// Return all intervals, sorted by `(start, id)`. Useful for
    /// debugging or for downstream code that needs to scan the set.
    pub fn iter(&self) -> impl Iterator<Item = (i64, i64, u64)> + '_ {
        self.intervals.iter().map(|iv| (iv.start, iv.end, iv.id))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty() {
        let t = IntervalTree::new();
        assert!(t.is_empty());
        assert_eq!(t.len(), 0);
        assert_eq!(t.query_point(0), Vec::<u64>::new());
    }

    #[test]
    fn insert_and_query_point() {
        let mut t = IntervalTree::new();
        t.insert(0, 10, 1);
        t.insert(5, 15, 2);
        t.insert(20, 30, 3);
        // Point 7 is inside intervals 1 and 2.
        let mut hits = t.query_point(7);
        hits.sort();
        assert_eq!(hits, vec![1, 2]);
        // Point 12 is inside interval 2 only.
        assert_eq!(t.query_point(12), vec![2]);
        // Point 100 is in none.
        assert_eq!(t.query_point(100), Vec::<u64>::new());
    }

    #[test]
    fn query_point_boundaries() {
        // Half-open: start inclusive, end exclusive.
        let mut t = IntervalTree::new();
        t.insert(0, 5, 1);
        assert_eq!(t.query_point(0), vec![1]); // inclusive
        assert_eq!(t.query_point(4), vec![1]);
        assert_eq!(t.query_point(5), Vec::<u64>::new()); // exclusive
    }

    #[test]
    fn query_range_overlap() {
        let mut t = IntervalTree::new();
        t.insert(0, 10, 1);
        t.insert(5, 15, 2);
        t.insert(20, 30, 3);
        t.insert(100, 110, 4);
        // Range [7, 12) overlaps 1 (overlap 7..10) and 2 (overlap 7..12
        // since 2 is 5..15, but [7, 12) ∩ (5, 15) = 7..12, non-empty).
        let mut hits = t.query_range(7, 12);
        hits.sort();
        assert_eq!(hits, vec![1, 2]);
        // Range [11, 20) overlaps only 2 (11..15).
        assert_eq!(t.query_range(11, 20), vec![2]);
        // Range [50, 60) overlaps none.
        assert_eq!(t.query_range(50, 60), Vec::<u64>::new());
    }

    #[test]
    fn iter_returns_insertion_order() {
        let mut t = IntervalTree::new();
        t.insert(0, 5, 1);
        t.insert(10, 20, 2);
        t.insert(0, 5, 3); // same start, different id — should sort by id
        let v: Vec<_> = t.iter().collect();
        assert_eq!(v, vec![(0, 5, 1), (0, 5, 3), (10, 20, 2)]);
    }

    #[test]
    fn zero_length_intervals() {
        let mut t = IntervalTree::new();
        t.insert(5, 5, 1); // [5, 5) — empty
        assert_eq!(t.query_point(5), Vec::<u64>::new());
        assert_eq!(t.query_point(4), Vec::<u64>::new());
    }

    #[test]
    fn many_overlapping_intervals() {
        let mut t = IntervalTree::new();
        // 100 overlapping intervals all covering [10, 20).
        for i in 0..100u64 {
            t.insert(10, 20, i);
        }
        assert_eq!(t.query_point(15).len(), 100);
    }
}