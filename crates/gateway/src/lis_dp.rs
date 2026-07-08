//! Longest Increasing Subsequence (LIS).
//!
//! Given a sequence of comparable elements, find the length of the longest
//! strictly-increasing subsequence, and reconstruct one such subsequence.
//!
//! Two algorithms are provided:
//!
//! - `lis_length_dp`: O(n²) dynamic programming. Simple, easy to verify.
//! - `lis_length_patience`: O(n log n) patience-sort variant (recommended).
//! - `lis_reconstruct`: O(n log n) reconstruction that returns one optimal
//!   subsequence, not just its length.
//!
//! All algorithms handle empty input (returns 0) and single-element input.

/// O(n²) DP. `lt(a, b)` is the strict less-than test for the element type.
pub fn lis_length_dp<T, F>(seq: &[T], lt: F) -> usize
where
    F: Fn(&T, &T) -> bool,
{
    if seq.is_empty() {
        return 0;
    }
    let mut dp = vec![1usize; seq.len()];
    let mut best = 1usize;
    for i in 1..seq.len() {
        for j in 0..i {
            if lt(&seq[j], &seq[i]) {
                let cand = dp[j] + 1;
                if cand > dp[i] {
                    dp[i] = cand;
                }
            }
        }
        if dp[i] > best {
            best = dp[i];
        }
    }
    best
}

/// O(n log n) patience-sort. Returns the length of the longest strictly-
/// increasing subsequence.
///
/// `lt` must define a strict total order on the element type.
pub fn lis_length_patience<T, F>(seq: &[T], lt: F) -> usize
where
    F: Fn(&T, &T) -> bool,
{
    if seq.is_empty() {
        return 0;
    }
    // tails[i] = index of the smallest possible tail of an increasing
    // subsequence of length i+1.
    let mut tails: Vec<usize> = Vec::with_capacity(seq.len());
    // predecessor[i] = index of the previous element in the subsequence
    // ending at i.
    let mut predecessor: Vec<Option<usize>> = vec![None; seq.len()];
    let mut best_len = 0usize;

    for (i, x) in seq.iter().enumerate() {
        // Binary search for the leftmost tail >= x (using strict-less).
        let pos = tails
            .binary_search_by(|&t| {
                // We want: tails[t] < x → Less; tails[t] >= x → Greater.
                if lt(&seq[t], x) {
                    std::cmp::Ordering::Less
                } else {
                    std::cmp::Ordering::Greater
                }
            })
            .unwrap_or_else(|e| e);
        if pos > 0 {
            predecessor[i] = Some(tails[pos - 1]);
        }
        if pos == tails.len() {
            tails.push(i);
        } else {
            tails[pos] = i;
        }
        if pos + 1 > best_len {
            best_len = pos + 1;
        }
    }
    best_len
}

/// O(n log n) reconstruction. Returns one optimal (strictly) increasing
/// subsequence in original order.
pub fn lis_reconstruct<T, F>(seq: &[T], lt: F) -> Vec<usize>
where
    T: Clone,
    F: Fn(&T, &T) -> bool,
{
    if seq.is_empty() {
        return Vec::new();
    }
    let mut tails: Vec<usize> = Vec::with_capacity(seq.len());
    let mut predecessor: Vec<Option<usize>> = vec![None; seq.len()];

    for (i, x) in seq.iter().enumerate() {
        let pos = tails
            .binary_search_by(|&t| {
                if lt(&seq[t], x) {
                    std::cmp::Ordering::Less
                } else {
                    std::cmp::Ordering::Greater
                }
            })
            .unwrap_or_else(|e| e);
        if pos > 0 {
            predecessor[i] = Some(tails[pos - 1]);
        }
        if pos == tails.len() {
            tails.push(i);
        } else {
            tails[pos] = i;
        }
    }
    let mut path = Vec::new();
    let mut cur = tails.last().copied();
    while let Some(idx) = cur {
        path.push(idx);
        cur = predecessor[idx];
    }
    path.reverse();
    path
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_sequence() {
        let v: Vec<i32> = vec![];
        assert_eq!(lis_length_dp(&v, |a, b| a < b), 0);
        assert_eq!(lis_length_patience(&v, |a, b| a < b), 0);
        assert!(lis_reconstruct(&v, |a, b| a < b).is_empty());
    }

    #[test]
    fn single_element() {
        let v = vec![42];
        assert_eq!(lis_length_dp(&v, |a, b| a < b), 1);
        assert_eq!(lis_length_patience(&v, |a, b| a < b), 1);
        assert_eq!(lis_reconstruct(&v, |a, b| a < b), vec![0]);
    }

    #[test]
    fn sorted_ascending() {
        let v = vec![1, 2, 3, 4, 5];
        assert_eq!(lis_length_dp(&v, |a, b| a < b), 5);
        assert_eq!(lis_length_patience(&v, |a, b| a < b), 5);
        assert_eq!(lis_reconstruct(&v, |a, b| a < b), vec![0, 1, 2, 3, 4]);
    }

    #[test]
    fn sorted_descending() {
        let v = vec![5, 4, 3, 2, 1];
        assert_eq!(lis_length_dp(&v, |a, b| a < b), 1);
        assert_eq!(lis_length_patience(&v, |a, b| a < b), 1);
    }

    #[test]
    fn classic_example() {
        // 10, 9, 2, 5, 3, 7, 101, 18 → LIS = 4 (e.g. 2,3,7,18 or 2,5,7,101)
        let v = vec![10, 9, 2, 5, 3, 7, 101, 18];
        assert_eq!(lis_length_dp(&v, |a, b| a < b), 4);
        assert_eq!(lis_length_patience(&v, |a, b| a < b), 4);
        let recon = lis_reconstruct(&v, |a, b| a < b);
        assert_eq!(recon.len(), 4);
        // Verify it's actually increasing
        for w in recon.windows(2) {
            assert!(v[w[0]] < v[w[1]]);
        }
    }

    #[test]
    fn duplicates_are_not_strictly_increasing() {
        let v = vec![1, 1, 1, 1];
        assert_eq!(lis_length_dp(&v, |a, b| a < b), 1);
        assert_eq!(lis_length_patience(&v, |a, b| a < b), 1);
    }

    #[test]
    fn works_with_strings() {
        let v = vec!["a", "b", "a", "c", "d"];
        assert_eq!(lis_length_dp(&v, |a, b| a < b), 4);
        assert_eq!(lis_length_patience(&v, |a, b| a < b), 4);
    }

    #[test]
    fn stress_random_matches_dp() {
        // Verify patience-sort agrees with DP on small inputs.
        let mut seq = Vec::new();
        let mut x: u32 = 1;
        for _ in 0..30 {
            x = x.wrapping_mul(1103515245).wrapping_add(12345);
            seq.push(x % 50);
        }
        let dp = lis_length_dp(&seq, |a, b| a < b);
        let pat = lis_length_patience(&seq, |a, b| a < b);
        assert_eq!(dp, pat);
    }

    #[test]
    fn reconstruct_preserves_original_order() {
        let v = vec![0, 8, 4, 12, 2, 10, 6, 14, 1, 9];
        let recon = lis_reconstruct(&v, |a, b| a < b);
        // Indices are strictly increasing
        for w in recon.windows(2) {
            assert!(w[0] < w[1]);
        }
        // Values are strictly increasing
        let values: Vec<i32> = recon.iter().map(|&i| v[i]).collect();
        for w in values.windows(2) {
            assert!(w[0] < w[1]);
        }
        // Length should be the LIS
        assert_eq!(recon.len(), lis_length_patience(&v, |a, b| a < b));
    }
}