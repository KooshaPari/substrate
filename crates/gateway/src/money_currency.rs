//! Money / currency formatting helpers.
//!
//! [`Money`] holds a fixed-precision integer representation (minor
//! units, e.g. cents for USD) and a [`Currency`] code (ISO 4217).
//! Avoids floating-point by storing everything in i64 minor units.
//!
//! Operations:
//! - Arithmetic: `add`, `sub` (with rollover detection)
//! - Display: `format_with` produces `USD 12.34` style strings
//! - Parsing: [`parse`] accepts the same format
//!
//! Limitations:
//! - All amounts are i64 (max ~9.2 × 10^18 minor units)
//! - No currency conversion
//! - No locale-aware decimal/thousands separator

/// ISO 4217 currency code. Stored as a fixed-size string slice.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Currency(pub [u8; 3]);

impl Currency {
    pub const USD: Currency = Currency(*b"USD");
    pub const EUR: Currency = Currency(*b"EUR");
    pub const GBP: Currency = Currency(*b"GBP");
    pub const JPY: Currency = Currency(*b"JPY");

    /// Construct from any 3-byte input. Panics if `s.len() != 3`.
    pub const fn from_bytes(s: &[u8]) -> Self {
        assert!(s.len() == 3, "currency code must be exactly 3 bytes");
        let mut out = [0u8; 3];
        out[0] = s[0];
        out[1] = s[1];
        out[2] = s[2];
        Self(out)
    }
}

/// A fixed-precision monetary amount.
///
/// `minor_units` is the count of indivisible currency units (cents for
/// USD, yen for JPY). `scale` is the number of fractional digits
/// (2 for USD, 0 for JPY).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Money {
    pub minor_units: i64,
    pub scale: u8,
    pub currency: Currency,
}

impl Money {
    /// Build a money amount from minor units and a currency.
    pub fn new(minor_units: i64, scale: u8, currency: Currency) -> Self {
        Self {
            minor_units,
            scale,
            currency,
        }
    }

    /// Add two money amounts. Returns `Err` if scales or currencies differ.
    pub fn add(&self, other: &Money) -> Result<Money, String> {
        if self.scale != other.scale {
            return Err(format!("scale mismatch: {} vs {}", self.scale, other.scale));
        }
        if self.currency != other.currency {
            return Err(format!(
                "currency mismatch: {:?} vs {:?}",
                self.currency.0, other.currency.0
            ));
        }
        let sum = self
            .minor_units
            .checked_add(other.minor_units)
            .ok_or_else(|| "overflow".to_string())?;
        Ok(Money {
            minor_units: sum,
            scale: self.scale,
            currency: self.currency,
        })
    }

    /// Subtract two money amounts. Same rules as `add`.
    pub fn sub(&self, other: &Money) -> Result<Money, String> {
        if self.scale != other.scale {
            return Err(format!("scale mismatch: {} vs {}", self.scale, other.scale));
        }
        if self.currency != other.currency {
            return Err(format!(
                "currency mismatch: {:?} vs {:?}",
                self.currency.0, other.currency.0
            ));
        }
        let diff = self
            .minor_units
            .checked_sub(other.minor_units)
            .ok_or_else(|| "overflow".to_string())?;
        Ok(Money {
            minor_units: diff,
            scale: self.scale,
            currency: self.currency,
        })
    }

    /// Format the amount with currency prefix: e.g. `USD 12.34`.
    /// The integer and fractional parts are split using `scale` digits.
    pub fn format_with(&self) -> String {
        let sign = if self.minor_units < 0 { "-" } else { "" };
        let abs = self.minor_units.unsigned_abs() as i64;
        let scale = 10i64.pow(self.scale as u32);
        let int_part = abs / scale;
        let frac_part = abs % scale;
        let cur = std::str::from_utf8(&self.currency.0).unwrap_or("???");
        if self.scale == 0 {
            format!("{sign}{cur} {int_part}")
        } else {
            let frac_str = format!("{:0>width$}", frac_part, width = self.scale as usize);
            format!("{sign}{cur} {int_part}.{frac_str}")
        }
    }
}

