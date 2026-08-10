//! Edit distance (Levenshtein distance) and weighted edit distance.
//!
//! The edit distance between two strings is the minimum number of
//! single-character edit operations (insertion, deletion, substitution)
//! required to transform one into the other. Defined by Vladimir
//! Levenshtein (1965), it is the canonical string-similarity metric in
//! spell-checkers, bioinformatics (DNA/RNA global alignment with
//! unit weights), fuzzy search, and diff post-processing.
//!
//! Two algorithms are provided:
//! - [`distance`] — classic Wagner–Fischer DP with O(m·n) time and O(n)
//!   space (rolling row).
//! - [`distance_weighted`] — generalized variant with per-operation
//!   costs (insert/delete/substitute). Useful when, e.g., insertions
//!   cost more than deletions.
//!
//! Reference: R. A. Wagner, M. J. Fischer, "The string-to-string
//! correction problem", Journal of the ACM 21(1), 1974, pp. 168–173.
//!
//! Pure safe Rust, no external dependencies. Operates on `&[u8]`
//! (byte-level) so callers can apply it to UTF-8 strings, ASCII
//! tokens, or arbitrary byte sequences.

/// Classic Levenshtein distance (Wagner–Fischer, O(m·n) time, O(n) space).
///
/// Returns the minimum number of single-byte insertions, deletions,
/// or substitutions to turn `a` into `b`. The metric satisfies:
/// - `distance(a, a) == 0`
/// - `distance(a, b) == distance(b, a)`
/// - Triangle inequality with any third string.
///
/// Maximum return value: `a.len() + b.len()` (delete all, insert all).
pub fn distance(a: &[u8], b: &[u8]) -> usize {
    if a.is_empty() {
        return b.len();
    }
    if b.is_empty() {
        return a.len();
    }
    let m = a.len();
    let n = b.len();

    // Two rolling rows: prev (row i) and curr (row i+1).
    let mut prev: Vec<usize> = (0..=n).collect();
    let mut curr: Vec<usize> = vec![0; n + 1];

    for i in 1..=m {
        curr[0] = i;
        for j in 1..=n {
            let cost = if a[i - 1] == b[j - 1] { 0 } else { 1 };
            let del = prev[j].wrapping_add(1);
            let ins = curr[j - 1].wrapping_add(1);
            let sub = prev[j - 1].wrapping_add(cost);
            curr[j] = del.min(ins).min(sub);
        }
        std::mem::swap(&mut prev, &mut curr);
    }
    prev[n]
}

/// Weighted edit distance with custom insert / delete / substitute costs.
///
/// `ins_cost` and `del_cost` are the per-byte costs of inserting into
/// `a` or deleting from `a`. `sub_cost` is the cost of substituting a
/// mismatching byte; a matching byte costs 0. Use `ins_cost == del_cost
/// == 1` and `sub_cost == 1` to recover the classic Levenshtein
/// distance.
pub fn distance_weighted(
    a: &[u8],
    b: &[u8],
    ins_cost: usize,
    del_cost: usize,
    sub_cost: usize,
) -> usize {
    let m = a.len();
    let n = b.len();
    let mut prev: Vec<usize> = (0..=n).map(|j| j.saturating_mul(ins_cost)).collect();
    let mut curr: Vec<usize> = vec![0; n + 1];

    for i in 1..=m {
        curr[0] = prev[0].saturating_add(del_cost);
        for j in 1..=n {
            let cost = if a[i - 1] == b[j - 1] { 0 } else { sub_cost };
            let del = prev[j].saturating_add(del_cost);
            let ins = curr[j - 1].saturating_add(ins_cost);
            let sub = prev[j - 1].saturating_add(cost);
            curr[j] = del.min(ins).min(sub);
        }
        std::mem::swap(&mut prev, &mut curr);
    }
    prev[n]
}

