//! Rail-fence (zig-zag) cipher.
//!
//! The rail-fence cipher is a classical transposition cipher in which
//! the plaintext is written in a zig-zag across a fixed number of
//! "rails" (rows) and then read off rail-by-rail to form the
//! ciphertext.
//!
//! Given `rails = 3` and plaintext `"WEAREDISCOVEREDFLEEATONCE"`,
//! the zig-zag layout is
//!
//! ```text
//! W . . . E . . . C . . . R . . . L . . . T . . . E
//! . E . R . D . S . O . E . E . F . E . A . O . C .
//! . . A . . . I . . . V . . . D . . . E . . . N . .
//! ```
//!
//! Reading rail-by-rail yields the ciphertext
//! `"WECRLTEERDSOEEFEAOCAIVDEN"`. The original problem (and that
//! ciphertext) is the canonical worked example in the Wikipedia
//! "Rail fence cipher" article.
//!
//! Decryption reverses the rail-by-rail read by computing the
//! zig-zag index of every output position and distributing the
//! ciphertext characters back into those positions.
//!
//! Implementation notes:
//! - Only ASCII letters are accepted by the two `*_ascii` helpers;
//!   the more general `encrypt_str` / `decrypt_str` operate on any
//!   `char` and preserve non-letters.
//! - The cipher is **case-preserving**: `"AttackAtDawn"` and
//!   `"attackatdawn"` produce distinct ciphertexts (case is a
//!   legitimate distinguishing feature of the input character).
//! - No padding or key schedule is used; `rails` is the entire key.

/// Encrypt `plaintext` using a rail-fence cipher with the given
/// number of rails.
///
/// Returns the ciphertext as a `String` of the same length as the
/// input. Non-letter characters are emitted at the zig-zag positions
/// they would occupy if they were letters, so `encrypt_str(s, n)`
/// followed by `decrypt_str(..., n)` is a lossless round-trip for any
/// `String`.
pub fn encrypt_str(plaintext: &str, rails: usize) -> String {
    if rails <= 1 || rails >= plaintext.chars().count().max(2) {
        // With rails == 1, no transposition happens. With rails >=
        // length, every character is on its own rail.
        return plaintext.to_string();
    }
    let chars: Vec<char> = plaintext.chars().collect();
    let order = zig_zag_order(chars.len(), rails);
    let mut buckets: Vec<Vec<char>> = vec![Vec::new(); rails];
    for (idx, &ch) in order.iter().zip(chars.iter()) {
        buckets[*idx].push(ch);
    }
    let mut out = String::with_capacity(chars.len());
    for bucket in buckets.iter() {
        out.extend(bucket.iter());
    }
    out
}

/// Decrypt a rail-fence ciphertext produced by [`encrypt_str`].
///
/// Returns the original plaintext, or the input unchanged if `rails`
/// is degenerate (see [`encrypt_str`]).
pub fn decrypt_str(ciphertext: &str, rails: usize) -> String {
    if rails <= 1 || rails >= ciphertext.chars().count().max(2) {
        return ciphertext.to_string();
    }
    let chars: Vec<char> = ciphertext.chars().collect();
    let n = chars.len();
    let order = zig_zag_order(n, rails);

    // Distribute characters rail-by-rail back to their zig-zag slots.
    let mut buckets: Vec<Vec<char>> = vec![Vec::new(); rails];
    let mut idx = 0;
    for r in 0..rails {
        let need = order.iter().filter(|&&x| x == r).count();
        buckets[r] = chars[idx..idx + need].to_vec();
        idx += need;
    }

    let mut out = String::with_capacity(n);
    for slot in order.iter() {
        out.push(buckets[*slot].remove(0));
    }
    out
}

/// Encrypt only the ASCII letters of `plaintext`, uppercasing each
/// letter before encrypting. Letters outside `A..=Z` are dropped.
///
/// This matches the classic worked-example form of the cipher (used
/// in the Wikipedia article, the CryptoMuseum entry, and most
/// textbooks).
pub fn encrypt_ascii(plaintext: &str, rails: usize) -> String {
    let cleaned: String = plaintext
        .chars()
        .filter(|c| c.is_ascii_alphabetic())
        .map(|c| c.to_ascii_uppercase())
        .collect();
    encrypt_str(&cleaned, rails)
}

/// Decrypt a ciphertext produced by [`encrypt_ascii`].
///
/// Returns the **uppercase** plaintext, since `encrypt_ascii` already
/// uppercased the input. Callers that need the original casing must
/// store it out-of-band.
pub fn decrypt_ascii(ciphertext: &str, rails: usize) -> String {
    decrypt_str(ciphertext, rails)
}

