//! UTF-8 codepoint iterator.
//!
//! Iterates over the Unicode scalar values in a UTF-8 byte sequence.
//! Returns [`None`] on malformed bytes (rather than panicking) so
//! callers can decide how to handle invalid input.
//!
//! Wraps the std [`std::str::Chars`] iterator for `&str` input, or
//! walks bytes manually for `[u8]` input (where the encoding might
//! be invalid).

/// Iterator over Unicode codepoints in a `&str`. Returns the
/// underlying char iterator; equivalent to `s.chars()` but provided
/// for API symmetry with [`Utf8ByteIter`].
pub fn chars_iter(s: &str) -> std::str::Chars<'_> {
    s.chars()
}

/// Iterator over Unicode codepoints in a `[u8]` slice that IS known
/// to be well-formed UTF-8. Uses the safe `str::from_utf8` conversion
/// + `chars()`. If `bytes` is not valid UTF-8 the returned iterator is
/// empty (rather than triggering undefined behaviour). Callers that
/// have already validated their input pay only the validation cost.
pub fn chars_iter_bytes_valid(bytes: &[u8]) -> std::str::Chars<'_> {
    // The previous implementation used `from_utf8_unchecked`, which is
    // rejected by `#![forbid(unsafe_code)]` in lib.rs. Pre-existing
    // regression introduced in commit d650fa4 (L140 v0.3.0 expansion,
    // utf8_iter + decimal_lc + calendar_date). Fixed as part of wave-36.
    // We leak a small String when input is invalid UTF-8 (rare path;
    // contract requires caller to pass valid input). On the happy path
    // the safe conversion returns a borrowed `Chars` with no allocation.
    match std::str::from_utf8(bytes) {
        Ok(s) => s.chars(),
        Err(_) => Box::leak(String::new().into_boxed_str()).chars(),
    }
}

/// Iterator over Unicode codepoints in a `[u8]` slice that might be
/// invalid. Calls a validator on each codepoint boundary; any
/// invalid byte sequence yields [`u32::MAX`] (0x10FFFF + 1, a sentinel
/// value) and advances past the failure byte.
///
/// Returns owned `char_or_err` codepoint values: `Ok(char)` for valid
/// sequences, `Err(byte_offset, raw_byte)` for the first byte of each
/// invalid sequence. Stops at end of input.
pub fn chars_iter_bytes_lossy(bytes: &[u8]) -> LossyUtf8Iter<'_> {
    LossyUtf8Iter { bytes, pos: 0 }
}

pub struct LossyUtf8Iter<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl<'a> Iterator for LossyUtf8Iter<'a> {
    type Item = Result<char, (usize, u8)>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.pos >= self.bytes.len() {
            return None;
        }
        let start = self.pos;
        let bytes = &self.bytes[start..];
        // Decode one UTF-8 codepoint manually; on failure, advance 1 byte.
        let result = decode_one(bytes);
        match result {
            Some((ch, len)) => {
                self.pos += len;
                Some(Ok(ch))
            }
            None => {
                let bad = self.bytes[start];
                self.pos += 1;
                Some(Err((start, bad)))
            }
        }
    }
}

fn decode_one(bytes: &[u8]) -> Option<(char, usize)> {
    let b0 = *bytes.first()?;
    if b0 < 0x80 {
        return Some((b0 as char, 1));
    }
    if b0 < 0xC0 {
        return None; // continuation byte at start
    }
    let n = if b0 < 0xE0 {
        2
    } else if b0 < 0xF0 {
        3
    } else if b0 < 0xF8 {
        4
    } else {
        return None;
    };
    if bytes.len() < n {
        return None;
    }
    let mut cp: u32 = (b0
        & match n {
            2 => 0x1F,
            3 => 0x0F,
            _ => 0x07,
        }) as u32;
    for i in 1..n {
        let b = bytes[i];
        if (b & 0xC0) != 0x80 {
            return None;
        }
        cp = (cp << 6) | (b & 0x3F) as u32;
    }
    // Validate ranges + overlong + surrogate
    let min_cp = match n {
        2 => 0x80,
        3 => 0x800,
        4 => 0x10000,
        _ => unreachable!(),
    };
    if cp < min_cp || (0xD800..0xE000).contains(&cp) || cp > 0x10FFFF {
        return None;
    }
    char::from_u32(cp).map(|c| (c, n))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn str_chars_iter() {
        let s = "héllo";
        let chars: Vec<char> = chars_iter(s).collect();
        assert_eq!(chars, vec!['h', 'é', 'l', 'l', 'o']);
    }

    #[test]
    fn lossy_iter_valid_string() {
        let bytes = "abc".as_bytes();
        let collected: Vec<_> = chars_iter_bytes_lossy(bytes).map(|r| r.unwrap()).collect();
        assert_eq!(collected, vec!['a', 'b', 'c']);
    }

    #[test]
    fn lossy_iter_multibyte() {
        let bytes = "héllo".as_bytes();
        let collected: Vec<_> = chars_iter_bytes_lossy(bytes).map(|r| r.unwrap()).collect();
        assert_eq!(collected, vec!['h', 'é', 'l', 'l', 'o']);
    }

    #[test]
    fn lossy_iter_invalid_byte_errors() {
        let bytes = b"a\xFFz";
        let collected: Vec<_> = chars_iter_bytes_lossy(bytes).collect();
        assert_eq!(collected.len(), 3);
        assert_eq!(collected[0], Ok('a'));
        assert!(matches!(collected[1], Err((1, 0xFF))));
        assert_eq!(collected[2], Ok('z'));
    }

    #[test]
    fn decode_one_two_byte() {
        let bytes = [0xC3, 0xA9]; // é
        let (ch, len) = decode_one(&bytes).unwrap();
        assert_eq!(ch, 'é');
        assert_eq!(len, 2);
    }

    #[test]
    fn decode_one_three_byte() {
        let bytes = [0xE2, 0x82, 0xAC]; // €
        let (ch, len) = decode_one(&bytes).unwrap();
        assert_eq!(ch, '€');
        assert_eq!(len, 3);
    }

    #[test]
    fn decode_one_invalid_returns_none() {
        let bytes = [0xFF];
        assert!(decode_one(&bytes).is_none());
    }
}
