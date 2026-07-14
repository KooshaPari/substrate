//! Knuth-Morris-Pratt string matching.
//!
//! Builds a failure function from the `needle` once, then runs `O(n+m)`
//! on the haystack. Useful when searching the same needle across many
//! texts, or when streaming a long text once.
//!
//! Reference: Knuth, Morris, Pratt, "Fast Pattern Matching in Strings"
//! (SIAM J. Comput. 1977).

/// Precompute the KMP failure function for `needle`.
fn build_failure(needle: &[u8]) -> Vec<usize> {
    let m = needle.len();
    let mut fail = vec![0usize; m];
    let mut k = 0usize;
    for i in 1..m {
        while k > 0 && needle[k] != needle[i] {
            k = fail[k - 1];
        }
        if needle[k] == needle[i] {
            k += 1;
        }
        fail[i] = k;
    }
    fail
}

/// Find all occurrences of `needle` in `haystack`. Returns the byte
/// offsets where each match begins. O(n+m).
pub fn kmp_find(haystack: &[u8], needle: &[u8]) -> Vec<usize> {
    if needle.is_empty() {
        return (0..=haystack.len()).collect();
    }
    let fail = build_failure(needle);
    let mut out = Vec::new();
    let mut k = 0usize;
    for (i, &b) in haystack.iter().enumerate() {
        while k > 0 && needle[k] != b {
            k = fail[k - 1];
        }
        if needle[k] == b {
            k += 1;
        }
        if k == needle.len() {
            out.push(i + 1 - needle.len());
            k = fail[k - 1];
        }
    }
    out
}

/// Find the first occurrence of `needle` in `haystack`. Returns the
/// byte offset or `None`.
pub fn kmp_find_first(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() {
        return Some(0);
    }
    let fail = build_failure(needle);
    let mut k = 0usize;
    for (i, &b) in haystack.iter().enumerate() {
        while k > 0 && needle[k] != b {
            k = fail[k - 1];
        }
        if needle[k] == b {
            k += 1;
        }
        if k == needle.len() {
            return Some(i + 1 - needle.len());
        }
    }
    None
}

/// Convenience wrapper for string slices. Returns `Some((start, end))`
/// byte offsets for the first match, or `None`.
pub fn kmp_str<'a>(haystack: &'a str, needle: &str) -> Option<(usize, usize)> {
    kmp_find_first(haystack.as_bytes(), needle.as_bytes()).map(|i| (i, i + needle.len()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_needle() {
        // Empty needle matches at every position 0..=len.
        assert_eq!(kmp_find(b"abc", b""), vec![0, 1, 2, 3]);
        assert_eq!(kmp_find_first(b"abc", b""), Some(0));
    }

    #[test]
    fn single_match() {
        assert_eq!(kmp_find(b"hello world", b"world"), vec![6]);
        assert_eq!(kmp_find_first(b"hello world", b"world"), Some(6));
    }

    #[test]
    fn no_match() {
        assert_eq!(kmp_find(b"hello world", b"xyz"), Vec::<usize>::new());
        assert_eq!(kmp_find_first(b"hello world", b"xyz"), None);
    }

    #[test]
    fn multiple_matches() {
        assert_eq!(kmp_find(b"abababab", b"ab"), vec![0, 2, 4, 6]);
        assert_eq!(kmp_find(b"aaaa", b"aa"), vec![0, 1, 2]);
    }

    #[test]
    fn overlap_with_self() {
        // needle = "aaa"; in "aaaa" matches at 0, 1, 2 (each overlapping).
        assert_eq!(kmp_find(b"aaaa", b"aaa"), vec![0, 1]);
        // (kmp_find only returns non-overlapping matches in some impls; here we
        //  return positions where each match *starts*, which overlap.)
        assert_eq!(kmp_find(b"aaaaa", b"aaa"), vec![0, 1, 2]);
    }

    #[test]
    fn empty_haystack_nonempty_needle() {
        assert_eq!(kmp_find(b"", b"x"), Vec::<usize>::new());
        assert_eq!(kmp_find_first(b"", b"x"), None);
    }

    #[test]
    fn needle_at_start() {
        assert_eq!(kmp_find(b"hello", b"he"), vec![0]);
    }

    #[test]
    fn needle_at_end() {
        assert_eq!(kmp_find(b"hello", b"lo"), vec![3]);
    }

    #[test]
    fn needle_longer_than_haystack() {
        assert_eq!(kmp_find(b"hi", b"hello"), Vec::<usize>::new());
    }

    #[test]
    fn kmp_str_wrapper() {
        assert_eq!(kmp_str("hello world", "world"), Some((6, 11)));
        assert_eq!(kmp_str("hello", "bye"), None);
    }

    #[test]
    fn alternating_pattern() {
        // Period-3 pattern, found multiple times.
        assert_eq!(kmp_find(b"abcabcxyzabc", b"abc"), vec![0, 3, 9]);
    }

    #[test]
    fn self_overlapping_needle() {
        // "abab" has failure-function values [0, 0, 1, 2].
        let fail = build_failure(b"abab");
        assert_eq!(fail, vec![0, 0, 1, 2]);
    }

    #[test]
    fn larger_text() {
        let text = "the quick brown fox jumps over the lazy dog the quick brown fox";
        assert_eq!(kmp_find(text.as_bytes(), b"quick"), vec![4, 48]);
    }
}
