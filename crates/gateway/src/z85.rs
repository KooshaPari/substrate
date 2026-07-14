//! Z85 base-85 encoding (ZeroMQ / RFC ???).
//!
//! Z85 is a base-85 encoding designed for use in ZeroMQ RFC files and
//! `CURVE` key representations. Unlike RFC 1924 base85, Z85 uses a
//! human-friendlier alphabet that avoids quote, backslash, and other
//! characters that need escaping in source code and JSON.
//!
//! Properties:
//! * 5 ASCII characters encode exactly 4 raw bytes (5/4 expansion).
//! * Input length must be a multiple of 4 bytes.
//! * Output length is exactly `5 * (input.len() / 4)`.
//!
//! Alphabet (85 printable ASCII chars, no quoting required):
//!
//! ```text
//! 0..9  a..z  A..Z  .-:+=^!/*?&<>()[]{}@%$#
//! ```
//!
//! Reference: <https://rfc.zeromq.org/spec/32/> (Z85 specification).

/// Z85 85-character alphabet.
pub const ALPHABET: &[u8; 85] =
    b"0123456789abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ.-:+=^!/*?&<>()[]{}@%$#";

/// Decode table: maps ASCII char -> value 0..84, or 255 for invalid.
/// 255 is one byte of `u8` and unlikely to be a valid value (since the
/// alphabet has exactly 85 entries).
const DECODE_TABLE: [u8; 256] = {
    let mut t = [255u8; 256];
    let bytes = ALPHABET;
    let mut i = 0;
    while i < 85 {
        t[bytes[i] as usize] = i as u8;
        i += 1;
    }
    t
};

/// Encode a byte slice into Z85. Returns an empty string for empty input.
///
/// # Panics
/// Panics if `data.len()` is not a multiple of 4 (Z85 is a block-aligned
/// encoding).
pub fn encode(data: &[u8]) -> String {
    if data.is_empty() {
        return String::new();
    }
    assert_eq!(
        data.len() % 4,
        0,
        "z85: input length must be a multiple of 4 (got {})",
        data.len()
    );
    let mut out = String::with_capacity(data.len() / 4 * 5);
    for chunk in data.chunks_exact(4) {
        // Big-endian 32-bit integer.
        let v = ((chunk[0] as u32) << 24)
            | ((chunk[1] as u32) << 16)
            | ((chunk[2] as u32) << 8)
            | (chunk[3] as u32);
        // Repeated divmod-by-85, MSB-first (big-endian digit order).
        let mut digits = [0u8; 5];
        let mut x = v;
        for i in (0..5).rev() {
            digits[i] = (x % 85) as u8;
            x /= 85;
        }
        for d in digits {
            out.push(ALPHABET[d as usize] as char);
        }
    }
    out
}

