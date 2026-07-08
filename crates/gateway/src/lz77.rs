//! LZ77 sliding-window compression (Ziv & Lempel, 1977).
//!
//! LZ77 compresses by replacing repeated substrings with back-references
//! into a sliding window of recent output. Each reference is a triple
//! `(distance, length, literal)` where:
//!
//! - `distance` is the byte offset back into the output window (≥ 1).
//! - `length` is the number of bytes to copy from that position (≥ 3 for
//!   matches; 1-2 byte matches are typically not worth the reference).
//! - `literal` is the next byte after the match (or the first byte if no
//!   match was found).
//!
//! Reference: Ziv & Lempel, "A Universal Algorithm for Sequential Data
//! Compression" (IEEE Transactions on Information Theory, 1977);
//! <https://en.wikipedia.org/wiki/LZ77_and_LZ78>.
//!
//! This implementation uses a brute-force longest-match search with a
//! configurable window size (default 4096 bytes). It produces a stream of
//! `Lz77Token`s plus a `literal_run` for runs of bytes that have no
//! in-window match.
//!
//! Pure safe Rust. No `unsafe`, no external crates.

/// Maximum look-back distance (window size).
pub const WINDOW_SIZE: usize = 4096;
/// Minimum match length we encode as a reference (1-2 byte matches
/// would cost more as a token than as literals).
pub const MIN_MATCH: usize = 3;
/// Maximum match length we encode.
pub const MAX_MATCH: usize = 258;

/// One token in the compressed stream.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Lz77Token {
    /// A literal byte that could not be back-referenced.
    Literal(u8),
    /// A back-reference: copy `length` bytes from `distance` bytes back.
    BackRef { distance: usize, length: usize },
}

/// Compress `input` into a stream of LZ77 tokens.
///
/// `window` controls the maximum look-back distance. `min_match` is the
/// minimum match length worth encoding as a back-reference.
pub fn lz77_compress(input: &[u8]) -> Vec<Lz77Token> {
    lz77_compress_with(input, WINDOW_SIZE, MIN_MATCH)
}

/// Compress with explicit window and min-match settings.
pub fn lz77_compress_with(input: &[u8], window: usize, min_match: usize) -> Vec<Lz77Token> {
    let mut out: Vec<Lz77Token> = Vec::new();
    let mut i = 0usize;
    while i < input.len() {
        let start = i.saturating_sub(window);
        let (best_dist, best_len) = find_longest_match(input, start, i, min_match);
        if best_len >= min_match {
            out.push(Lz77Token::BackRef {
                distance: best_dist,
                length: best_len,
            });
            i += best_len;
        } else {
            out.push(Lz77Token::Literal(input[i]));
            i += 1;
        }
    }
    out
}

/// Find the longest match for `input[i..]` that begins within
/// `input[start..i]`. Returns `(distance, length)` where `distance`
/// is the offset back from `i` and `length` is the match length.
/// Returns `(0, 0)` if no match of at least `min_match` exists.
fn find_longest_match(
    input: &[u8],
    start: usize,
    i: usize,
    min_match: usize,
) -> (usize, usize) {
    if i == 0 || i == start {
        return (0, 0);
    }
    // Bail early if there's not enough room left in the input for a
    // minimum-length match.
    if input.len() - i < min_match {
        return (0, 0);
    }
    let max_search = start..i;
    let mut best_dist = 0usize;
    let mut best_len = 0usize;
    for cand in max_search.rev() {
        // Compute match length: input[cand..] vs input[i..], byte-by-byte.
        // The encoder has access to the full input buffer; the decoder
        // replicates byte-by-byte from the already-emitted output.
        let mut len = 0usize;
        let max_len = (input.len() - i).min(MAX_MATCH);
        while len < max_len && input[cand + len] == input[i + len] {
            len += 1;
            if len >= MAX_MATCH {
                break;
            }
        }
        if len > best_len {
            best_len = len;
            best_dist = i - cand;
            if best_len >= MAX_MATCH {
                break;
            }
        }
    }
    if best_len >= min_match {
        (best_dist, best_len)
    } else {
        (0, 0)
    }
}

