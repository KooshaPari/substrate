//! Classic in-place sorting algorithms.
//!
//! Implementations are intentionally textbook and not the most
//! performant possible (no pattern-defeating quick sort, no
//! TimSort-style runs, no introsort). Use [`std::slice::sort_unstable`]
//! or the `rayon` par_* primitives for production code. This module
//! is for clarity and teaching.
//!
//! All functions sort in ascending order using the [`Ord`] trait and
//! take a `&mut [T]`. They are unstable-agnostic: equal elements may
//! be reordered.

use std::cmp::Ord;

/// Merge sort — stable, O(n log n) worst-case. Requires a `Vec` clone
/// of the input for the merge step (O(n) auxiliary space).
pub fn merge_sort<T: Ord + Clone>(slice: &mut [T]) {
    let n = slice.len();
    if n < 2 {
        return;
    }
    let mut aux: Vec<T> = slice.to_vec();
    merge_sort_helper(slice, &mut aux, 0, n);
}

fn merge_sort_helper<T: Ord + Clone>(slice: &mut [T], aux: &mut Vec<T>, lo: usize, hi: usize) {
    if hi - lo < 2 {
        return;
    }
    let mid = lo + (hi - lo) / 2;
    merge_sort_helper(slice, aux, lo, mid);
    merge_sort_helper(slice, aux, mid, hi);
    // Skip merge if already sorted.
    if slice[mid - 1] <= slice[mid] {
        return;
    }
    // Merge the two halves into aux then copy back.
    for (i, item) in slice[lo..hi].iter().enumerate() {
        aux[lo + i] = item.clone();
    }
    let (mut i, mut j) = (lo, mid);
    for k in lo..hi {
        if i < mid && (j >= hi || aux[i] <= aux[j]) {
            slice[k] = aux[i].clone();
            i += 1;
        } else {
            slice[k] = aux[j].clone();
            j += 1;
        }
    }
}

/// Heap sort — in-place, O(n log n) worst-case, not stable.
pub fn heap_sort<T: Ord>(slice: &mut [T]) {
    let n = slice.len();
    if n < 2 {
        return;
    }
    // Build a max-heap.
    for i in (0..n / 2).rev() {
        sift_down(slice, i, n);
    }
    // Repeatedly extract the max and shrink the heap.
    for end in (1..n).rev() {
        slice.swap(0, end);
        sift_down(slice, 0, end);
    }
}

fn sift_down<T: Ord>(slice: &mut [T], mut root: usize, end: usize) {
    loop {
        let left = 2 * root + 1;
        let right = 2 * root + 2;
        if left >= end {
            break;
        }
        let mut largest = root;
        if slice[left] > slice[largest] {
            largest = left;
        }
        if right < end && slice[right] > slice[largest] {
            largest = right;
        }
        if largest == root {
            break;
        }
        slice.swap(root, largest);
        root = largest;
    }
}

/// Quick sort — Lomuto partition, in-place, O(n log n) average,
/// O(n²) worst case on already-sorted input. For teaching; not
/// resistant to median-of-three or pattern-defeating pivot selection.
pub fn quick_sort<T: Ord>(slice: &mut [T]) {
    if slice.len() < 2 {
        return;
    }
    quick_sort_helper(slice, 0, slice.len() - 1);
}

fn quick_sort_helper<T: Ord>(slice: &mut [T], lo: usize, hi: usize) {
    if lo >= hi {
        return;
    }
    // Median-of-three pivot to mitigate O(n²) on sorted input.
    let mid = lo + (hi - lo) / 2;
    if slice[mid] < slice[lo] {
        slice.swap(lo, mid);
    }
    if slice[hi] < slice[lo] {
        slice.swap(lo, hi);
    }
    if slice[hi] < slice[mid] {
        slice.swap(mid, hi);
    }
    // Place pivot at `hi - 1` and partition.
    slice.swap(mid, hi);
    let pivot = hi;
    let mut i = lo;
    for j in lo..hi {
        if slice[j] <= slice[pivot] {
            slice.swap(i, j);
            i += 1;
        }
    }
    slice.swap(i, pivot);
    if i > 0 {
        quick_sort_helper(slice, lo, i - 1);
    }
    quick_sort_helper(slice, i + 1, hi);
}

