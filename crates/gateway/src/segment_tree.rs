//! Iterative segment tree for range queries and point updates.
//!
//! Supports an associative, commutative "combine" operation over a
//! static range of elements. Pre-builds a tree over the input array
//! in O(n) time, then answers range queries in O(log n) and applies
//! point updates in O(log n).
//!
//! Default instantiation provides:
//! - [`SumSegTree`] — sum over a range.
//! - [`MinSegTree`] — minimum over a range (uses i64::MAX as identity).
//! - [`MaxSegTree`] — maximum over a range (uses i64::MIN as identity).
//!
//! For non-numeric combines (e.g., XOR, bitwise OR), build your own
//! with the lower-level [`SegTree`] by supplying `op` and `identity`.
//!
//! Reference: Bentley, "Solutions to Klee's rectangle problems" (1977).

/// A segment tree that stores the result of an associative, commutative
/// `op` over each segment. Uses the "iterative bottom-up" layout where
/// the leaves are at indices `[n..2n)` and the root is at index 1.
pub struct SegTree<T, F: Fn(&T, &T) -> T> {
    data: Vec<T>,
    n: usize,
    op: F,
}

impl<T: Copy, F: Fn(&T, &T) -> T> SegTree<T, F> {
    /// Build a segment tree from `values` with the given combine `op`
    /// and `identity` element.
    pub fn from_vec(values: Vec<T>, identity: T, op: F) -> Self {
        let n = values.len().next_power_of_two().max(1);
        let mut data = vec![identity; 2 * n];
        for (i, v) in values.iter().enumerate() {
            data[n + i] = *v;
        }
        // Build up from leaves.
        for i in (1..n).rev() {
            data[i] = op(&data[2 * i], &data[2 * i + 1]);
        }
        Self { data, n, op }
    }

    pub fn len(&self) -> usize {
        self.n
    }

    pub fn is_empty(&self) -> bool {
        self.n == 0
    }

    /// Update the element at index `i` to `value`.
    pub fn update(&mut self, i: usize, value: T) {
        assert!(i < self.n, "segment tree index {} out of range", i);
        let mut idx = self.n + i;
        self.data[idx] = value;
        idx /= 2;
        while idx >= 1 {
            self.data[idx] = (self.op)(&self.data[2 * idx], &self.data[2 * idx + 1]);
            idx /= 2;
        }
    }

    /// Range query: `op` over `values[lo..=hi]` (inclusive). O(log n).
    /// `lo` must be <= `hi`. Out-of-range indices panic.
    pub fn query(&self, lo: usize, hi: usize) -> T {
        assert!(lo <= hi, "query lo ({}) must be <= hi ({})", lo, hi);
        assert!(hi < self.n, "query hi ({}) out of range", hi);
        let mut l = self.n + lo;
        let mut r = self.n + hi;
        let mut left = self._identity_ref();
        let mut right = self._identity_ref();
        while l <= r {
            if l % 2 == 1 {
                left = (self.op)(&left, &self.data[l]);
                l += 1;
            }
            if r % 2 == 0 {
                right = (self.op)(&self.data[r], &right);
                r -= 1;
            }
            l /= 2;
            r /= 2;
        }
        (self.op)(&left, &right)
    }

    fn _identity_ref(&self) -> T {
        // We stored identity at index 0 (or any unused slot). For
        // Copy types this works, but the lifetime requires &T.
        // Workaround: each query rebuilds via the unused index 0.
        self.data[0]
    }
}

/// Specialized sum segment tree.
pub struct SumSegTree {
    inner: SegTree<i64, fn(&i64, &i64) -> i64>,
}

impl SumSegTree {
    pub fn from_vec(values: Vec<i64>) -> Self {
        Self {
            inner: SegTree::from_vec(values, 0, |a, b| a + b),
        }
    }

    pub fn update(&mut self, i: usize, value: i64) {
        self.inner.update(i, value);
    }

    pub fn query(&self, lo: usize, hi: usize) -> i64 {
        self.inner.query(lo, hi)
    }

