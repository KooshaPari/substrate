//! Base32 (RFC 4648) encoder + decoder.
//!
//! Standard alphabet (A-Z, 2-7) with `=` padding. Suitable for
//! `data:` URLs, TOTP secrets, and short binary identifiers. Does NOT
//! implement base32hex (RFC 4648 §7) or z-base-32 (Zooko).
//!
//! Encoding is strict: rejects inputs with characters outside the
//! alphabet. Decoding accepts both padded and unpadded forms.

const ALPHABET: &[u8; 32] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";

/// RFC 4648 base32 encode.
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
    // Pad to multiple of 8
    while out.len() % 8 != 0 {
        out.push('=');
    }
    out
}

/// RFC 4648 base32 decode. Strips `=` padding.
pub fn decode(input: &str) -> Result<Vec<u8>, String> {
    let stripped: String = input.chars().filter(|&c| c != '=').collect();
    if stripped.is_empty() {
        return Ok(Vec::new());
    }
    let bytes = stripped.as_bytes();
    let mut out = Vec::with_capacity(stripped.len() * 5 / 8);
    let mut buf: u64 = 0;
    let mut bits: u32 = 0;
    for &b in bytes {
        let idx = ALPHABET.iter().position(|&c| c == b).ok_or_else(|| format!("bad char 0x{b:02x}"))?;
        buf = (buf << 5) | idx as u64;
        bits += 5;
        if bits >= 8 {
            bits -= 8;
            out.push(((buf >> bits) & 0xff) as u8);
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
    fn rfc4648_test_vectors() {
        // RFC 4648 §10 test vectors
        assert_eq!(encode(b"f"), "MY======");
        assert_eq!(encode(b"fo"), "MZXQ====");
        assert_eq!(encode(b"foo"), "MZXW6===");
        assert_eq!(encode(b"foob"), "MZXW6YQ=");
        assert_eq!(encode(b"fooba"), "MZXW6YTB");
        assert_eq!(encode(b"foobar"), "MZXW6YTBOI======");
    }

    #[test]
    fn rfc4648_decode_vectors() {
        assert_eq!(decode("MY======").unwrap(), b"f");
        assert_eq!(decode("MZXQ====").unwrap(), b"fo");
        assert_eq!(decode("MZXW6===").unwrap(), b"foo");
        assert_eq!(decode("MZXW6YQ=").unwrap(), b"foob");
        assert_eq!(decode("MZXW6YTB").unwrap(), b"fooba");
        assert_eq!(decode("MZXW6YTBOI======").unwrap(), b"foobar");
    }

    #[test]
    fn decode_unpadded_works() {
        assert_eq!(decode("MZXW6YTB").unwrap(), b"fooba");
    }

    #[test]
    fn decode_bad_char_errors() {
        assert!(decode("@@@@").is_err());
    }

    #[test]
    fn lowercase_alphabet_rejected() {
        // Strict decoder; lowercase is a different convention
        assert!(decode("mzxw6ytb").is_err());
    }
}