/// Insertion sort — stable, O(n²) worst-case, but linear time on
/// nearly-sorted input. Good for small or nearly-sorted slices.
pub fn insertion_sort<T: Ord>(slice: &mut [T]) {
    for i in 1..slice.len() {
        let mut j = i;
        while j > 0 && slice[j - 1] > slice[j] {
            slice.swap(j - 1, j);
            j -= 1;
        }
    }
}

/// Binary-search a sorted slice for `needle`. Returns the position
/// at which `needle` should be inserted to keep the slice sorted
/// (lower_bound semantics).
pub fn lower_bound<T: Ord>(slice: &[T], needle: &T) -> usize {
    let mut lo = 0usize;
    let mut hi = slice.len();
    while lo < hi {
        let mid = lo + (hi - lo) / 2;
        if &slice[mid] < needle {
            lo = mid + 1;
        } else {
            hi = mid;
        }
    }
    lo
}

#[cfg(test)]
mod tests {
    use super::*;

    fn check(slice: &[i32]) {
        let mut sorted: Vec<i32> = slice.to_vec();
        sorted.sort();
        let mut s = slice.to_vec();
        merge_sort(&mut s);
        assert_eq!(s, sorted, "merge_sort({:?})", slice);

        let mut s = slice.to_vec();
        heap_sort(&mut s);
        assert_eq!(s, sorted, "heap_sort({:?})", slice);

        let mut s = slice.to_vec();
        quick_sort(&mut s);
        assert_eq!(s, sorted, "quick_sort({:?})", slice);

        let mut s = slice.to_vec();
        insertion_sort(&mut s);
        assert_eq!(s, sorted, "insertion_sort({:?})", slice);
    }

    #[test]
    fn empty() {
        let mut s: [i32; 0] = [];
        merge_sort(&mut s);
        heap_sort(&mut s);
        quick_sort(&mut s);
        insertion_sort(&mut s);
        let expected: [i32; 0] = [];
        assert_eq!(s, expected);
    }

    #[test]
    fn single() {
        let mut s = [42];
        merge_sort(&mut s);
        assert_eq!(s, [42]);
    }

    #[test]
    fn two() {
        check(&[2, 1]);
        check(&[1, 2]);
    }

    #[test]
    fn random() {
        check(&[3, 1, 4, 1, 5, 9, 2, 6, 5, 3, 5]);
    }

    #[test]
    fn already_sorted() {
        check(&[1, 2, 3, 4, 5, 6, 7, 8, 9, 10]);
    }

    #[test]
    fn reverse_sorted() {
        check(&[10, 9, 8, 7, 6, 5, 4, 3, 2, 1]);
    }

    #[test]
    fn all_equal() {
        check(&[5, 5, 5, 5, 5]);
    }

    #[test]
    fn large_input() {
        let mut v: Vec<i32> = (0..1000).rev().collect();
        merge_sort(&mut v);
        assert!(v.windows(2).all(|w| w[0] <= w[1]));
    }

    #[test]
    fn lower_bound_basic() {
        let s = [1, 3, 5, 7, 9];
        assert_eq!(lower_bound(&s, &0), 0);
        assert_eq!(lower_bound(&s, &1), 0);
        assert_eq!(lower_bound(&s, &4), 2);
        assert_eq!(lower_bound(&s, &5), 2);
        assert_eq!(lower_bound(&s, &10), 5);
    }

    #[test]
    fn lower_bound_empty() {
        let s: [i32; 0] = [];
        assert_eq!(lower_bound(&s, &5), 0);
    }
}
