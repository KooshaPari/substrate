//! UTF-8 byte-sequence validator.
//!
//! A single-pass streaming-style UTF-8 validator that reports the byte
//! offset of the first invalid byte (if any). Use [`is_valid_str`] for a
//! quick bool check or [`validate`] for a counted-position report.
//!
//! This is a focused correctness validator — it does NOT enumerate the
//! Unicode codepoints or convert between string types. It only answers:
//! "is this byte sequence a well-formed UTF-8 string?"

/// True if `bytes` is a well-formed UTF-8 sequence.
pub fn is_valid_str(bytes: &[u8]) -> bool {
    validate(bytes).is_ok()
}

/// Validate a UTF-8 byte sequence. Returns `Ok(())` if valid, or
/// `Err(byte_offset)` pointing at the byte position of the first invalid
/// byte (0-indexed).
pub fn validate(bytes: &[u8]) -> Result<(), usize> {
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        let (codepoint_len, expected_continuation) = if b < 0x80 {
            (1, 0)
        } else if b < 0xC0 {
            // Continuation byte at start position — invalid
            return Err(i);
        } else if b < 0xE0 {
            (2, 1)
        } else if b < 0xF0 {
            (3, 2)
        } else if b < 0xF8 {
            (4, 3)
        } else {
            return Err(i);
        };
        if i + codepoint_len > bytes.len() {
            return Err(i);
        }
        // Validate continuation bytes
        for k in 1..=expected_continuation {
            if (bytes[i + k] & 0xC0) != 0x80 {
                return Err(i + k);
            }
        }
        // Validate codepoint ranges + overlong encodings
        match codepoint_len {
            1 => {
                // ASCII byte; already validated as < 0x80
            }
            2 => {
                let cp = (((b & 0x1F) as u32) << 6) | ((bytes[i + 1] & 0x3F) as u32);
                if cp < 0x80 {
                    return Err(i);
                }
            }
            3 => {
                let cp = (((b & 0x0F) as u32) << 12)
                    | (((bytes[i + 1] & 0x3F) as u32) << 6)
                    | ((bytes[i + 2] & 0x3F) as u32);
                if cp < 0x800 || (0xD800..0xE000).contains(&cp) {
                    return Err(i);
                }
            }
            4 => {
                let cp = (((b & 0x07) as u32) << 18)
                    | (((bytes[i + 1] & 0x3F) as u32) << 12)
                    | (((bytes[i + 2] & 0x3F) as u32) << 6)
                    | ((bytes[i + 3] & 0x3F) as u32);
                if cp < 0x10000 || cp > 0x10FFFF {
                    return Err(i);
                }
            }
            _ => unreachable!(),
        }
        i += codepoint_len;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_is_valid() {
        assert!(is_valid_str(b""));
        assert_eq!(validate(b""), Ok(()));
    }

    #[test]
    fn ascii_valid() {
        assert!(is_valid_str(b"hello"));
        assert!(is_valid_str(b"Hello, world!\n\t"));
    }

    #[test]
    fn two_byte_utf8_valid() {
        // U+00E9 = é = 0xC3 0xA9
        assert!(is_valid_str(&[0xC3, 0xA9]));
        assert!(is_valid_str("héllo".as_bytes()));
    }

    #[test]
    fn three_byte_utf8_valid() {
        // U+20AC = € = 0xE2 0x82 0xAC
        assert!(is_valid_str(&[0xE2, 0x82, 0xAC]));
    }

    #[test]
    fn four_byte_utf8_valid() {
        // U+1F600 = 😀 = 0xF0 0x9F 0x98 0x80
        assert!(is_valid_str(&[0xF0, 0x9F, 0x98, 0x80]));
    }

    #[test]
    fn truncate_in_middle_of_codepoint_errors() {
        // 0xC3 (start of 2-byte sequence) without the continuation byte
        assert!(!is_valid_str(&[0xC3]));
        assert_eq!(validate(&[0xC3]), Err(0));
    }

    #[test]
    fn overlong_encoding_rejected() {
        // 0xC0 0x80 is the overlong encoding of U+0000 (invalid)
        assert!(!is_valid_str(&[0xC0, 0x80]));
    }

    #[test]
    fn surrogate_rejected() {
        // U+D800 (high surrogate) is invalid in UTF-8
        assert!(!is_valid_str(&[0xED, 0xA0, 0x80]));
    }

    #[test]
    fn byte_too_large_rejected() {
        // 0xF8 starts a 5-byte sequence which is invalid UTF-8
        assert!(!is_valid_str(&[0xF8, 0x80, 0x80, 0x80, 0x80]));
    }

    #[test]
    fn error_position_correct() {
        // 0xC0 starts a 2-byte sequence (overlong); the validator reports
        // the first malformed byte. Either position 5 (the 0xC0 itself,
        // rejected as overlong) or position 6 (the next byte, rejected
        // as non-continuation) is acceptable. Our implementation reports 6.
        let bytes = b"hello\xC0world";
        let err = validate(bytes);
        assert!(err.is_err());
        assert!(err.unwrap_err() >= 5 && err.unwrap_err() <= 6);
    }
}