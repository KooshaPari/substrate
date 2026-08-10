//! RFC 3986 percent-encoding for URIs.
//!
//! Implements two complementary operations:
//!
//! - [`percent_encode`] — encodes "unsafe" bytes as `%XX` triplets.
//! - [`percent_decode`] — reverses the operation, rejecting invalid
//!   encodings with `Err`.
//!
//! Reference: RFC 3986 §2 — "Characters"; RFC 3986 §2.4 — "When to Encode".
//! The default unreserved set matches the RFC: `A-Z a-z 0-9 - _ . ~`.
//!
//! Two profiles:
//!
//! - `component_encode` — used for URI components (path-segment, query):
//!   encodes everything outside unreserved.
//! - `path_encode` — encodes everything outside unreserved plus `/` (so
//!   existing path separators survive).

/// Unreserved characters per RFC 3986 §2.3: `A-Z a-z 0-9 - _ . ~`.
#[inline]
fn is_unreserved(b: u8) -> bool {
    matches!(b, b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~')
}

/// Hex-encode a single byte.
fn hex_byte(b: u8) -> [u8; 3] {
    let hi = b >> 4;
    let lo = b & 0x0F;
    [
        b'%',
        if hi < 10 { b'0' + hi } else { b'A' + hi - 10 },
        if lo < 10 { b'0' + lo } else { b'A' + lo - 10 },
    ]
}

/// Encode `data` for use as a URI component (e.g. query parameter value):
/// bytes outside the unreserved set are replaced with `%XX`.
pub fn component_encode(data: &[u8]) -> String {
    let mut out = String::with_capacity(data.len());
    for &b in data {
        if is_unreserved(b) {
            out.push(b as char);
        } else {
            let trip = hex_byte(b);
            out.push(trip[0] as char);
            out.push(trip[1] as char);
            out.push(trip[2] as char);
        }
    }
    out
}

/// Encode `data` for use as a URI path segment: same as component, but
/// also preserves `/` so existing path separators survive intact.
pub fn path_encode(data: &[u8]) -> String {
    let mut out = String::with_capacity(data.len());
    for &b in data {
        if is_unreserved(b) || b == b'/' {
            out.push(b as char);
        } else {
            let trip = hex_byte(b);
            out.push(trip[0] as char);
            out.push(trip[1] as char);
            out.push(trip[2] as char);
        }
    }
    out
}

/// Decode percent-encoded `s`. Returns `Err` on a stray `%` or a non-hex
/// digit. Both upper- and lower-case hex digits are accepted.
///
/// `+` is NOT decoded as a space — that's a `application/x-www-form-urlencoded`
/// convention (WHATWG URL), not RFC 3986.
pub fn percent_decode(s: &str) -> Result<Vec<u8>, String> {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        if b == b'%' {
            if i + 2 >= bytes.len() {
                return Err(format!("trailing % at offset {}", i));
            }
            let hi = hex_val(bytes[i + 1]).ok_or_else(|| {
                format!(
                    "invalid hex digit {:?} at offset {}",
                    bytes[i + 1] as char,
                    i + 1
                )
            })?;
            let lo = hex_val(bytes[i + 2]).ok_or_else(|| {
                format!(
                    "invalid hex digit {:?} at offset {}",
                    bytes[i + 2] as char,
                    i + 2
                )
            })?;
            out.push((hi << 4) | lo);
            i += 3;
        } else {
            out.push(b);
            i += 1;
        }
    }
    Ok(out)
}

#[inline]
fn hex_val(c: u8) -> Option<u8> {
    match c {
        b'0'..=b'9' => Some(c - b'0'),
        b'a'..=b'f' => Some(c - b'a' + 10),
        b'A'..=b'F' => Some(c - b'A' + 10),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encode_unreserved_unchanged() {
        assert_eq!(component_encode(b"abcXYZ-_.~123"), "abcXYZ-_.~123");
    }

    #[test]
    fn encode_space() {
        assert_eq!(component_encode(b"a b"), "a%20b");
    }

    #[test]
    fn encode_specials() {
        assert_eq!(component_encode(b"a&b=c"), "a%26b%3Dc");
    }

    #[test]
    fn path_encode_preserves_slash() {
        assert_eq!(path_encode(b"a/b c"), "a/b%20c");
    }

    #[test]
    fn encode_high_byte() {
        // 0xC3 0xA9 is "é" in UTF-8 -> each byte encoded as %C3 %A9
        assert_eq!(component_encode("é".as_bytes()), "%C3%A9");
    }

    #[test]
    fn encode_empty() {
        assert_eq!(component_encode(b""), "");
        assert_eq!(path_encode(b""), "");
    }

    #[test]
    fn decode_basic() {
        let decoded = percent_decode("hello%20world").unwrap();
        assert_eq!(decoded, b"hello world");
    }

    #[test]
    fn decode_mixed_case_hex() {
        // %2a and %2A should both decode to '*'.
        assert_eq!(percent_decode("%2a").unwrap(), b"*");
        assert_eq!(percent_decode("%2A").unwrap(), b"*");
    }

    #[test]
    fn decode_uppercase() {
        // Already-encoded output round-trips.
        let s = "hello%20world%21";
        assert_eq!(percent_decode(s).unwrap(), b"hello world!");
    }

    #[test]
    fn decode_no_change_when_clean() {
        assert_eq!(
            percent_decode("hello-world.txt").unwrap(),
            b"hello-world.txt"
        );
    }

    #[test]
    fn decode_rejects_trailing_percent() {
        assert!(percent_decode("abc%").is_err());
    }

    #[test]
    fn decode_rejects_bad_hex() {
        assert!(percent_decode("abc%2g").is_err());
        assert!(percent_decode("abc%X2").is_err());
    }

    #[test]
    fn round_trip() {
        let original = b"hello world! \xC3\xA9 \x00\x01";
        let encoded = component_encode(original);
        let decoded = percent_decode(&encoded).unwrap();
        assert_eq!(decoded, original);
    }

    #[test]
    fn plus_not_decoded_as_space() {
        // RFC 3986 distinguishes + from space; only `application/x-www-form-urlencoded`
        // collapses + to space.
        let decoded = percent_decode("a+b").unwrap();
        assert_eq!(decoded, b"a+b");
    }
}
