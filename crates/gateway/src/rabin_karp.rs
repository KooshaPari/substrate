//! Rabin-Karp string search using rolling hash.
//!
//! Rabin-Karp finds all occurrences of a pattern in a text in O(n+m) expected
//! time by maintaining a rolling hash over a sliding window of length `m`.
//! On each window shift, the hash is updated in O(1) without re-summing every
//! byte. When hashes match, a full byte comparison is performed to rule out
//! spurious collisions.
//!
//! This module ships a [`RabinKarp`] struct parameterized by the modulus and
//! base, plus a [`search`] convenience function that uses a small default
//! (mod=1_000_000_007, base=256) and a [`search_all`] helper that returns
//! every match position.
//!
//! Reference: Karp & Rabin, "Efficient randomized pattern-matching algorithms"
//! (1987). Companion to [`crate::kmp_search`].

/// Rabin-Karp rolling-hash search state.
#[derive(Clone, Debug)]
pub struct RabinKarp {
    base: u64,
    mod_: u64,
    pattern_hash: u64,
    /// `base.pow(pattern_len - 1) mod modulus`, precomputed for the O(1) shift.
    power: u64,
}

impl RabinKarp {
    /// Construct a searcher for `pattern` using the given base and modulus.
    ///
    /// `modulus` should be a large prime for low collision rates. `base` is
    /// typically a small constant such as 256 (one byte) or 257 (a common
    /// deterministic variant).
    pub fn new(pattern: &[u8], base: u64, modulus: u64) -> Self {
        assert!(modulus > 1, "modulus must be > 1");
        let mut h: u64 = 0;
        for &b in pattern {
            h = (h.wrapping_mul(base).wrapping_add(b as u64)) % modulus;
        }
        let mut power: u64 = 1;
        for _ in 1..pattern.len() {
            power = (power.wrapping_mul(base)) % modulus;
        }
        Self {
            base,
            mod_: modulus,
            pattern_hash: h,
            power,
        }
    }

    /// Compute the initial rolling hash of the first `pattern_len` bytes of
    /// `text`. Useful when streaming: feed this hash to [`Self::shift`].
    pub fn hash_window(&self, text: &[u8]) -> u64 {
        let mut h: u64 = 0;
        for &b in text {
            h = (h.wrapping_mul(self.base).wrapping_add(b as u64)) % self.mod_;
        }
        h
    }

    /// Slide the hash one position to the right.
    ///
    /// Removes `outgoing` (the byte leaving the window) and adds `incoming`
    /// (the byte entering). Uses only modular arithmetic — safe against
    /// overflow as long as `modulus` fits in `u64`.
    pub fn shift(&self, prev: u64, outgoing: u8, incoming: u8) -> u64 {
        let mut h = prev;
        h = h.wrapping_add(self.mod_) - ((outgoing as u64).wrapping_mul(self.power) % self.mod_);
        h %= self.mod_;
        h = (h.wrapping_mul(self.base).wrapping_add(incoming as u64)) % self.mod_;
        h
    }

    /// Pattern hash value.
    pub fn pattern_hash(&self) -> u64 {
        self.pattern_hash
    }
}

/// Return the index of the first occurrence of `pattern` in `text`, or `None`.
///
/// Uses the default base/modulus (256 / 1_000_000_007). For a different
/// configuration, build a [`RabinKarp`] and roll the hash manually.
pub fn search(text: &[u8], pattern: &[u8]) -> Option<usize> {
    if pattern.is_empty() {
        return Some(0);
    }
    if pattern.len() > text.len() {
        return None;
    }
    let rk = RabinKarp::new(pattern, 256, 1_000_000_007);
    let mut hash = rk.hash_window(&text[..pattern.len()]);
    let pat_hash = rk.pattern_hash();
    for i in 0..=text.len() - pattern.len() {
        if hash == pat_hash && &text[i..i + pattern.len()] == pattern {
            return Some(i);
        }
        if i + pattern.len() < text.len() {
            hash = rk.shift(hash, text[i], text[i + pattern.len()]);
        }
    }
    None
}

