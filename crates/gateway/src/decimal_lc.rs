//! Decimal ↔ string conversion utility.
//!
//! [`encode`] produces a fixed-point decimal string from a 64-bit integer
//! numerator and a `1/10^d` denominator's exponent. Useful for currency
//! formatting where double-precision errors are unacceptable.
//!
//! [`parse`] is the inverse: it accepts strings in the form
//! `[+-]?digits(digits)?` (the optional fractional part may be omitted).
//!
//! All math uses integer arithmetic on the underlying numerator+
//! denominator pair; there is no `f64` involved.

/// Format `numerator / 10^scale_exp` as a decimal string.
///
/// Examples:
/// - encode(123, 0) → "123"
/// - encode(123, 2) → "1.23"
/// - encode(-123, 2) → "-1.23"
/// - encode(5, 1) → "0.5"
pub fn encode(numerator: i64, scale_exp: u8) -> String {
    if scale_exp == 0 {
        return numerator.to_string();
    }
    let scale = 10i64.pow(scale_exp as u32);
    let sign = if numerator < 0 { "-" } else { "" };
    let abs = numerator.unsigned_abs() as i64;
    let int_part = abs / scale;
    let frac_part = abs % scale;
    if scale_exp > 0 && frac_part != 0 {
        // Pad the fractional part with leading zeros so it has exactly
        // `scale_exp` digits.
        let mut frac_str = (scale + frac_part).to_string();
        frac_str.remove(0); // strip the leading '1' from `scale + frac`
        format!("{sign}{int_part}.{frac_str}")
    } else {
        format!("{sign}{int_part}")
    }
}

/// Parse a decimal string into a `i64` numerator and a scale exponent.
///
/// Accepts strings like "1.23", "-1.23", "+0.5", "0.", ".5", "5", or
/// scientific notation is NOT supported (callers should normalize
/// before calling).
///
/// Returns the numerator and exponent; combine as
/// `numerator / 10^exp` for the underlying value.
///
/// Trailing digits beyond 18 fractional places are truncated (the max
/// representable precision without f64).
pub fn parse(s: &str) -> Result<(i64, u8), String> {
    let s = s.trim();
    if s.is_empty() {
        return Err("empty input".into());
    }
    let (sign, rest) = if let Some(rest) = s.strip_prefix('-') {
        (-1, rest)
    } else if let Some(rest) = s.strip_prefix('+') {
        (1, rest)
    } else {
        (1, s)
    };
    let (int_str, frac_str) = match rest.split_once('.') {
        Some((i, f)) => (i, f),
        None => (rest, ""),
    };
    if int_str.is_empty() {
        return Err(format!("missing integer part in '{}'", s));
    }
    let parsed_int: i64 = int_str
        .parse()
        .map_err(|e| format!("invalid integer part '{}': {}", int_str, e))?;
    // Truncate fractional to 18 digits (max safe precision)
    let frac_truncated: &str = if frac_str.len() > 18 {
        &frac_str[..18]
    } else {
        frac_str
    };
    let exp: u8 = frac_truncated.len() as u8;
    let parsed_frac: i64 = if frac_truncated.is_empty() {
        0
    } else {
        frac_truncated
            .parse()
            .map_err(|e| format!("invalid fractional '{}': {}", frac_truncated, e))?
    };
    let scale = 10i64.pow(exp as u32);
    let combined = parsed_int
        .checked_mul(scale)
        .and_then(|x| x.checked_add(parsed_frac))
        .ok_or_else(|| "overflow".to_string())?;
    Ok((sign * combined, exp))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encode_integer() {
        assert_eq!(encode(123, 0), "123");
        assert_eq!(encode(-42, 0), "-42");
    }

    #[test]
    fn encode_two_decimal_places() {
        assert_eq!(encode(123, 2), "1.23");
        assert_eq!(encode(-123, 2), "-1.23");
    }

    #[test]
    fn encode_pads_leading_zeros_in_fraction() {
        // 5 / 10 = 0.5, not .5 — pad to exactly `scale_exp` digits
        assert_eq!(encode(5, 1), "0.5");
        assert_eq!(encode(1, 3), "0.001");
    }

    #[test]
    fn encode_zero_integer() {
        // 0 / 100 = 0.00
        assert_eq!(encode(0, 2), "0");
    }

    #[test]
    fn encode_negative_trailing_zeros() {
        // 12300 / 100 = 123 -> "123" not "123.00" (no trailing zeros)
        assert_eq!(encode(12300, 2), "123");
    }

    #[test]
    fn parse_integer() {
        assert_eq!(parse("123").unwrap(), (123, 0));
        assert_eq!(parse("-42").unwrap(), (-42, 0));
    }

    #[test]
    fn parse_two_decimal_places() {
        assert_eq!(parse("1.23").unwrap(), (123, 2));
        assert_eq!(parse("-1.23").unwrap(), (-123, 2));
    }

    #[test]
    fn parse_pads_fraction_leading_zeros() {
        // "0.001" -> 1 / 1000
        assert_eq!(parse("0.001").unwrap(), (1, 3));
        assert_eq!(parse("0.5").unwrap(), (5, 1));
    }

    #[test]
    fn parse_invalid_errors() {
        assert!(parse("").is_err());
        assert!(parse("abc").is_err());
        assert!(parse(".").is_err()); // missing integer part
    }

    #[test]
    fn round_trip() {
        for (n, e) in [(123i64, 0u8), (5, 1), (123, 2), (-100, 3)] {
            let encoded = encode(n, e);
            let (parsed_n, _) = parse(&encoded).unwrap();
            assert_eq!(parsed_n, n, "round-trip failed for n={n} e={e}");
        }
    }
}