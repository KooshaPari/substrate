//! Quickselect: O(n) average-case selection of the k-th order statistic.
//!
//! Quickselect is a selection algorithm that finds the k-th smallest element
//! in an unordered list. It is related to Quicksort but recurses into only
//! one partition at each step, giving expected linear time. Worst case is
//! quadratic for adversarial inputs; this implementation mitigates that by
//! randomizing the pivot choice (a uniformly-random pivot index is selected
//! at each level).
//!
//! Reference: Hoare, "Algorithm 65 (Find)" (1961); commonly attributed
//! description: <https://en.wikipedia.org/wiki/Quickselect>.
//!
//! Both [`quickselect`] (returns the k-th smallest value) and
//! [`quickselect_in_place`] (reorders the slice so the k-th smallest is at
//! position k) are provided.

use rand::{Rng, SeedableRng};
use rand::rngs::StdRng;

/// Returns the k-th smallest element of `arr` (0-indexed).
///
/// Panics if `arr` is empty or `k >= arr.len()`.
pub fn quickselect<T: Ord + Clone>(arr: &[T], k: usize) -> T {
    assert!(!arr.is_empty(), "quickselect: empty input");
    assert!(k < arr.len(), "quickselect: k out of range");
    let mut work = arr.to_vec();
    let mut rng = StdRng::seed_from_u64(0xc0ffee);
    select_in_place(&mut work, k, &mut rng);
    work[k].clone()
}

/// Reorders `arr` so that the element at position `k` is the k-th smallest
/// element, with all smaller elements before it and all greater (or equal)
/// elements at or after it.
///
/// Panics if `arr` is empty or `k >= arr.len()`.
pub fn quickselect_in_place<T: Ord + Clone, R: Rng>(arr: &mut [T], k: usize, rng: &mut R) {
    assert!(!arr.is_empty(), "quickselect_in_place: empty input");
    assert!(k < arr.len(), "quickselect_in_place: k out of range");
    select_in_place(arr, k, rng);
}

fn select_in_place<T: Ord + Clone, R: Rng>(arr: &mut [T], k: usize, rng: &mut R) {
    let mut lo = 0usize;
    let mut hi = arr.len();
    loop {
        if hi - lo <= 1 {
            return;
        }
        // Random pivot index in [lo, hi).
        let pivot_idx = lo + rng.gen_range(0..(hi - lo));
        arr.swap(pivot_idx, hi - 1);
        let pivot_val = arr[hi - 1].clone();
        // Lomuto partition with pivot at hi-1.
        let mut i = lo;
        for j in lo..hi - 1 {
            if arr[j] < pivot_val {
                arr.swap(i, j);
                i += 1;
            }
        }
        arr.swap(i, hi - 1);
        if k < i {
            hi = i;
        } else if k > i {
            lo = i + 1;
        } else {
            return;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn median_of_three() {
        let arr = vec![3u32, 1, 2];
        // k=0 → 1, k=1 → 2, k=2 → 3
        assert_eq!(quickselect(&arr, 0), 1);
        assert_eq!(quickselect(&arr, 1), 2);
        assert_eq!(quickselect(&arr, 2), 3);
    }

    #[test]
    fn already_sorted() {
        let arr: Vec<i32> = (0..50).collect();
        for k in 0..50 {
            assert_eq!(quickselect(&arr, k), k as i32);
        }
    }

    #[test]
    fn reverse_sorted() {
        let arr: Vec<i32> = (0..50).rev().collect();
        for k in 0..50 {
            assert_eq!(quickselect(&arr, k), k as i32);
        }
    }

    #[test]
    fn random_order_matches_sort() {
        let arr = vec![17, 4, 92, 1, 33, 8, 64, 2, 50, 7, 21, 5];
        let mut sorted = arr.clone();
        sorted.sort();
        for k in 0..arr.len() {
            assert_eq!(quickselect(&arr, k), sorted[k]);
        }
    }

    #[test]
    fn all_equal() {
        let arr = vec![42i32; 17];
        for k in 0..arr.len() {
            assert_eq!(quickselect(&arr, k), 42);
        }
    }

    #[test]
    fn duplicates() {
        let arr = vec![5, 1, 5, 2, 5, 3, 1, 5];
        let mut sorted = arr.clone();
        sorted.sort();
        for k in 0..arr.len() {
            assert_eq!(quickselect(&arr, k), sorted[k]);
        }
    }

    #[test]
    fn single_element() {
        assert_eq!(quickselect(&[99u32], 0), 99);
    }

    #[test]
    fn in_place_partition() {
        let mut arr = vec![5, 1, 4, 2, 3];
        let mut rng = StdRng::seed_from_u64(7);
        quickselect_in_place(&mut arr, 2, &mut rng);
        // After partition, position 2 holds the 3rd smallest (= 3); elements
        // at positions <2 are <= 3; positions >2 are >= 3.
        assert_eq!(arr[2], 3);
        for v in &arr[..2] {
            assert!(*v <= 3);
        }
        for v in &arr[3..] {
            assert!(*v >= 3);
        }
    }

    #[test]
    #[should_panic(expected = "out of range")]
    fn k_out_of_range_panics() {
        let arr = vec![1u32, 2, 3];
        let _ = quickselect(&arr, 3);
    }

    #[test]
    #[should_panic(expected = "empty input")]
    fn empty_input_panics() {
        let arr: Vec<u32> = vec![];
        let _ = quickselect(&arr, 0);
    }
}