/// Decompress an LZ77 token stream back into the original byte sequence.
pub fn lz77_decompress(tokens: &[Lz77Token]) -> Vec<u8> {
    let mut out: Vec<u8> = Vec::new();
    for tok in tokens {
        match tok {
            Lz77Token::Literal(b) => out.push(*b),
            Lz77Token::BackRef { distance, length } => {
                assert!(*distance > 0, "LZ77 distance must be > 0");
                assert!(*distance <= out.len(), "LZ77 distance exceeds window");
                let start = out.len() - distance;
                for k in 0..*length {
                    let src = start + (k % distance);
                    out.push(out[src]);
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_input_compresses_to_empty() {
        let tokens = lz77_compress(b"");
        assert!(tokens.is_empty());
        let decoded = lz77_decompress(&tokens);
        assert_eq!(decoded, b"");
    }

    #[test]
    fn single_byte_is_literal() {
        let tokens = lz77_compress(b"x");
        assert_eq!(tokens, vec![Lz77Token::Literal(b'x')]);
        let decoded = lz77_decompress(&tokens);
        assert_eq!(decoded, b"x");
    }

    #[test]
    fn short_repetition_emits_back_reference() {
        // "abcabcabc" — third byte should start a 6-byte match back to 0.
        let tokens = lz77_compress(b"abcabcabc");
        let decoded = lz77_decompress(&tokens);
        assert_eq!(decoded, b"abcabcabc");
        // We expect at least one BackRef.
        assert!(tokens
            .iter()
            .any(|t| matches!(t, Lz77Token::BackRef { .. })));
    }

    #[test]
    fn long_repetition_roundtrip() {
        let input = vec![b'A'; 1000];
        let tokens = lz77_compress(&input);
        let decoded = lz77_decompress(&tokens);
        assert_eq!(decoded, input);
        // Compressed token count should be << input length (since each
        // back-reference covers many bytes).
        assert!(tokens.len() < 100, "expected few tokens, got {}", tokens.len());
    }

    #[test]
    fn back_reference_handles_overlap() {
        let tokens = lz77_compress(b"aaaaa");
        let decoded = lz77_decompress(&tokens);
        assert_eq!(decoded, b"aaaaa");
        let has_match = tokens.iter().any(|t| matches!(t,
            Lz77Token::BackRef { distance, length } if *distance >= 1 && *length >= 3));
        assert!(has_match, "expected a back-ref with length >= 3");
    }

    #[test]
    fn overlapping_copy_decodes_correctly() {
        // "aabaabaa" — encoder may emit a back-reference whose distance is
        // less than its length (classic overlapping LZ77 case). Decoder must
        // handle byte-by-byte replication even when src index < dst index.
        let tokens = lz77_compress(b"aabaabaa");
        let decoded = lz77_decompress(&tokens);
        assert_eq!(decoded, b"aabaabaa");
    }

    #[test]
    fn no_match_falls_back_to_literals() {
        let input: Vec<u8> = (0u16..200).map(|i| (i & 0xFF) as u8).collect();
        let tokens = lz77_compress(&input);
        let decoded = lz77_decompress(&tokens);
        assert_eq!(decoded, input);
    }

    #[test]
    fn window_size_respected() {
        // A match at distance > window must not be referenced.
        let tokens = lz77_compress_with(b"abcdefabcdef", 4, 3);
        let decoded = lz77_decompress(&tokens);
        assert_eq!(decoded, b"abcdefabcdef");
        for t in &tokens {
            if let Lz77Token::BackRef { distance, .. } = t {
                assert!(*distance <= 4, "distance {} exceeds window", distance);
            }
        }
    }

    #[test]
    fn min_match_threshold_respected() {
        // With min_match=5, "abab" should NOT generate a back-reference.
        let tokens = lz77_compress_with(b"abababab", 4096, 5);
        let decoded = lz77_decompress(&tokens);
        assert_eq!(decoded, b"abababab");
        for t in &tokens {
            if let Lz77Token::BackRef { length, .. } = t {
                assert!(*length >= 5, "found back-ref shorter than min_match");
            }
        }
    }

    #[test]
    fn realistic_text_roundtrip() {
        let input = b"LZ77 is the basis for many popular compression formats, including DEFLATE which is used in PNG and ZIP. This implementation is a teaching example.";
        let tokens = lz77_compress(input);
        let decoded = lz77_decompress(&tokens);
        assert_eq!(decoded, input);
    }

    #[test]
    fn binary_data_roundtrip() {
        let input: Vec<u8> = (0u8..=255).cycle().take(2048).collect();
        let tokens = lz77_compress(&input);
        let decoded = lz77_decompress(&tokens);
        assert_eq!(decoded, input);
    }

    #[test]
    fn mixed_patterns_roundtrip() {
        let mut input = Vec::new();
        input.extend_from_slice(b"the quick brown fox ");
        input.extend_from_slice(b"jumps over the lazy dog ");
        input.extend_from_slice(b"the quick brown fox ");
        input.extend_from_slice(b"the lazy dog sleeps");
        let tokens = lz77_compress(&input);
        let decoded = lz77_decompress(&tokens);
        assert_eq!(decoded, input);
    }
}