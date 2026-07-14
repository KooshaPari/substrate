//! String similarity metrics: Hamming distance and Jaro similarity.
//!
//! Companion to the existing [`levenshtein`](crate::levenshtein) module:
//! - [`hamming`] — count of differing positions in equal-length strings.
//! - [`jaro`] — Jaro similarity in [0, 1] (record-link / Soundex-style).
//!
//! Both work on Unicode scalar values (`char`), not bytes, so they give
//! the same answer for equivalent Unicode-normalized strings.

/// Hamming distance — number of differing positions in equal-length strings.
/// Returns `None` if the strings have different lengths (Hamming distance
/// is undefined in that case).
pub fn hamming(a: &str, b: &str) -> Option<usize> {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    if a.len() != b.len() {
        return None;
    }
    Some(a.iter().zip(b.iter()).filter(|(x, y)| x != y).count())
}

/// Jaro similarity in [0, 1].
///
/// `jaro(s, s) == 1.0`. For completely different strings, approaches 0.
/// A common extension is Jaro-Winkler, which gives more weight to common
/// prefixes; this module implements plain Jaro.
pub fn jaro(a: &str, b: &str) -> f64 {
    if a == b {
        return 1.0;
    }
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let (n, m) = (a.len(), b.len());
    if n == 0 || m == 0 {
        return 0.0;
    }

    // Match window: floor(max(n, m) / 2) - 1.
    let window = (n.max(m) / 2).saturating_sub(1);
    let mut a_match = vec![false; n];
    let mut b_match = vec![false; m];
    let mut matches = 0usize;

    for i in 0..n {
        let lo = i.saturating_sub(window);
        let hi = (i + window + 1).min(m);
        for j in lo..hi {
            if !b_match[j] && a[i] == b[j] {
                a_match[i] = true;
                b_match[j] = true;
                matches += 1;
                break;
            }
        }
    }
    if matches == 0 {
        return 0.0;
    }

    // Count transpositions (half of mismatched matched pairs).
    let mut transpositions = 0usize;
    let mut k = 0usize;
    for i in 0..n {
        if a_match[i] {
            while !b_match[k] {
                k += 1;
            }
            if a[i] != b[k] {
                transpositions += 1;
            }
            k += 1;
        }
    }
    let m_f = matches as f64;
    let jaro_sim =
        (m_f / n as f64 + m_f / m as f64 + (m_f - transpositions as f64 / 2.0) / m_f) / 3.0;
    jaro_sim
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hamming_identical() {
        assert_eq!(hamming("karolin", "karolin"), Some(0));
    }

    #[test]
    fn hamming_classic() {
        // karolin vs kathrin — 3 differences.
        assert_eq!(hamming("karolin", "kathrin"), Some(3));
    }

    #[test]
    fn hamming_different_lengths() {
        assert_eq!(hamming("short", "longer string"), None);
    }

    #[test]
    fn hamming_empty() {
        assert_eq!(hamming("", ""), Some(0));
    }

    #[test]
    fn hamming_unicode() {
        // Same length but different codepoints.
        assert_eq!(hamming("café", "cafe"), Some(1));
    }

    #[test]
    fn jaro_identical() {
        assert_eq!(jaro("MARTHA", "MARTHA"), 1.0);
        assert_eq!(jaro("", ""), 1.0);
    }

    #[test]
    fn jaro_completely_different() {
        assert_eq!(jaro("abc", "xyz"), 0.0);
    }

    #[test]
    fn jaro_one_empty() {
        assert_eq!(jaro("", "abc"), 0.0);
        assert_eq!(jaro("abc", ""), 0.0);
    }

    #[test]
    fn jaro_known_value() {
        // Jaro("MARTHA", "MARHTA") ~ 0.944
        let s = jaro("MARTHA", "MARHTA");
        assert!(s > 0.9 && s < 1.0, "got {}", s);
    }

    #[test]
    fn jaro_single_char_match() {
        assert_eq!(jaro("a", "a"), 1.0);
        assert_eq!(jaro("a", "b"), 0.0);
    }
}