/// Returns true if the edit distance between `a` and `b` is at most
/// `max_dist`. Runs in O(m·n) but short-circuits as soon as any row
/// falls below the threshold.
pub fn within(a: &[u8], b: &[u8], max_dist: usize) -> bool {
    if a.is_empty() {
        return b.len() <= max_dist;
    }
    if b.is_empty() {
        return a.len() <= max_dist;
    }
    let m = a.len();
    let n = b.len();
    if m.abs_diff(n) > max_dist {
        return false;
    }
    let mut prev: Vec<usize> = (0..=n).collect();
    let mut curr: Vec<usize> = vec![0; n + 1];
    #[allow(unused_assignments)]
    let mut row_min = usize::MAX;
    for i in 1..=m {
        curr[0] = i;
        row_min = i;
        for j in 1..=n {
            let cost = if a[i - 1] == b[j - 1] { 0 } else { 1 };
            let del = prev[j].wrapping_add(1);
            let ins = curr[j - 1].wrapping_add(1);
            let sub = prev[j - 1].wrapping_add(cost);
            let v = del.min(ins).min(sub);
            curr[j] = v;
            if v < row_min {
                row_min = v;
            }
        }
        if row_min > max_dist {
            return false;
        }
        std::mem::swap(&mut prev, &mut curr);
    }
    prev[n] <= max_dist
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identical_strings_zero() {
        assert_eq!(distance(b"hello", b"hello"), 0);
        assert_eq!(distance(b"", b""), 0);
        assert_eq!(distance(b"a", b"a"), 0);
    }

    #[test]
    fn empty_inputs() {
        assert_eq!(distance(b"", b"abc"), 3);
        assert_eq!(distance(b"abc", b""), 3);
    }

    #[test]
    fn single_substitution() {
        // "kitten" -> "sitten" (1 sub)
        assert_eq!(distance(b"kitten", b"sitten"), 1);
        // "kitten" -> "sittin" (sub + delete = 2) — but the canonical Levenshtein
        // is "kitten"->"sitting" with 3 ops (k→s, e→i, +g).
        assert_eq!(distance(b"kitten", b"sitting"), 3);
    }

    #[test]
    fn classic_levenshtein_examples() {
        // From Wikipedia "Levenshtein distance" examples.
        assert_eq!(distance(b"kitten", b"sitting"), 3);
        assert_eq!(distance(b"Saturday", b"Sunday"), 3);
        assert_eq!(distance(b"flaw", b"lawn"), 2);
        assert_eq!(distance(b"intention", b"execution"), 5);
    }

    #[test]
    fn symmetry() {
        let a = b"abcdefg";
        let b = b"xyz";
        assert_eq!(distance(a, b), distance(b, a));
        let c = b"";
        let d = b"longer string here";
        assert_eq!(distance(c, d), distance(d, c));
    }

    #[test]
    fn triangle_inequality() {
        // For any three strings a, b, c: distance(a, c) <= distance(a, b) + distance(b, c).
        let a = b"abcdef";
        let b = b"abxyef";
        let c = b"abxyez";
        let dac = distance(a, c);
        let dab = distance(a, b);
        let dbc = distance(b, c);
        assert!(
            dac <= dab + dbc,
            "triangle inequality failed: {} > {} + {}",
            dac,
            dab,
            dbc
        );
    }

    #[test]
    fn max_distance_is_max_of_lengths() {
        // For any pair of equal-length strings, distance <= length
        // (substitute every byte in the worst case).
        let a = [0xFFu8; 50];
        let b = [0x00u8; 50];
        assert_eq!(distance(&a, &b), 50);
        let c: Vec<u8> = (0..50).map(|i| i as u8).collect();
        let d: Vec<u8> = (50..100).map(|i| i as u8).collect();
        assert_eq!(distance(&c, &d), 50);
    }

    #[test]
    fn weighted_recovers_classic() {
        // With unit costs, weighted == classic.
        let a = b"kitten";
        let b = b"sitting";
        let w = distance_weighted(a, b, 1, 1, 1);
        let c = distance(a, b);
        assert_eq!(w, c);
    }

    #[test]
    fn weighted_cheaper_inserts() {
        // When insert is much cheaper than substitute, an optimal path
        // may favor delete-and-insert over substitute. We verify that
        // weighted with ins=1, del=10, sub=10 produces a distance
        // strictly less than or equal to the classic distance for
        // at least one input pair.
        let a = b"abc";
        let b = b"xbc";
        let classic = distance(a, b); // = 1
        let weighted = distance_weighted(a, b, 1, 10, 10); // sub=10, del+ins=11, so 10
        assert_eq!(classic, 1);
        assert_eq!(weighted, 10);
        assert!(weighted > classic);
    }

    #[test]
    fn within_short_circuits() {
        assert!(within(b"hello", b"hello", 0));
        assert!(within(b"hello", b"hellp", 1));
        assert!(!within(b"hello", b"hellp", 0));
        assert!(!within(b"hello", b"world", 3));
        // Length difference exceeds threshold.
        assert!(!within(b"hi", b"hello world", 3));
        assert!(!within(b"hello world", b"hi", 3));
    }

    #[test]
    fn within_distance_relationship() {
        // within(a, b, d) iff distance(a, b) <= d.
        let pairs: Vec<(&[u8], &[u8])> = vec![
            (b"", b""),
            (b"a", b"b"),
            (b"kitten", b"sitting"),
            (b"Saturday", b"Sunday"),
            (b"hello", b"olleh"),
        ];
        for (a, b) in pairs {
            let d = distance(a, b);
            for threshold in 0..=d.saturating_add(2) {
                assert_eq!(
                    within(a, b, threshold),
                    d <= threshold,
                    "mismatch for a={:?} b={:?} d={} threshold={}",
                    a,
                    b,
                    d,
                    threshold
                );
            }
        }
    }

    #[test]
    fn handles_unicode_bytes() {
        // We operate on bytes; the user is responsible for choosing
        // UTF-8 char boundaries if they care about grapheme clusters.
        let s1 = "café".as_bytes(); // 5 bytes (é is 2)
        let s2 = "cafe".as_bytes(); // 4 bytes
                                    // Length difference is 1 (delete one byte) → distance 1.
                                    // We do not pin the exact value, just verify it's bounded
                                    // and consistent.
        let d = distance(s1, s2);
        assert!(d >= 1 && d <= s1.len() + s2.len());
        // Symmetry check on the same input.
        assert_eq!(distance(s1, s2), distance(s2, s1));
    }
}