/// Compute, for every position `i` in `[0, n)`, the rail index
/// occupied by that position in the zig-zag pattern.
///
/// The zig-zag bounces between rail 0 and rail `rails - 1`; for
/// `rails = 4` the per-position rail is:
///
/// ```text
/// 0, 1, 2, 3, 2, 1, 0, 1, 2, 3, 2, 1, 0, ...
/// ```
fn zig_zag_order(n: usize, rails: usize) -> Vec<usize> {
    let mut out = Vec::with_capacity(n);
    let mut rail: usize = 0;
    let mut dir: i64 = 1;
    for _ in 0..n {
        out.push(rail);
        if (rail == 0 && dir < 0) || (rails > 0 && rail + 1 == rails && dir > 0) {
            dir = -dir;
        }
        if dir > 0 {
            rail += 1;
        } else if rail > 0 {
            rail -= 1;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical_wikipedia_example() {
        // The reference example from Wikipedia "Rail fence cipher".
        let plain = "WEAREDISCOVEREDFLEEATONCE";
        let cipher = "WECRLTEERDSOEEFEAOCAIVDEN";
        assert_eq!(encrypt_ascii(plain, 3), cipher);
        assert_eq!(decrypt_ascii(cipher, 3), plain);
    }

    #[test]
    fn rails_one_is_identity() {
        // rails = 1 produces no transposition at all.
        let s = "HELLOWORLD";
        assert_eq!(encrypt_str(s, 1), s);
        assert_eq!(decrypt_str(s, 1), s);
    }

    #[test]
    fn rails_two_is_alternating() {
        // With rails = 2, characters in even positions land on rail 0
        // and characters in odd positions land on rail 1. Reading
        // rail-by-rail interleaves the two halves.
        let s = "ABCDEFGH";
        let c = encrypt_str(s, 2);
        // Even positions 0,2,4,6 = "ACEG"; odd positions 1,3,5,7 = "BDFH"
        assert_eq!(c, "ACEG".to_string() + "BDFH");
        assert_eq!(decrypt_str(&c, 2), s);
    }

    #[test]
    fn round_trip_short_string() {
        let s = "HELLO";
        let c = encrypt_str(s, 3);
        assert_eq!(decrypt_str(&c, 3), s);
    }

    #[test]
    fn round_trip_long_string() {
        let s = "WEAREDISCOVEREDFLEEATONCEANDWEMUSTFIGHT";
        let c = encrypt_str(s, 4);
        assert_eq!(decrypt_str(&c, 4), s);
    }

    #[test]
    fn zig_zag_indices_rails_3() {
        // Manually computed zig-zag pattern for rails=3:
        // 0 1 2 1 0 1 2 1 0 1 2 1 0 ...
        let order = zig_zag_order(13, 3);
        assert_eq!(order, vec![0, 1, 2, 1, 0, 1, 2, 1, 0, 1, 2, 1, 0]);
    }

    #[test]
    fn zig_zag_indices_rails_4() {
        // rails=4: 0,1,2,3,2,1,0,1,2,3,2,1,0
        let order = zig_zag_order(13, 4);
        assert_eq!(order, vec![0, 1, 2, 3, 2, 1, 0, 1, 2, 3, 2, 1, 0]);
    }

    #[test]
    fn rails_greater_than_length_is_identity() {
        // With rails >= length, every character is on its own rail and
        // no transposition occurs. Our check uses
        // `chars.count().max(2)` so single-character inputs also stay
        // identity-like.
        let s = "ABCDE";
        let c = encrypt_str(s, 5);
        assert_eq!(c, s);
    }

    #[test]
    fn non_letters_preserved_round_trip() {
        // The general `*_str` helpers must round-trip even with spaces
        // and punctuation in the input.
        let s = "Hello, World! 2026.";
        let c = encrypt_str(s, 3);
        assert_eq!(decrypt_str(&c, 3), s);
    }

    #[test]
    fn ascii_drops_non_letters() {
        // `encrypt_ascii` discards digits and punctuation.
        let plain = "Hello, World!";
        let cipher = encrypt_ascii(plain, 3);
        // Only HELLOWORLD survives; spaces, comma, exclamation are
        // dropped.
        let expected_input = "HELLOWORLD";
        assert_eq!(cipher, encrypt_str(expected_input, 3));
    }

    #[test]
    fn decrypt_ascii_returns_uppercase() {
        let cipher = encrypt_ascii("Attack At Dawn", 3);
        let back = decrypt_ascii(&cipher, 3);
        assert_eq!(back, back.to_ascii_uppercase());
    }

    #[test]
    fn rails_two_round_trip_longer() {
        let s = "THEQUICKBROWNFOXJUMPSOVERTHELAZYDOG";
        let c = encrypt_str(s, 2);
        assert_eq!(decrypt_str(&c, 2), s);
    }

    #[test]
    fn rails_six_round_trip() {
        let s = "SUBSTRATEL167";
        let c = encrypt_str(s, 6);
        assert_eq!(decrypt_str(&c, 6), s);
    }

    #[test]
    fn single_character_input() {
        // Single-character input must not panic regardless of rails.
        let c = encrypt_str("X", 7);
        assert_eq!(c, "X");
        assert_eq!(decrypt_str(&c, 7), "X");
    }
}
