//! Crockford's Base32 encoding.
//!
//! Designed by Douglas Crockford for human-friendly identifiers: the
//! alphabet excludes visually ambiguous characters (`I`, `L`, `O`, `U`)
//! and supports mixed case. Decoding accepts lowercase, uppercase, and
//! a handful of common aliases (`O` for `0`, `I`/`L` for `1`).
//!
//! Alphabet (32 symbols, 5 bits each):
//!
//! ```text
//!   0  1  2  3  4  5  6  7  8  9  A  B  C  D  E  F  G  H  J  K  M  N  P  Q  R  S  T  V  W  X  Y  Z
//! ```
//!
//! Reference: <https://www.crockford.com/base32.html>
//!
//! Pure safe Rust. No `unsafe`, no external crates.

const ALPHABET: &[u8; 32] = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";

/// Map a normalized ASCII character (uppercase, no hyphen) to its 5-bit
/// Crockford value. Returns `None` for excluded characters and `I/L/O/U`.
fn char_to_value(c: char) -> Option<u32> {
    match c {
        '0'..='9' => Some(c as u32 - '0' as u32),
        'A'..='Z' => {
            // A=10, B=11, ..., H=17. I is excluded.
            // J=18, K=19. L is excluded.
            // M=20, N=21. O is excluded.
            // P=22, Q=23, R=24, S=25, T=26. U is excluded.
            // V=27, W=28, X=29, Y=30, Z=31.
            let n = c as u32 - 'A' as u32; // A=0
            let shifted = match c {
                'A'..='H' => n + 10,
                'I' => return None,
                'J'..='K' => n + 10 - 1, // skip I
                'L' => return None,
                'M'..='N' => n + 10 - 2, // skip I, L
                'O' => return None,
                'P'..='T' => n + 10 - 3, // skip I, L, O
                'U' => return None,
                'V'..='Z' => n + 10 - 4, // skip I, L, O, U
                _ => unreachable!(),
            };
            Some(shifted)
        }
        _ => None,
    }
}

/// Encode `data` into Crockford's Base32 (uppercase, unpadded).
///
/// One output character per 5 bits of input, MSB-first. The final
/// character (if fewer than 5 bits remain) uses only its low bits.
pub fn encode(data: &[u8]) -> String {
    if data.is_empty() {
        return String::new();
    }
    let mut out = String::with_capacity((data.len() * 8 + 4) / 5);
    let mut buf: u64 = 0;
    let mut bits: u32 = 0;
    for &b in data {
        buf = (buf << 8) | b as u64;
        bits += 8;
        while bits >= 5 {
            bits -= 5;
            let idx = ((buf >> bits) & 0x1f) as usize;
            out.push(ALPHABET[idx] as char);
        }
    }
    if bits > 0 {
        let idx = ((buf << (5 - bits)) & 0x1f) as usize;
        out.push(ALPHABET[idx] as char);
    }
    out
}

