//! Boyer-Moore (bad-character rule) single-pattern string search.
//!
//! Boyer-Moore scans the text left-to-right but compares the pattern
//! right-to-left, allowing the search index to skip multiple characters on a
//! mismatch. The bad-character rule shifts the window forward so that the
//! mismatched text character aligns with its rightmost occurrence in the
//! pattern (or past the end of the pattern if it never appears).
//!
//! Average-case sublinear time. This implementation uses only the
//! bad-character rule (the simpler of the two BM heuristics); the
//! good-suffix rule would require additional state and is omitted for
//! compactness.
//!
//! Reference: Boyer & Moore, "A Fast String Searching Algorithm" (1977);
//! <https://en.wikipedia.org/wiki/Boyer%E2%80%93Moore_string-search_algorithm>.
//!
//! Pure safe Rust. Works on bytes (no Unicode awareness); use on ASCII or
//! on byte slices that are known to be aligned to a code-point boundary.

/// Build the bad-character skip table for `pattern`.
///
/// `skip[c]` is the distance from the rightmost occurrence of byte `c` in
/// `pattern` to the right end of the pattern. If `c` does not occur in
/// `pattern`, `skip[c] = pattern.len()`.
pub fn bad_char_table(pattern: &[u8]) -> [usize; 256] {
    let mut skip = [pattern.len(); 256];
    if pattern.is_empty() {
        return skip;
    }
    for (i, &b) in pattern.iter().enumerate() {
        // Last occurrence wins (so rightmost).
        skip[b as usize] = pattern.len() - 1 - i;
    }
    skip
}

/// Find all (possibly overlapping) occurrences of `pattern` in `text`.
///
/// Returns the starting byte index of each match, in ascending order. If
/// `pattern` is empty, returns every byte index from `0..=text.len()`
/// (convention: empty pattern matches at every position).
pub fn boyer_moore_find(text: &[u8], pattern: &[u8]) -> Vec<usize> {
    let mut hits = Vec::new();
    if pattern.is_empty() {
        for i in 0..=text.len() {
            hits.push(i);
        }
        return hits;
    }
    if pattern.len() > text.len() {
        return hits;
    }
    let skip = bad_char_table(pattern);
    let mut s = 0usize;
    while s + pattern.len() <= text.len() {
        // Compare pattern against text[s..s+pattern.len()], right-to-left.
        let mut matched = true;
        for j in (0..pattern.len()).rev() {
            if pattern[j] != text[s + j] {
                // Mismatch at position j. Shift s by skip[text[s+j]], but at
                // least 1 to guarantee progress.
                let shift = skip[text[s + j] as usize];
                s += if shift == 0 { 1 } else { shift };
                matched = false;
                break;
            }
        }
        if matched {
            hits.push(s);
            // For overlapping matches, shift by 1.
            s += 1;
        }
    }
    hits
}

