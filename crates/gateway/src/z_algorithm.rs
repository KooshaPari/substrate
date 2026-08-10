//! Z-algorithm for linear-time pattern matching.
//!
//! The Z-array of a string `s` is an array `z[i]` such that `z[i]` is the
//! length of the longest common prefix of `s` and `s[i..]`. Computing the
//! Z-array runs in O(n) time, after which matching a pattern `p` against a
//! text `t` reduces to a single Z-pass on `p + '$' + t` — any position with
//! `z[i] >= p.len()` is an occurrence of the pattern.
//!
//! Reference: Gusfield, "Algorithms on Strings, Trees, and Sequences" (1997),
//! §1.4; original Z-algo due to Main & Lorentz (1984).
//!
//! This module ships:
//! - [`z_array`] — compute the Z-array of a byte slice.
//! - [`search`] — return the first match position of `pattern` in `text`.
//! - [`search_all`] — return every match position.
//! - [`count_overlap`] — count overlapping matches.
//!
//! The sentinel byte `0x00` is used to join pattern and text in
//! [`search`]/[`search_all`]. If your data may legitimately contain `0x00`,
//! the pattern length comparison still works correctly as long as the
//! sentinel does not appear in either input — callers must guarantee this.
//! [`z_array`] on its own is unaffected.

/// Compute the Z-array of `s`.
///
/// `z[i]` is the length of the longest prefix of `s` that matches
/// `s[i..]`. `z[0]` is conventionally set to `s.len()`.
pub fn z_array(s: &[u8]) -> Vec<usize> {
    let n = s.len();
    let mut z = vec![0usize; n];
    if n == 0 {
        return z;
    }
    z[0] = n;
    let mut l = 0usize;
    let mut r = 0usize;
    for i in 1..n {
        if i < r {
            z[i] = (r - i).min(z[i - l]);
        }
        while i + z[i] < n && s[z[i]] == s[i + z[i]] {
            z[i] += 1;
        }
        if i + z[i] > r {
            l = i;
            r = i + z[i];
        }
    }
    z
}

/// Return the first match position of `pattern` in `text`, or `None`.
///
/// Uses an internal `0x00` sentinel; do not pass inputs containing `0x00`.
pub fn search(text: &[u8], pattern: &[u8]) -> Option<usize> {
    if pattern.is_empty() {
        return Some(0);
    }
    if pattern.len() > text.len() {
        return None;
    }
    let mut combined = Vec::with_capacity(pattern.len() + 1 + text.len());
    combined.extend_from_slice(pattern);
    combined.push(0);
    combined.extend_from_slice(text);
    let z = z_array(&combined);
    let offset = pattern.len() + 1;
    let plen = pattern.len();
    for (i, &zi) in z[offset..].iter().enumerate() {
        if zi >= plen {
            return Some(i);
        }
    }
    None
}

/// Return all match positions of `pattern` in `text`, in ascending order.
///
/// Overlapping matches are reported. Uses a `0x00` sentinel internally.
pub fn search_all(text: &[u8], pattern: &[u8]) -> Vec<usize> {
    let mut out = Vec::new();
    if pattern.is_empty() || pattern.len() > text.len() {
        return out;
    }
    let mut combined = Vec::with_capacity(pattern.len() + 1 + text.len());
    combined.extend_from_slice(pattern);
    combined.push(0);
    combined.extend_from_slice(text);
    let z = z_array(&combined);
    let offset = pattern.len() + 1;
    let plen = pattern.len();
    for (i, &zi) in z[offset..].iter().enumerate() {
        if zi >= plen {
            out.push(i);
        }
    }
    out
}

/// Count overlapping occurrences of `pattern` in `text`.
pub fn count_overlap(text: &[u8], pattern: &[u8]) -> usize {
    search_all(text, pattern).len()
}

/// Compute the longest common prefix of two byte slices in O(min(len)) time.
pub fn longest_common_prefix(a: &[u8], b: &[u8]) -> usize {
    let n = a.len().min(b.len());
    let mut i = 0;
    while i < n && a[i] == b[i] {
        i += 1;
    }
    i
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn z_array_known() {
        // Classic textbook example.
        let s = b"aabxaab";
        let z = z_array(s);
        // z[0] = 7; z[1] = 1; z[2] = 0; z[3] = 0; z[4] = 3; z[5] = 1; z[6] = 0.
        assert_eq!(z, vec![7, 1, 0, 0, 3, 1, 0]);
    }

    #[test]
    fn z_array_uniform() {
        let z = z_array(b"aaaa");
        assert_eq!(z, vec![4, 3, 2, 1]);
    }

    #[test]
    fn z_array_empty() {
        let z = z_array(b"");
        assert!(z.is_empty());
    }

    #[test]
    fn z_array_single() {
        assert_eq!(z_array(b"x"), vec![1]);
    }

    #[test]
    fn search_first_match() {
        assert_eq!(search(b"hello world", b"world"), Some(6));
        assert_eq!(search(b"hello world", b"hello"), Some(0));
        assert_eq!(search(b"abcabc", b"abc"), Some(0));
    }

    #[test]
    fn search_no_match() {
        assert_eq!(search(b"abcdef", b"xyz"), None);
    }

    #[test]
    fn search_pattern_too_long() {
        assert_eq!(search(b"hi", b"hello"), None);
    }

    #[test]
    fn search_empty_pattern() {
        assert_eq!(search(b"anything", b""), Some(0));
    }

    #[test]
    fn search_all_overlapping() {
        let hits = search_all(b"aaaa", b"aa");
        assert_eq!(hits, vec![0, 1, 2]);
    }

    #[test]
    fn search_all_disjoint() {
        let hits = search_all(b"abcabcabc", b"abc");
        assert_eq!(hits, vec![0, 3, 6]);
    }

    #[test]
    fn count_overlaps_correctly() {
        assert_eq!(count_overlap(b"aaaaaa", b"aa"), 5);
        assert_eq!(count_overlap(b"abcdef", b"gh"), 0);
    }

    #[test]
    fn longest_common_prefix_basic() {
        assert_eq!(longest_common_prefix(b"abcdef", b"abcxyz"), 3);
        assert_eq!(longest_common_prefix(b"abc", b"abc"), 3);
        assert_eq!(longest_common_prefix(b"abc", b"abd"), 2);
        assert_eq!(longest_common_prefix(b"abc", b"x"), 0);
        assert_eq!(longest_common_prefix(b"abc", b""), 0);
    }

    #[test]
    fn matches_rabin_karp_positions() {
        // Both algorithms should agree on this input.
        let text = b"AABAACAADAABAABA";
        let pat = b"AABA";
        assert_eq!(search(text, pat), Some(0));
        assert_eq!(search_all(text, pat), vec![0, 9, 12]);
    }

    #[test]
    fn long_text_no_panic() {
        let text = vec![b'a'; 65_536];
        let hits = search_all(&text, b"aaa");
        assert_eq!(hits.len(), 65_536 - 2);
    }
}
