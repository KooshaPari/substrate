//! Caesar cipher encoder/decoder.
//!
//! Shifts each ASCII letter by `shift` positions (mod 26) and leaves
//! non-letters unchanged. Negative shifts wrap the same way. The
//! standard 26-letter Latin alphabet is used; ASCII A-Z and a-z are
//! shifted independently.
//!
//! [`encode`] and [`decode`] are inverse operations: `decode(s, -k)` is
//! the same as `decode(s, 26 - k)`.

/// Caesar cipher shift by `shift` positions. `shift` may be any
/// integer; only its value mod 26 affects the result.
///
/// Examples:
/// - encode("HELLO", 3) = "KHOOR"
/// - encode("HELLO", 0) = "HELLO"
/// - decode("KHOOR", 3) = "HELLO"
pub fn encode(s: &str, shift: i32) -> String {
    let shift = shift.rem_euclid(26);
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        if c.is_ascii_uppercase() {
            let base = b'A';
            let idx = (c as u8 - base + shift as u8) % 26;
            out.push((base + idx) as char);
        } else if c.is_ascii_lowercase() {
            let base = b'a';
            let idx = (c as u8 - base + shift as u8) % 26;
            out.push((base + idx) as char);
        } else {
            out.push(c);
        }
    }
    out
}

/// Caesar cipher decode (inverse of [`encode`] with the same shift).
pub fn decode(s: &str, shift: i32) -> String {
    encode(s, -shift)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encode_classic_shift_3() {
        assert_eq!(encode("HELLO", 3), "KHOOR");
    }

    #[test]
    fn encode_zero_shift_identity() {
        assert_eq!(encode("HELLO", 0), "HELLO");
    }

    #[test]
    fn encode_preserves_non_letters() {
        assert_eq!(encode("HELLO, WORLD!", 3), "KHOOR, ZRUOG!");
    }

    #[test]
    fn encode_lowercase_works() {
        assert_eq!(encode("hello", 3), "khoor");
    }

    #[test]
    fn encode_wraps_at_z() {
        assert_eq!(encode("XYZ", 3), "ABC");
    }

    #[test]
    fn negative_shift_works() {
        assert_eq!(encode("HELLO", -3), "EBIIL");
    }

    #[test]
    fn shift_greater_than_26() {
        // 29 % 26 = 3
        assert_eq!(encode("HELLO", 29), "KHOOR");
    }

    #[test]
    fn decode_is_inverse() {
        let original = "Hello, World!";
        for shift in [-7, -1, 0, 1, 7, 29] {
            let encoded = encode(original, shift);
            let decoded = decode(&encoded, shift);
            assert_eq!(decoded, original, "shift={shift}");
        }
    }

    #[test]
    fn decode_khoor_returns_hello() {
        assert_eq!(decode("KHOOR", 3), "HELLO");
    }
}