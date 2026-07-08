//! Fenwick tree (Binary Indexed Tree) for cumulative-sum queries.
//!
//! Supports:
//! - `add(i, delta)` — point update in O(log n).
//! - `prefix_sum(i)` — sum of elements [0..=i] in O(log n).
//! - `range_sum(lo, hi)` — sum of [lo..=hi] via two prefix sums.
//! - `find_kth(k)` — smallest index `i` with `prefix_sum(i) >= k`
//!   in O(log n). Assumes the array contains non-negative values.
//!
//! Internally indexed 1-based (the standard Fenwick trick) to make
//! the parent/index math branch-free. The public API is 0-based.
//!
//! Reference: Peter Fenwick, "A New Data Structure for Cumulative
//! Frequency Tables" (1994).

/// A Fenwick tree over `n` slots (0-based indexing in the public API).
#[derive(Debug, Clone)]
pub struct FenwickTree {
    tree: Vec<u64>,
    n: usize,
}

impl FenwickTree {
    /// Create a zeroed tree of size `n`. Panics if `n == 0`.
    pub fn new(n: usize) -> Self {
        assert!(n > 0, "Fenwick tree requires n > 0");
        Self {
            tree: vec![0; n + 1],
            n,
        }
    }

    /// Create a tree pre-populated from an initial array of length `n`.
    pub fn from_array(values: &[u64]) -> Self {
        let mut t = Self::new(values.len());
        for (i, &v) in values.iter().enumerate() {
            t.add(i, v);
        }
        t
    }

    pub fn len(&self) -> usize {
        self.n
    }

    pub fn is_empty(&self) -> bool {
        self.n == 0
    }

    /// Add `delta` to the element at index `i` (0-based). Panics
    /// if `i >= n`.
    pub fn add(&mut self, i: usize, delta: u64) {
        assert!(i < self.n, "fenwick add index {} out of range {}", i, self.n);
        let mut idx = i + 1; // 1-based
        while idx <= self.n {
            // Wrapping add: u64 wraps around, but that would be a bug
            // if the tree is used for non-negative cumulative values.
            // Saturate instead to keep sums well-defined.
            let (sum, overflow) = self.tree[idx].overflowing_add(delta);
            self.tree[idx] = sum;
            if overflow {
                panic!("fenwick tree u64 overflow at index {idx}");
            }
            idx += idx & idx.wrapping_neg(); // i & -i in two's complement
        }
    }

    /// Sum of elements in `[0..=i]`. Panics if `i >= n`.
    pub fn prefix_sum(&self, i: usize) -> u64 {
        assert!(i < self.n, "fenwick prefix sum index {} out of range {}", i, self.n);
        let mut idx = i + 1;
        let mut sum: u64 = 0;
        while idx > 0 {
            sum += self.tree[idx];
            idx -= idx & idx.wrapping_neg();
        }
        sum
    }

    /// Sum of elements in `[lo..=hi]`. Returns 0 if `lo > hi`.
    pub fn range_sum(&self, lo: usize, hi: usize) -> u64 {
        if lo > hi {
            return 0;
        }
        assert!(hi < self.n, "fenwick range_sum hi {} out of range {}", hi, self.n);
        if lo == 0 {
            self.prefix_sum(hi)
        } else {
            self.prefix_sum(hi) - self.prefix_sum(lo - 1)
        }
    }

    /// Find the smallest index `i` such that `prefix_sum(i) >= k`.
    /// Returns `None` if the total sum is less than `k`. Assumes the
    /// tree contains only non-negative values.
    pub fn find_kth(&self, k: u64) -> Option<usize> {
        // Find highest power of 2 <= n.
        let mut bit: usize = 1;
        while bit <= self.n {
            bit <<= 1;
        }
        bit >>= 1;
        let mut idx = 0usize;
        let mut sum = 0u64;
        while bit > 0 {
            let next = idx + bit;
            if next <= self.n && sum + self.tree[next] < k {
                sum += self.tree[next];
                idx = next;
            }
            bit >>= 1;
        }
        if idx >= self.n {
            None
        } else {
            Some(idx) // we ended at the prefix just below k; result is idx (0-based)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_tree() {
        let t = FenwickTree::new(5);
        for i in 0..5 {
            assert_eq!(t.prefix_sum(i), 0);
        }
    }

    #[test]
    fn single_add() {
        let mut t = FenwickTree::new(5);
        t.add(3, 7);
        assert_eq!(t.prefix_sum(0), 0);
        assert_eq!(t.prefix_sum(1), 0);
        assert_eq!(t.prefix_sum(2), 0);
        assert_eq!(t.prefix_sum(3), 7);
        assert_eq!(t.prefix_sum(4), 7);
    }

    #[test]
    fn from_array_constructor() {
        let t = FenwickTree::from_array(&[1, 2, 3, 4, 5]);
        assert_eq!(t.prefix_sum(0), 1);
        assert_eq!(t.prefix_sum(4), 15);
        assert_eq!(t.range_sum(1, 3), 9); // 2+3+4
    }

    #[test]
    fn range_sum_basic() {
        let mut t = FenwickTree::new(8);
        for (i, &v) in [3u64, 1, 4, 1, 5, 9, 2, 6].iter().enumerate() {
            t.add(i, v);
        }
        assert_eq!(t.range_sum(0, 7), 31);
        assert_eq!(t.range_sum(0, 0), 3);
        assert_eq!(t.range_sum(7, 7), 6);
        assert_eq!(t.range_sum(2, 5), 19); // 4+1+5+9
        assert_eq!(t.range_sum(0, 3), 9);
    }

    #[test]
    fn range_sum_empty_when_lo_gt_hi() {
        let t = FenwickTree::new(5);
        assert_eq!(t.range_sum(4, 2), 0);
    }

    #[test]
    fn repeated_add() {
        let mut t = FenwickTree::new(5);
        t.add(2, 3);
        t.add(2, 5);
        assert_eq!(t.prefix_sum(2), 8);
    }

    #[test]
    fn find_kth_basic() {
        let t = FenwickTree::from_array(&[1, 2, 3, 4, 5]); // cumsums: 1, 3, 6, 10, 15
        assert_eq!(t.find_kth(1), Some(0));
        assert_eq!(t.find_kth(2), Some(1));
        assert_eq!(t.find_kth(3), Some(1));
        assert_eq!(t.find_kth(4), Some(2));
        assert_eq!(t.find_kth(5), Some(2));
        assert_eq!(t.find_kth(6), Some(2));
        assert_eq!(t.find_kth(15), Some(4));
        assert_eq!(t.find_kth(16), None);
    }

    #[test]
    fn find_kth_empty_total() {
        let t = FenwickTree::new(5);
        assert_eq!(t.find_kth(1), None);
    }

    #[test]
    fn panics_on_zero() {
        let r = std::panic::catch_unwind(|| {
            let _ = FenwickTree::new(0);
        });
        assert!(r.is_err());
    }

    #[test]
    fn large_input() {
        let values: Vec<u64> = (1..=1000).collect();
        let t = FenwickTree::from_array(&values);
        // Sum 1..=1000 = 500500.
        assert_eq!(t.prefix_sum(999), 500500);
        // values is 0-based: values[i] = i+1 for i in 0..1000.
        // range_sum(100, 999) sums values[100..=999] = 101..=1000 = 495450.
        assert_eq!(t.range_sum(100, 999), 495450);
    }
}