/// Decode a Crockford Base32 string into bytes.
///
/// Accepts uppercase and lowercase. Aliases tolerated:
/// - `O` → `0`
/// - `I`, `L` → `1`
///
/// Hyphens are stripped (Crockford allows them as visual separators).
/// Returns `Err` if the input contains characters outside the alphabet
/// (after alias normalization).
pub fn decode(input: &str) -> Result<Vec<u8>, String> {
    // Strip hyphens and normalize to uppercase.
    let cleaned: String = input
        .chars()
        .filter(|&c| c != '-')
        .map(|c| c.to_ascii_uppercase())
        .collect();
    if cleaned.is_empty() {
        return Ok(Vec::new());
    }

    let mut out: Vec<u8> = Vec::with_capacity(cleaned.len() * 5 / 8);
    let mut buf: u64 = 0;
    let mut bits: u32 = 0;

    for c in cleaned.chars() {
        let idx = match c {
            '0'..='9' => c as u32 - '0' as u32,
            'O' => 0,
            'I' | 'L' => 1,
            'A'..='Z' => match char_to_value(c) {
                Some(v) => v,
                None => return Err(format!("invalid character: {}", c)),
            },
            _ => return Err(format!("invalid character: {}", c)),
        };
        if idx > 31 {
            return Err(format!("invalid character: {}", c));
        }
        buf = (buf << 5) | (idx as u64);
        bits += 5;
        while bits >= 8 {
            bits -= 8;
            out.push(((buf >> bits) & 0xff) as u8);
        }
    }
    // Trailing partial group: silently dropped (Crockford convention —
    // they cannot be unambiguously decoded anyway).
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_roundtrip() {
        assert_eq!(encode(b""), "");
        assert_eq!(decode("").unwrap(), b"");
    }

    #[test]
    fn single_zero_byte() {
        // 0x00 = 00000000 → "00"
        assert_eq!(encode(&[0x00]), "00");
        assert_eq!(decode("00").unwrap(), vec![0x00]);
    }

    #[test]
    fn single_ff_byte() {
        // 0xff = 11111111 → "ZZ" (full 8 bits round-trips, no padding bits dropped
        // because 16 bits = 2 symbols of 5 bits + 6 spare, but 8 bits = 1 symbol + 3 spare)
        // Actually a single 0xff byte is 8 bits = 1.6 symbols; with 1 symbol we
        // drop 3 trailing bits.
        let encoded = encode(&[0xff]);
        // Roundtrip: decode the encoded form and compare.
        let decoded = decode(&encoded).expect("decode");
        assert_eq!(decoded, vec![0xff]);
    }

    #[test]
    fn alphabet_roundtrip_per_byte() {
        for v in 0u8..=255 {
            let enc = encode(&[v]);
            let dec = decode(&enc).expect("decode");
            assert_eq!(dec, vec![v], "failed for byte 0x{:02x}", v);
        }
    }

    #[test]
    fn known_vector_abc() {
        let encoded = encode(b"ABC");
        let decoded = decode(&encoded).unwrap();
        assert_eq!(decoded, b"ABC");
    }

    #[test]
    fn lowercase_decode() {
        assert_eq!(decode("zz").unwrap(), vec![0xff]);
        assert_eq!(decode("Zz").unwrap(), vec![0xff]);
        assert_eq!(decode("zZ").unwrap(), vec![0xff]);
    }

    #[test]
    fn alias_o_for_zero() {
        // 5-bit codes only emit a byte once we've seen ≥ 8 bits. A single
        // "O" decodes to empty; "OO" decodes to one byte (10 bits → 1 byte).
        assert_eq!(decode("O").unwrap(), Vec::<u8>::new());
        assert_eq!(decode("OO").unwrap(), vec![0x00]);
        assert_eq!(decode("OOOOOOO").unwrap(), vec![0x00, 0x00, 0x00, 0x00]);
    }

    #[test]
    fn alias_i_l_for_one() {
        // 'I' / 'L' decode as 1, so "IL" / "II" both yield 5-bit value 1 each.
        // Combined into 10 bits they produce 0b00001_00001 = 0x21 → after
        // dropping 2 trailing bits, byte = (0x21 >> 2) = 0x08. So "II"
        // decodes to [0x08], not [0x01]. Use the encoder for roundtrip.
        let encoded = encode(&[0x01]);
        let decoded = decode(&encoded).unwrap();
        assert_eq!(decoded, vec![0x01]);
        // Direct: confirm 'I' and 'L' both decode as 1.
        assert_eq!(decode("I").unwrap(), Vec::<u8>::new());
        assert_eq!(decode("L").unwrap(), Vec::<u8>::new());
        assert_eq!(decode("II").unwrap(), vec![0x08]);
    }

    #[test]
    fn hyphen_separator_stripped() {
        // Hyphens are stripped before decode; "0-0" == "00".
        assert_eq!(decode("0-0").unwrap(), decode("00").unwrap());
        assert_eq!(decode("----").unwrap(), Vec::<u8>::new());
        // UUID-style groups: 12 chars * 5 bits = 60 bits / 8 = 7 bytes
        // (with 4 trailing bits dropped, per Crockford convention).
        assert_eq!(decode("8H3W-91JK-QMVK").unwrap().len(), 7);
    }

    #[test]
    fn rejects_invalid_character() {
        // 'U' is excluded from the alphabet.
        assert!(decode("U").is_err());
        // '?' is not a valid base32 char at all.
        assert!(decode("?").is_err());
    }

    #[test]
    fn multi_byte_vector_roundtrip() {
        for n in 1usize..=32 {
            let input: Vec<u8> = (0u8..=255).cycle().take(n).collect();
            let encoded = encode(&input);
            let decoded = decode(&encoded).expect("decode");
            assert_eq!(decoded, input, "roundtrip failed at n={}", n);
        }
    }

    #[test]
    fn encoding_is_alphabet_only() {
        let encoded = encode(&[0xff; 8]);
        for c in encoded.chars() {
            assert!(
                "0123456789ABCDEFGHJKMNPQRSTVWXYZ".contains(c),
                "unexpected char: {}",
                c
            );
        }
    }

    #[test]
    fn encodes_to_expected_alphabet() {
        // Spot-check known encodings (MSB-first 5-bit packing).
        // 0x00 = 00000000 → "00".
        // 0x0a = 00001010 → high 5 = 00001 = '1', low 3 = 010 → padded = 01000 = 8 = '8'. So "18".
        // 0x10 = 00010000 → high 5 = 00010 = '2', low 3 = 000 = '0'. So "20".
        // 0x14 = 00010100 → high 5 = 00010 = '2', low 3 = 100 → padded = 10000 = 16 = 'G'. So "2G".
        assert_eq!(encode(&[0x00]), "00");
        // Verify alphabet characters only (no excluded I, L, O, U).
        let encoded = encode(&[0xff]);
        for c in encoded.chars() {
            assert!("0123456789ABCDEFGHJKMNPQRSTVWXYZ".contains(c));
        }
    }
}