/// Decode a Z85 string back into bytes.
///
/// # Panics
/// Panics if `s.len()` is not a multiple of 5, or if any character is not
/// in the Z85 alphabet.
pub fn decode(s: &str) -> Vec<u8> {
    if s.is_empty() {
        return Vec::new();
    }
    assert_eq!(
        s.len() % 5,
        0,
        "z85: input length must be a multiple of 5 (got {})",
        s.len()
    );
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len() / 5 * 4);
    for chunk in bytes.chunks_exact(5) {
        let mut v: u64 = 0;
        for &c in chunk {
            let d = DECODE_TABLE[c as usize];
            assert_ne!(d, 255, "z85: invalid character {:?}", c as char);
            v = v * 85 + d as u64;
        }
        out.push((v >> 24) as u8);
        out.push((v >> 16) as u8);
        out.push((v >> 8) as u8);
        out.push(v as u8);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encode_empty() {
        assert_eq!(encode(&[]), "");
    }

    #[test]
    fn decode_empty() {
        assert_eq!(decode(""), Vec::<u8>::new());
    }

    #[test]
    fn encode_zero_block() {
        // 4 zero bytes -> "00000" (z85 of 0x00000000).
        let z = encode(&[0, 0, 0, 0]);
        assert_eq!(z, "00000");
    }

    #[test]
    fn decode_zero_block() {
        let b = decode("00000");
        assert_eq!(b, vec![0, 0, 0, 0]);
    }

    #[test]
    fn encode_zero_mq_reference_vector() {
        // Canonical Z85 reference vector (Zeromq RFC 32 §3):
        //   bytes = [0x86, 0x4F, 0xD2, 0x6F, 0xB5, 0x59, 0xF7, 0x5B]
        //   z85   = "HelloWorld"
        let z = encode(&[0x86, 0x4F, 0xD2, 0x6F, 0xB5, 0x59, 0xF7, 0x5B]);
        assert_eq!(z, "HelloWorld");
    }

    #[test]
    fn decode_zero_mq_reference_vector() {
        let b = decode("HelloWorld");
        assert_eq!(b, vec![0x86, 0x4F, 0xD2, 0x6F, 0xB5, 0x59, 0xF7, 0x5B]);
    }

    #[test]
    fn encode_decode_roundtrip() {
        let cases: &[&[u8]] = &[
            &[0x00, 0x00, 0x00, 0x00],
            &[0xFF, 0xFF, 0xFF, 0xFF],
            &[0x01, 0x02, 0x03, 0x04],
            &[0xDE, 0xAD, 0xBE, 0xEF],
            &[0x86, 0x4F, 0xD2, 0x6F],
            &[0x86, 0x4F, 0xD2, 0x6F, 0xB5, 0x59, 0xF7, 0x5B],
            &[
                0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0a, 0x0b, 0x0c, 0x0d,
                0x0e, 0x0f,
            ],
        ];
        for input in cases {
            assert!(input.len() % 4 == 0, "test input must be 4-aligned");
            let z = encode(input);
            assert_eq!(z.len(), input.len() / 4 * 5);
            let back = decode(&z);
            assert_eq!(&back[..], *input, "round-trip failed for {:?}", input);
        }
    }

    #[test]
    fn encode_uses_full_alphabet() {
        // Walk through all 85 chars: encode value n into 4-byte big-endian.
        // Round-trip every letter.
        for n in 0u32..=84 {
            let bytes = n.to_be_bytes();
            let z = encode(&bytes);
            assert_eq!(z.len(), 5);
            // First char is the most-significant digit.
            let back = decode(&z);
            assert_eq!(back, bytes.to_vec(), "round-trip failed for n={}", n);
        }
    }

    #[test]
    fn decode_table_covers_all_alphabet_chars() {
        // Every byte in ALPHABET must map to a unique value 0..84.
        let mut seen = [false; 85];
        for &c in ALPHABET.iter() {
            let v = DECODE_TABLE[c as usize];
            assert_ne!(v, 255, "ALPHABET char {:?} not in DECODE_TABLE", c as char);
            assert!(
                !seen[v as usize],
                "duplicate value {} for char {:?}",
                v, c as char
            );
            seen[v as usize] = true;
        }
        assert!(seen.iter().all(|&s| s));
    }

    #[test]
    #[should_panic]
    fn encode_panics_on_non_block_input() {
        let _ = encode(&[1, 2, 3]);
    }

    #[test]
    #[should_panic]
    fn decode_panics_on_non_block_input() {
        let _ = decode("abcd");
    }

    #[test]
    #[should_panic]
    fn decode_panics_on_invalid_char() {
        // Single quote is not in Z85 alphabet (Z85 deliberately avoids it).
        let _ = decode("'''''");
    }

    #[test]
    fn zero_mq_known_curve_public_key_length() {
        // A typical Z85-encoded CURVE public key (32 bytes) is exactly
        // 32 / 4 * 5 = 40 characters long.
        let key = [0x42u8; 32];
        let z = encode(&key);
        assert_eq!(z.len(), 40);
        // Round-trip.
        let back = decode(&z);
        assert_eq!(back, key.to_vec());
    }
}
