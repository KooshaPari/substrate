//! Levenshtein edit-distance utilities.
//!
//! Provides a classic dynamic-programming edit distance between two strings,
//! an early-termination variant bounded by a caller-supplied cap, and a
//! convenience helper that filters and ranks candidate strings by distance.
//!
//! The classic algorithm considers three single-character operations:
//!
//! - substitution (one character replaced by another),
//! - insertion (one character added), and
//! - deletion (one character removed).
//!
//! Each operation has unit cost; the distance is the minimum total cost to
//! transform one string into the other. A pure transposition (`ab` -> `ba`)
//! therefore costs `2`, not `1`, because at minimum one character must be
//! deleted and one inserted. If you need Damerau-style transposition-aware
//! distance, layer that on top of [`distance`].
//!
//! All distances here operate on Unicode scalar values (`char`) rather than
//! bytes. The cap-aware variant returns `cap + 1` whenever the true distance
//! is known to exceed the cap, which is useful as a sentinel for "too far"
//! without committing to an exact integer.
//!
//! # Examples
//!
//! ```
//! use substrate_gateway::levenshtein::{distance, suggest_within};
//!
//! assert_eq!(distance("kitten", "sitting"), 3);
//! assert_eq!(distance("ab", "ba"), 2); // pure transpose, classic Levenshtein
//!
//! let mut sugg = suggest_within("apple", &["apply", "aple", "banana"], 2);
//! assert_eq!(sugg.first().map(|(s, _)| s.as_str()), Some("aple"));
//! ```

/// Compute the classic Levenshtein edit distance between two strings.
///
/// Returns the minimum number of single-character insertions, deletions, or
/// substitutions required to transform `a` into `b`. The function is
/// symmetric in its arguments (`distance(a, b) == distance(b, a)`) and
/// treats Unicode scalar values as the atomic unit.
///
/// # Complexity
///
/// Time and space both `O(|a| * |b|)` in the lengths of the inputs, using
/// two rolling rows of `b.len() + 1` cells.
///
/// # Examples
///
/// ```
/// use substrate_gateway::levenshtein::distance;
///
/// assert_eq!(distance("", ""), 0);
/// assert_eq!(distance("abc", ""), 3);
/// assert_eq!(distance("", "abc"), 3);
/// assert_eq!(distance("cat", "bat"), 1); // single substitution
/// assert_eq!(distance("cat", "cats"), 1); // single insertion
/// assert_eq!(distance("cats", "cat"), 1); // single deletion
/// ```
pub fn distance(a: &str, b: &str) -> usize {
    // Fast-path empty inputs: cost is just the length of the other string.
    if a.is_empty() {
        return b.chars().count();
    }
    if b.is_empty() {
        return a.chars().count();
    }

    let a_chars: Vec<char> = a.chars().collect();
    let b_chars: Vec<char> = b.chars().collect();
    let n = b_chars.len();

    // Row 0: cost of inserting each prefix of b into the empty string.
    let mut prev: Vec<usize> = (0..=n).collect();
    let mut curr: Vec<usize> = vec![0; n + 1];

    for i in 1..=a_chars.len() {
        curr[0] = i; // cost of deleting a's first i chars to reach empty b
        let ac = a_chars[i - 1];
        let mut row_min = curr[0];
        for j in 1..=n {
            let cost = if ac == b_chars[j - 1] { 0 } else { 1 };
            let sub = prev[j - 1] + cost;
            let ins = curr[j - 1] + 1;
            let del = prev[j] + 1;
            let v = sub.min(ins).min(del);
            curr[j] = v;
            if v < row_min {
                row_min = v;
            }
        }
        // If the entire row stayed above `i`, the answer will be > i; we still
        // have to keep going because later rows could go lower, but if the
        // minimum cell in the row is already > a global bound the caller might
        // supply, they can short-circuit via `distance_with_cap`.
        std::mem::swap(&mut prev, &mut curr);
        // Silence unused warning while keeping the structure clear for future
        // banded-DP extensions.
        let _ = row_min;
    }
    prev[n]
}