/// Return all match positions of `pattern` in `text`, in ascending order.
///
/// Overlapping matches are reported: searching for "aa" in "aaaa" yields
/// `[0, 1, 2]`.
pub fn search_all(text: &[u8], pattern: &[u8]) -> Vec<usize> {
    let mut out = Vec::new();
    if pattern.is_empty() || pattern.len() > text.len() {
        return out;
    }
    let rk = RabinKarp::new(pattern, 256, 1_000_000_007);
    let mut hash = rk.hash_window(&text[..pattern.len()]);
    let pat_hash = rk.pattern_hash();
    for i in 0..=text.len() - pattern.len() {
        if hash == pat_hash && &text[i..i + pattern.len()] == pattern {
            out.push(i);
        }
        if i + pattern.len() < text.len() {
            hash = rk.shift(hash, text[i], text[i + pattern.len()]);
        }
    }
    out
}

/// Count the number of (possibly overlapping) occurrences of `pattern` in `text`.
pub fn count(text: &[u8], pattern: &[u8]) -> usize {
    search_all(text, pattern).len()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_simple_match() {
        assert_eq!(search(b"hello world", b"world"), Some(6));
        assert_eq!(search(b"hello world", b"hello"), Some(0));
    }

    #[test]
    fn no_match_returns_none() {
        assert_eq!(search(b"abcdef", b"xyz"), None);
    }

    #[test]
    fn pattern_at_end() {
        assert_eq!(search(b"abcXYZ", b"XYZ"), Some(3));
    }

    #[test]
    fn pattern_longer_than_text() {
        assert_eq!(search(b"hi", b"hello"), None);
    }

    #[test]
    fn empty_pattern_matches_at_zero() {
        assert_eq!(search(b"anything", b""), Some(0));
    }

    #[test]
    fn empty_text_nonempty_pattern() {
        assert_eq!(search(b"", b"x"), None);
    }

    #[test]
    fn exact_length_match() {
        assert_eq!(search(b"abc", b"abc"), Some(0));
    }

    #[test]
    fn search_all_finds_overlaps() {
        let hits = search_all(b"aaaa", b"aa");
        assert_eq!(hits, vec![0, 1, 2]);
    }

    #[test]
    fn search_all_finds_disjoint_matches() {
        let hits = search_all(b"abababab", b"ab");
        assert_eq!(hits, vec![0, 2, 4, 6]);
    }

    #[test]
    fn search_all_empty_pattern() {
        assert!(search_all(b"abc", b"").is_empty());
    }

    #[test]
    fn count_matches_search_all_len() {
        let text = b"the cat sat on the mat and the cat ran";
        let hits = search_all(text, b"cat");
        assert_eq!(hits.len(), count(text, b"cat"));
        assert_eq!(hits, vec![4, 31]);
    }

    #[test]
    fn shift_recovers_full_recomputation() {
        // A sliding window of length 4 over a fixed text must produce the
        // same hash sequence as recomputing from scratch each time.
        let rk = RabinKarp::new(b"abcd", 257, 1_000_000_009);
        let text = b"xyzabcdwxyzabcd";
        // Initial window covers "xyza" (i.e. text[0..4]).
        let mut h = rk.hash_window(&text[..4]);
        let mut recomputed = vec![h];
        // Advance 10 windows.
        for i in 0..10 {
            h = rk.shift(h, text[i], text[i + 4]);
            let fresh = rk.hash_window(&text[i + 1..i + 5]);
            assert_eq!(h, fresh, "drift at step {i}");
            recomputed.push(h);
        }
        assert_eq!(recomputed.len(), 11);
    }

    #[test]
    fn matches_kmp_search_simple() {
        // Same input should yield the same answer as KMP (see kmp_search).
        // We can't import that crate module here without exposing it, so we
        // just double-check a few known positions.
        let text = b"AABAACAADAABAABA";
        let pat = b"AABA";
        let rk = RabinKarp::new(pat, 256, 1_000_000_007);
        let mut h = rk.hash_window(&text[..pat.len()]);
        let mut hits = Vec::new();
        for i in 0..=text.len() - pat.len() {
            if h == rk.pattern_hash() && &text[i..i + pat.len()] == pat {
                hits.push(i);
            }
            if i + pat.len() < text.len() {
                h = rk.shift(h, text[i], text[i + pat.len()]);
            }
        }
        assert_eq!(hits, vec![0, 9, 12]);
    }

    #[test]
    fn large_text_does_not_panic() {
        // 64 KiB of 'a' looking for "abcdef".
        let text = vec![b'a'; 65_536];
        assert_eq!(search(&text, b"abcdef"), None);
        let hits = search_all(&text, b"aaa");
        assert_eq!(hits.len(), 65_536 - 2);
    }
}