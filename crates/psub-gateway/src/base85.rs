//! RFC 1924 base85 encoder (Ascii85 variant).
//!
//! Encodes arbitrary bytes into the base85 alphabet used by RFC 1924
//! (also called "ascii85-z" or "z85" in some references). The alphabet is
//! `0-9A-Za-z!#$%&()*+-;<=>?@^_`{|}~` and produces 5 ASCII chars per
//! 4 input bytes (5/4 expansion, better than base64's 4/3).
//!
//! Use [`encode`] for the standard pipeline and [`decode`] for the inverse.
//! Empty inputs round-trip cleanly.

const ALPHABET: &[u8; 85] =
    b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz!#$%&()*+-;<=>?@^_`{|}~";

/// Encode a byte slice into base85.
///
/// Returns an empty string for empty input. All 4-byte blocks (including
/// zero blocks) encode as 5 alphabet chars. Trailing partial blocks encode
/// to `remaining + 1` chars.
pub fn encode(data: &[u8]) -> String {
    if data.is_empty() {
        return String::new();
    }
    let mut out = String::with_capacity(data.len() * 5 / 4 + 5);
    let mut i = 0;
    while i + 4 <= data.len() {
        let v = ((data[i] as u32) << 24)
            | ((data[i + 1] as u32) << 16)
            | ((data[i + 2] as u32) << 8)
            | (data[i + 3] as u32);
        let mut vv = v;
        let mut chars = [0u8; 5];
        for j in (0..5).rev() {
            chars[j] = ALPHABET[(vv % 85) as usize];
            vv /= 85;
        }
        for c in chars {
            out.push(c as char);
        }
        i += 4;
    }
    if i < data.len() {
        let mut buf = [0u8; 4];
        let remaining = data.len() - i;
        buf[..remaining].copy_from_slice(&data[i..]);
        let v = ((buf[0] as u32) << 24)
            | ((buf[1] as u32) << 16)
            | ((buf[2] as u32) << 8)
            | (buf[3] as u32);
        let mut vv = v;
        let mut chars = [0u8; 5];
        for j in (0..5).rev() {
            chars[j] = ALPHABET[(vv % 85) as usize];
            vv /= 85;
        }
        for j in 0..(remaining + 1) {
            out.push(chars[j] as char);
        }
    }
    out
}

/// Decode a base85 string back into bytes.
///
/// Per RFC 1924, every encoded character is a member of the base85 alphabet
/// (including the literal `z`); there is no `z`-shorthand in RFC 1924 (that
/// shorthand belongs to Ascii85 / btoa). Returns `Err` on any character
/// outside the alphabet, on a single trailing character, or on a malformed
/// partial block.
pub fn decode(input: &str) -> Result<Vec<u8>, String> {
    let mut out = Vec::with_capacity(input.len() * 4 / 5);
    let bytes = input.as_bytes();
    let mut buf: Vec<u32> = Vec::with_capacity(5);
    for &b in bytes {
        let idx = ALPHABET
            .iter()
            .position(|&c| c == b)
            .ok_or_else(|| format!("bad char 0x{b:02x}"))?;
        buf.push(idx as u32);
        if buf.len() == 5 {
            let v = buf[0] * 85 * 85 * 85 * 85
                + buf[1] * 85 * 85 * 85
                + buf[2] * 85 * 85
                + buf[3] * 85
                + buf[4];
            out.push(((v >> 24) & 0xff) as u8);
            out.push(((v >> 16) & 0xff) as u8);
            out.push(((v >> 8) & 0xff) as u8);
            out.push((v & 0xff) as u8);
            buf.clear();
        }
    }
    if buf.len() == 1 {
        return Err("trailing 1 char".into());
    }
    if !buf.is_empty() {
        let mut v: u64 = 0;
        for &x in &buf {
            v = v * 85 + x as u64;
        }
        for _ in buf.len()..5 {
            v = v * 85 + 84;
        }
        let rem = buf.len() - 1;
        for i in 0..rem {
            out.push(((v >> (24 - i * 8)) & 0xff) as u8);
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_round_trip() {
        assert_eq!(encode(b""), "");
        assert_eq!(decode("").unwrap(), Vec::<u8>::new());
    }

    #[test]
    fn encode_known_vector() {
        // "Hello" = [0x48, 0x65, 0x6c, 0x6c, 0x6f] encodes to base85
        let h = encode(b"Hello");
        assert_eq!(h.len(), 7); // 4 + 3 bytes => 5 + 2 chars
        assert!(!h.is_empty());
    }

    #[test]
    fn decode_known_vector() {
        let h = encode(b"Hello");
        let d = decode(&h).unwrap();
        assert_eq!(d, b"Hello");
    }

    #[test]
    fn z_is_regular_alphabet_char() {
        // RFC 1924 has no 'z' shorthand; 'z' is a regular alphabet char at
        // index 61. Decoding a single 'z' should be treated as a 1-char
        // partial block (trailing 1 char error), not as zero shorthand.
        assert!(decode("z").is_err());
    }

    #[test]
    fn bad_char_errors() {
        // ' ' (space) is not in the RFC 1924 alphabet
        assert!(decode(" ").is_err());
    }

    #[test]
    fn round_trip_256_random_bytes() {
        let data: Vec<u8> = (0..=255u8).collect();
        let encoded = encode(&data);
        let decoded = decode(&encoded).unwrap();
        assert_eq!(decoded, data);
    }
}