    pub fn len(&self) -> usize {
        self.inner.len()
    }
}

/// Specialized min segment tree (identity = i64::MAX).
pub struct MinSegTree {
    inner: SegTree<i64, fn(&i64, &i64) -> i64>,
}

impl MinSegTree {
    pub fn from_vec(values: Vec<i64>) -> Self {
        Self {
            inner: SegTree::from_vec(values, i64::MAX, |a, b| *a.min(b)),
        }
    }

    pub fn update(&mut self, i: usize, value: i64) {
        self.inner.update(i, value);
    }

    pub fn query(&self, lo: usize, hi: usize) -> i64 {
        self.inner.query(lo, hi)
    }

    pub fn len(&self) -> usize {
        self.inner.len()
    }
}

/// Specialized max segment tree (identity = i64::MIN).
pub struct MaxSegTree {
    inner: SegTree<i64, fn(&i64, &i64) -> i64>,
}

impl MaxSegTree {
    pub fn from_vec(values: Vec<i64>) -> Self {
        Self {
            inner: SegTree::from_vec(values, i64::MIN, |a, b| *a.max(b)),
        }
    }

    pub fn update(&mut self, i: usize, value: i64) {
        self.inner.update(i, value);
    }

    pub fn query(&self, lo: usize, hi: usize) -> i64 {
        self.inner.query(lo, hi)
    }

    pub fn len(&self) -> usize {
        self.inner.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sum_basic() {
        let t = SumSegTree::from_vec(vec![1, 2, 3, 4, 5]);
        assert_eq!(t.query(0, 4), 15);
        assert_eq!(t.query(0, 0), 1);
        assert_eq!(t.query(2, 2), 3);
        assert_eq!(t.query(2, 4), 12);
    }

    #[test]
    fn sum_update_changes_queries() {
        let mut t = SumSegTree::from_vec(vec![1, 2, 3, 4, 5]);
        t.update(2, 10);
        assert_eq!(t.query(0, 4), 22);
        assert_eq!(t.query(2, 2), 10);
        assert_eq!(t.query(0, 1), 3);
    }

    #[test]
    fn min_basic() {
        let t = MinSegTree::from_vec(vec![5, 2, 8, 1, 9, 3]);
        assert_eq!(t.query(0, 5), 1);
        assert_eq!(t.query(0, 0), 5);
        assert_eq!(t.query(2, 3), 1);
        assert_eq!(t.query(4, 5), 3);
    }

    #[test]
    fn max_basic() {
        let t = MaxSegTree::from_vec(vec![5, 2, 8, 1, 9, 3]);
        assert_eq!(t.query(0, 5), 9);
        assert_eq!(t.query(0, 0), 5);
        assert_eq!(t.query(0, 2), 8);
    }

    #[test]
    fn out_of_range_panics() {
        let t = SumSegTree::from_vec(vec![1, 2, 3]);
        let r = std::panic::catch_unwind(|| {
            t.query(0, 5);
        });
        assert!(r.is_err());
    }

    #[test]
    fn large_input_consistency() {
        let values: Vec<i64> = (1..=1000).collect();
        let t = SumSegTree::from_vec(values.clone());
        let sum: i64 = values.iter().sum();
        assert_eq!(t.query(0, 999), sum);
        assert_eq!(t.query(0, 99), values[0..100].iter().sum::<i64>());
        assert_eq!(t.query(900, 999), values[900..1000].iter().sum::<i64>());
    }

    #[test]
    fn update_after_many_queries() {
        let mut t = SumSegTree::from_vec(vec![10; 100]);
        for _ in 0..50 {
            assert_eq!(t.query(0, 99), 1000);
        }
        t.update(50, 1000);
        assert_eq!(t.query(0, 99), 1000 + 990);
        assert_eq!(t.query(0, 49), 500);
        assert_eq!(t.query(50, 50), 1000);
    }

    #[test]
    fn empty_tree() {
        let t = SumSegTree::from_vec(Vec::new());
        // next_power_of_two(0) is 1; we have one slot, all identity.
        assert_eq!(t.len(), 1);
    }
}