/// Compute Levenshtein distance with early termination at `cap`.
///
/// Behaves like [`distance`] except that, as soon as the implementation can
/// prove the true distance exceeds `cap`, it returns `cap + 1` without
/// finishing the rest of the matrix. The off-by-one return value (`cap + 1`)
/// is a sentinel that is guaranteed to compare `>` `cap`.
///
/// The early termination uses the structural invariant
/// `dp[|a|][n] >= dp[i][n] - (|a| - i)`: each remaining row can decrease
/// the rightmost column by at most one, so once `dp[i][n]` exceeds
/// `cap + (|a| - i)` the final answer must also exceed `cap`. A symmetric
/// bound is checked against the length difference between `a` and `b`,
/// which lets us abort before touching the matrix at all when one string
/// is obviously much longer than the other.
///
/// # Examples
///
/// ```
/// use substrate_gateway::levenshtein::distance_with_cap;
///
/// // True distance is 3, cap is 5, so we get the exact value.
/// assert_eq!(distance_with_cap("kitten", "sitting", 5), 3);
/// // True distance is 6, cap is 3, so we get cap + 1 = 4.
/// assert_eq!(distance_with_cap("abcdef", "ghijkl", 3), 4);
/// // Exact match: distance 0 is always within any cap.
/// assert_eq!(distance_with_cap("abc", "abc", 0), 0);
/// ```
pub fn distance_with_cap(a: &str, b: &str, cap: usize) -> usize {
    // Empty-string short-circuits: we always know the exact distance.
    if a.is_empty() {
        let n = b.chars().count();
        return if n > cap { cap + 1 } else { n };
    }
    if b.is_empty() {
        let n = a.chars().count();
        return if n > cap { cap + 1 } else { n };
    }

    let a_chars: Vec<char> = a.chars().collect();
    let b_chars: Vec<char> = b.chars().collect();
    let m = a_chars.len();
    let n = b_chars.len();

    // Length-difference lower bound: even an optimal alignment needs at
    // least |m - n| edits. If that already exceeds cap, abort.
    if m.abs_diff(n) > cap {
        return cap + 1;
    }

    let mut prev: Vec<usize> = (0..=n).collect();
    let mut curr: Vec<usize> = vec![0; n + 1];

    for i in 1..=m {
        curr[0] = i;
        let ac = a_chars[i - 1];
        for j in 1..=n {
            let cost = if ac == b_chars[j - 1] { 0 } else { 1 };
            let sub = prev[j - 1] + cost;
            let ins = curr[j - 1] + 1;
            let del = prev[j] + 1;
            curr[j] = sub.min(ins).min(del);
        }
        // `dp[|a|][n] >= curr[n] - (m - i)`: each subsequent row can shave
        // off at most 1 from the rightmost cell (via a diagonal move).
        if curr[n] > cap + (m - i) {
            return cap + 1;
        }
        std::mem::swap(&mut prev, &mut curr);
    }
    prev[n]
}