/// Find the first occurrence of `pattern` in `text`, or `None`.
pub fn boyer_moore_first(text: &[u8], pattern: &[u8]) -> Option<usize> {
    if pattern.is_empty() {
        return Some(0);
    }
    if pattern.len() > text.len() {
        return None;
    }
    let skip = bad_char_table(pattern);
    let mut s = 0usize;
    while s + pattern.len() <= text.len() {
        let mut matched = true;
        for j in (0..pattern.len()).rev() {
            if pattern[j] != text[s + j] {
                let shift = skip[text[s + j] as usize];
                s += if shift == 0 { 1 } else { shift };
                matched = false;
                break;
            }
        }
        if matched {
            return Some(s);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classic_example() {
        // Self-verifying: build a text with a known match and verify hits.
        let text = b"AAAAAAAAAAAAAAATCATCGCATCGCATZZZZZZZZZZ";
        let pat = b"TCATCGCATCGCAT";
        let hits = boyer_moore_find(text, pat);
        // Verify by re-checking each hit matches the pattern at the given offset.
        for &h in &hits {
            assert!(h + pat.len() <= text.len(), "hit out of range: {h}");
            assert_eq!(&text[h..h + pat.len()], pat, "hit {h} is not a true match");
        }
        // Expect at least one match.
        assert!(!hits.is_empty(), "expected at least one match in {text:?}");
        // Brute-force cross-check.
        let mut expected = Vec::new();
        for i in 0..=text.len().saturating_sub(pat.len()) {
            if &text[i..i + pat.len()] == pat {
                expected.push(i);
            }
        }
        assert_eq!(hits, expected);
    }

    #[test]
    fn no_match() {
        let text = b"hello world";
        let hits = boyer_moore_find(text, b"xyz");
        assert!(hits.is_empty());
        assert_eq!(boyer_moore_first(text, b"xyz"), None);
    }

    #[test]
    fn empty_pattern_matches_at_zero() {
        let hits = boyer_moore_find(b"abc", b"");
        assert_eq!(hits, vec![0, 1, 2, 3]);
        assert_eq!(boyer_moore_first(b"abc", b""), Some(0));
    }

    #[test]
    fn pattern_longer_than_text() {
        let text = b"abc";
        let hits = boyer_moore_find(text, b"abcd");
        assert!(hits.is_empty());
        assert_eq!(boyer_moore_first(text, b"abcd"), None);
    }

    #[test]
    fn overlapping_matches() {
        // "aaa" in "aaaa" yields positions 0, 1.
        let hits = boyer_moore_find(b"aaaa", b"aaa");
        assert!(hits.contains(&0));
        assert!(hits.contains(&1));
    }

    #[test]
    fn single_byte_pattern() {
        let text = b"banana";
        let hits = boyer_moore_find(text, b"a");
        assert_eq!(hits, vec![1, 3, 5]);
        assert_eq!(boyer_moore_first(text, b"a"), Some(1));
    }

    #[test]
    fn pattern_at_start_and_end() {
        let text = b"abcXYZabc";
        let hits = boyer_moore_find(text, b"abc");
        assert_eq!(hits, vec![0, 6]);
    }

    #[test]
    fn bad_char_table_basic() {
        // pattern = "abcd", skip[c] for c in {a,b,c,d} = {3,2,1,0}
        // skip for other bytes = 4 (pattern.len()).
        let skip = bad_char_table(b"abcd");
        assert_eq!(skip[b'a' as usize], 3);
        assert_eq!(skip[b'b' as usize], 2);
        assert_eq!(skip[b'c' as usize], 1);
        assert_eq!(skip[b'd' as usize], 0);
        assert_eq!(skip[b'z' as usize], 4);
    }

    #[test]
    fn repeat_chars_in_pattern() {
        // For "abab", the last occurrence of 'a' is at index 2 → skip['a'] = 1.
        let skip = bad_char_table(b"abab");
        assert_eq!(skip[b'a' as usize], 1);
        assert_eq!(skip[b'b' as usize], 0);
    }

    #[test]
    fn single_char_text() {
        assert_eq!(boyer_moore_first(b"x", b"x"), Some(0));
        assert_eq!(boyer_moore_first(b"x", b"y"), None);
    }

    #[test]
    fn long_text_many_occurrences() {
        let text: Vec<u8> = (0..100).flat_map(|_| b"ab".iter().copied()).collect();
        let hits = boyer_moore_find(&text, b"ab");
        assert_eq!(hits.len(), 100);
        for (i, h) in hits.iter().enumerate() {
            assert_eq!(*h, 2 * i);
        }
    }

    #[test]
    fn first_returns_leftmost() {
        let text = b"the quick brown fox jumps over the lazy dog";
        assert_eq!(boyer_moore_first(text, b"the"), Some(0));
    }

    #[test]
    fn pattern_only_appears_in_middle() {
        let text = b"xxxxABCxxxx";
        assert_eq!(boyer_moore_first(text, b"ABC"), Some(4));
    }
}