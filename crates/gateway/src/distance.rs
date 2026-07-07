//! Distance and similarity metrics for short strings.
//!
//! [`hamming`] counts position-by-position mismatches between two strings
//! of equal length, returning `|a.len() - b.len()|` plus per-character
//! differences. [`jaccard`] computes the Jaccard similarity over character
//! sets.
//!
//! Both functions are O(min(|a|, |b|)) and pull no external crates.

/// Hamming distance: the number of positions at which the corresponding
/// characters differ. Strings of unequal length contribute `|len(a) - len(b)|`
/// to the count.
///
/// Returns 0 for two equal inputs.
pub fn hamming(a: &str, b: &str) -> usize {
    let ca: Vec<char> = a.chars().collect();
    let cb: Vec<char> = b.chars().collect();
    let mut d = (ca.len() as i64 - cb.len() as i64).unsigned_abs() as usize;
    for (x, y) in ca.iter().zip(cb.iter()) {
        if x != y {
            d += 1;
        }
    }
    d
}

/// Jaccard similarity over character sets: |a ∩ b| / |a ∪ b|.
///
/// Returns 1.0 for two identical strings, 0.0 for fully disjoint character
/// sets. If both strings are empty the result is 1.0 (they are both the
/// empty set).
pub fn jaccard(a: &str, b: &str) -> f64 {
    let sa: std::collections::HashSet<char> = a.chars().collect();
    let sb: std::collections::HashSet<char> = b.chars().collect();
    let inter = sa.intersection(&sb).count() as f64;
    let union = sa.union(&sb).count() as f64;
    if union == 0.0 {
        1.0
    } else {
        inter / union
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hamming_equal_strings() {
        assert_eq!(hamming("abc", "abc"), 0);
    }

    #[test]
    fn hamming_one_difference() {
        assert_eq!(hamming("abc", "abd"), 1);
    }

    #[test]
    fn hamming_length_difference() {
        assert_eq!(hamming("ab", "abcde"), 3);
    }

    #[test]
    fn hamming_empty_inputs() {
        assert_eq!(hamming("", ""), 0);
        assert_eq!(hamming("", "x"), 1);
    }

    #[test]
    fn jaccard_identical_sets() {
        assert!((jaccard("abc", "abc") - 1.0).abs() < 1e-9);
    }

    #[test]
    fn jaccard_disjoint() {
        assert_eq!(jaccard("abc", ""), 0.0);
    }

    #[test]
    fn jaccard_partial_overlap() {
        let s = jaccard("abcdef", "bcdefg");
        // intersection = {b,c,d,e,f} = 5, union = {a,b,c,d,e,f,g} = 7
        assert!((s - 5.0 / 7.0).abs() < 1e-9);
    }
}