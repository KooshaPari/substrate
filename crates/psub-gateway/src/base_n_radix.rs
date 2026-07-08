//! Base-N (radix) string conversion for bases 2..=36.
//!
//! Convert an unsigned integer to/from a base-N string representation
//! using the standard digit alphabet (0-9, then a-z for bases > 10).
//! Case-insensitive parsing. Negative numbers are NOT supported —
//! callers needing signed conversion should prepend the sign manually.
//!
//! Useful for file size formatting (1024-base), hex/binary dump
//! utilities, and short URL-safe id generators.

/// Convert a `u64` to a string in the given base (2..=36).
///
/// Panics if `base < 2` or `base > 36`.
pub fn to_base(mut n: u64, base: u32) -> String {
    assert!(base >= 2 && base <= 36, "base must be in [2, 36], got {}", base);
    if n == 0 {
        return "0".to_string();
    }
    let mut digits = Vec::new();
    while n > 0 {
        let d = (n % base as u64) as u32;
        digits.push(digit_to_char(d));
        n /= base as u64;
    }
    digits.into_iter().rev().collect()
}

/// Parse a string in the given base (2..=36) to a `u64`. Returns `Err`
/// on invalid characters, empty input, or overflow.
///
/// Case-insensitive: 'a'..='z' and 'A'..='Z' both accepted for digits
/// >= 10. The leading sign character `+` is accepted but ignored.
pub fn from_base(s: &str, base: u32) -> Result<u64, String> {
    assert!(base >= 2 && base <= 36, "base must be in [2, 36], got {}", base);
    let mut result: u64 = 0;
    let trimmed = s.trim().strip_prefix('+').unwrap_or(s.trim());
    if trimmed.is_empty() {
        return Err("empty input".into());
    }
    for c in trimmed.chars() {
        let d = char_to_digit(c)
            .ok_or_else(|| format!("invalid character '{}'", c))?;
        if d >= base {
            return Err(format!("digit {} >= base {}", d, base));
        }
        result = result
            .checked_mul(base as u64)
            .ok_or_else(|| "overflow".to_string())?;
        result = result
            .checked_add(d as u64)
            .ok_or_else(|| "overflow".to_string())?;
    }
    Ok(result)
}

fn digit_to_char(d: u32) -> char {
    if d < 10 {
        (b'0' + d as u8) as char
    } else {
        (b'a' + (d - 10) as u8) as char
    }
}

fn char_to_digit(c: char) -> Option<u32> {
    match c {
        '0'..='9' => Some(c as u32 - '0' as u32),
        'a'..='z' => Some(c as u32 - 'a' as u32 + 10),
        'A'..='Z' => Some(c as u32 - 'A' as u32 + 10),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn to_base_hex_basic() {
        assert_eq!(to_base(255, 16), "ff");
        assert_eq!(to_base(16, 16), "10");
        assert_eq!(to_base(0, 16), "0");
    }

    #[test]
    fn to_base_binary() {
        assert_eq!(to_base(5, 2), "101");
        assert_eq!(to_base(255, 2), "11111111");
    }

    #[test]
    fn to_base_decimal() {
        assert_eq!(to_base(42, 10), "42");
        assert_eq!(to_base(123456789, 10), "123456789");
    }

    #[test]
    fn to_base_octal() {
        assert_eq!(to_base(8, 8), "10");
        assert_eq!(to_base(64, 8), "100");
    }

    #[test]
    fn to_base_alphabet() {
        assert_eq!(to_base(35, 36), "z");
        assert_eq!(to_base(36, 36), "10");
    }

    #[test]
    fn from_base_basic() {
        assert_eq!(from_base("ff", 16).unwrap(), 255);
        assert_eq!(from_base("101", 2).unwrap(), 5);
        assert_eq!(from_base("0", 16).unwrap(), 0);
    }

    #[test]
    fn from_base_case_insensitive() {
        assert_eq!(from_base("FF", 16).unwrap(), 255);
        assert_eq!(from_base("AbCd", 16).unwrap(), 0xabcd);
    }

    #[test]
    fn from_base_accepts_plus_prefix() {
        assert_eq!(from_base("+ff", 16).unwrap(), 255);
    }

    #[test]
    fn round_trip_many_values() {
        for n in [0u64, 1, 42, 100, 12345, u64::MAX / 2, u64::MAX] {
            for base in [2, 8, 10, 16, 36] {
                let encoded = to_base(n, base);
                let decoded = from_base(&encoded, base).unwrap();
                assert_eq!(decoded, n, "round-trip failed for n={} base={}", n, base);
            }
        }
    }

    #[test]
    fn from_base_invalid_char_errors() {
        assert!(from_base("12g", 10).is_err());
    }

    #[test]
    fn from_base_digit_exceeds_base_errors() {
        // '9' is not valid in base 8
        assert!(from_base("9", 8).is_err());
    }

    #[test]
    #[should_panic]
    fn to_base_invalid_base_panics() {
        to_base(42, 1);
    }

    #[test]
    #[should_panic]
    fn from_base_invalid_base_panics() {
        from_base("42", 100).unwrap();
    }
}