/// Return every candidate within `max_dist` of `target`, sorted by distance.
///
/// Each result tuple is `(candidate_string, distance)`. The result is
/// ordered first by ascending distance and, to make the order deterministic
/// when distances tie, by ascending candidate string.
///
/// Candidates with distance greater than `max_dist` are omitted. The output
/// length is at most `candidates.len()`. Computing each distance uses
/// [`distance`]; if you have a tight cap, [`distance_with_cap`] can be
/// called directly and the comparison done in user code.
///
/// # Examples
///
/// ```
/// use substrate_gateway::levenshtein::suggest_within;
///
/// let sugg = suggest_within("apple", &["apply", "aple", "banana"], 2);
/// assert_eq!(sugg, vec![
///     ("aple".to_string(), 1),
///     ("apply".to_string(), 1),
/// ]);
/// ```
pub fn suggest_within(target: &str, candidates: &[&str], max_dist: usize) -> Vec<(String, usize)> {
    let mut out: Vec<(String, usize)> = candidates
        .iter()
        .filter_map(|c| {
            let d = distance(target, c);
            if d <= max_dist {
                Some((c.to_string(), d))
            } else {
                None
            }
        })
        .collect();
    // Stable sort: ascending distance, then ascending string for determinism.
    out.sort_by(|x, y| x.1.cmp(&y.1).then_with(|| x.0.cmp(&y.0)));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identical_strings_have_distance_zero() {
        assert_eq!(distance("hello", "hello"), 0);
        assert_eq!(distance("", ""), 0);
        assert_eq!(distance("a", "a"), 0);
    }

    #[test]
    fn single_insertion_costs_one() {
        assert_eq!(distance("cat", "cats"), 1);
        assert_eq!(distance("a", "ab"), 1);
        assert_eq!(distance("", "x"), 1);
    }

    #[test]
    fn single_deletion_costs_one() {
        assert_eq!(distance("cats", "cat"), 1);
        assert_eq!(distance("ab", "a"), 1);
        assert_eq!(distance("x", ""), 1);
    }

    #[test]
    fn single_substitution_costs_one() {
        assert_eq!(distance("cat", "bat"), 1);
        assert_eq!(distance("hello", "jello"), 1);
    }

    #[test]
    fn pure_transpose_costs_two_classic() {
        // Classic Levenshtein: ab -> ba needs one delete + one insert = 2.
        assert_eq!(distance("ab", "ba"), 2);
        assert_eq!(distance("abcd", "bacd"), 2);
    }

    #[test]
    fn empty_string_distance() {
        assert_eq!(distance("", ""), 0);
        assert_eq!(distance("hello", ""), 5);
        assert_eq!(distance("", "hello"), 5);
    }

    #[test]
    fn distance_with_cap_early_terminates_when_exceeds() {
        // True distance is 6, cap is 3 -> cap + 1 = 4.
        assert_eq!(distance_with_cap("abcdef", "ghijkl", 3), 4);
        // True distance is 6, cap is 6 -> still get exact 6.
        assert_eq!(distance_with_cap("abcdef", "ghijkl", 6), 6);
        // True distance is 3, cap is 5 -> get exact 3.
        assert_eq!(distance_with_cap("kitten", "sitting", 5), 3);
        // Exact match: 0, never over cap.
        assert_eq!(distance_with_cap("abc", "abc", 0), 0);
        // Identical non-empty strings with cap=0 still return 0.
        assert_eq!(distance_with_cap("anything", "anything", 0), 0);
        // Differ by one char with cap=0 -> returns 1 (1 <= 0+1, exact value).
        assert_eq!(distance_with_cap("abc", "abd", 0), 1);
    }

    #[test]
    fn distance_with_cap_returns_cap_plus_one_never_just_cap() {
        // Sentinel must be cap + 1, not cap, so callers can use `> cap`.
        let r = distance_with_cap("abc", "xyz", 0);
        assert_eq!(r, 1);
        let r = distance_with_cap("abcdefghij", "klmnopqrst", 4);
        assert_eq!(r, 5);
    }

    #[test]
    fn suggest_within_filters_by_max_distance() {
        let out = suggest_within("apple", &["apply", "aple", "banana"], 2);
        // "banana" is far away, filtered out. "apply" is dist 1 (drop e),
        // "aple" is dist 1 (drop p), both within max_dist 2.
        assert_eq!(
            out,
            vec![
                ("aple".to_string(), 1),
                ("apply".to_string(), 1),
            ]
        );
    }

    #[test]
    fn suggest_within_sorts_by_distance_then_lex() {
        let out = suggest_within("cat", &["bat", "cats", "car", "cut", "batty"], 3);
        // Distances: bat=1, car=1, cats=1, cut=1, batty=3.
        // Within 3, sorted by (dist, lex): bat, car, cats, cut, batty.
        assert_eq!(
            out,
            vec![
                ("bat".to_string(), 1),
                ("car".to_string(), 1),
                ("cats".to_string(), 1),
                ("cut".to_string(), 1),
                ("batty".to_string(), 3),
            ]
        );
    }

    #[test]
    fn suggest_within_empty_candidates_returns_empty() {
        let out = suggest_within("anything", &[], 5);
        assert!(out.is_empty());
    }

    #[test]
    fn distance_handles_unicode_chars() {
        // "café" -> "cafe": one deletion of 'é' (multi-byte but one char).
        assert_eq!(distance("café", "cafe"), 1);
        // Same length, differs in one char.
        assert_eq!(distance("café", "caff"), 1);
    }
}