/// Parse a string of the form `"USD 12.34"` or `"12.34 USD"` into a
/// `Money`. Scale defaults to 2 for known currencies with fractional
/// units (USD/EUR/GBP) and 0 for JPY.
///
/// Returns `Err` on malformed input.
pub fn parse(s: &str) -> Result<Money, String> {
    let s = s.trim();
    if s.is_empty() {
        return Err("empty input".into());
    }
    let (cur_str, amount_str) = if s.len() >= 3 && s.as_bytes()[3] == b' ' {
        // "USD 12.34" form
        (&s[..3], s[4..].trim())
    } else if s.len() >= 4 && s.as_bytes()[s.len() - 4] == b' ' {
        // "12.34 USD" form
        (&s[s.len() - 3..], s[..s.len() - 4].trim())
    } else {
        return Err("missing currency code".into());
    };
    let currency = Currency::from_bytes(cur_str.as_bytes());

    // Parse the amount
    let (sign, body) = if let Some(rest) = amount_str.strip_prefix('-') {
        (-1, rest)
    } else {
        (1, amount_str)
    };
    let (int_part, frac_part) = match body.split_once('.') {
        Some((i, f)) => (i, f),
        None => (body, ""),
    };
    if int_part.is_empty() {
        return Err(format!("missing integer part in '{}'", amount_str));
    }
    let scale: u8 = frac_part.len() as u8;
    let parsed_int: i64 = int_part
        .parse()
        .map_err(|e| format!("invalid integer '{}': {}", int_part, e))?;
    let parsed_frac: i64 = if frac_part.is_empty() {
        0
    } else {
        frac_part
            .parse()
            .map_err(|e| format!("invalid fractional '{}': {}", frac_part, e))?
    };
    let scale_pow = 10i64.pow(scale as u32);
    let minor_units = parsed_int
        .checked_mul(scale_pow)
        .and_then(|x| x.checked_add(parsed_frac))
        .ok_or_else(|| "overflow".to_string())?;
    Ok(Money {
        minor_units: sign * minor_units,
        scale,
        currency,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn add_same_scale_and_currency() {
        let a = Money::new(123, 2, Currency::USD);
        let b = Money::new(456, 2, Currency::USD);
        let sum = a.add(&b).unwrap();
        assert_eq!(sum.minor_units, 579);
    }

    #[test]
    fn add_different_scale_errors() {
        let a = Money::new(123, 2, Currency::USD);
        let b = Money::new(456, 0, Currency::USD);
        assert!(a.add(&b).is_err());
    }

    #[test]
    fn add_different_currency_errors() {
        let a = Money::new(100, 2, Currency::USD);
        let b = Money::new(100, 2, Currency::EUR);
        assert!(a.add(&b).is_err());
    }

    #[test]
    fn sub_returns_signed_difference() {
        let a = Money::new(500, 2, Currency::USD);
        let b = Money::new(123, 2, Currency::USD);
        assert_eq!(a.sub(&b).unwrap().minor_units, 377);
        assert_eq!(b.sub(&a).unwrap().minor_units, -377);
    }

    #[test]
    fn format_basic() {
        let m = Money::new(1234, 2, Currency::USD);
        assert_eq!(m.format_with(), "USD 12.34");
    }

    #[test]
    fn format_negative() {
        let m = Money::new(-1234, 2, Currency::USD);
        assert_eq!(m.format_with(), "-USD 12.34");
    }

    #[test]
    fn format_zero_scale_jpy() {
        let m = Money::new(1234, 0, Currency::JPY);
        assert_eq!(m.format_with(), "JPY 1234");
    }

    #[test]
    fn format_pads_fractional_zeros() {
        let m = Money::new(5, 2, Currency::USD);
        assert_eq!(m.format_with(), "USD 0.05");
    }

    #[test]
    fn parse_currency_first() {
        let m = parse("USD 12.34").unwrap();
        assert_eq!(m.minor_units, 1234);
        assert_eq!(m.scale, 2);
        assert_eq!(m.currency, Currency::USD);
    }

    #[test]
    fn parse_currency_last() {
        let m = parse("12.34 USD").unwrap();
        assert_eq!(m.minor_units, 1234);
        assert_eq!(m.scale, 2);
    }

    #[test]
    fn parse_negative() {
        let m = parse("USD -5.99").unwrap();
        assert_eq!(m.minor_units, -599);
    }

    #[test]
    fn parse_jpy_no_fraction() {
        let m = parse("JPY 1000").unwrap();
        assert_eq!(m.minor_units, 1000);
        assert_eq!(m.scale, 0);
    }

    #[test]
    fn round_trip_format_parse() {
        let original = Money::new(987654, 2, Currency::USD);
        let formatted = original.format_with();
        let parsed = parse(&formatted).unwrap();
        assert_eq!(parsed.minor_units, original.minor_units);
        assert_eq!(parsed.scale, original.scale);
    }
}