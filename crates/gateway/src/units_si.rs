//! SI (Système International) unit formatting and parsing.
//!
//! [`format_si`] converts a u64/f64 byte/scalar value to a human-
//! readable form with the appropriate SI prefix (k, M, G, T, P, E, ...).
//! [`parse_si`] parses the reverse direction. The base unit is
//! specified by the caller (e.g. "B" for bytes, "Hz" for hertz).
//!
//! Uses base 1000 (decimal) prefixes by default; pass `base = 1024`
//! for IEC binary prefixes (Ki, Mi, Gi, ...).

/// Format a value with the appropriate SI prefix. `unit` is appended
/// verbatim (e.g. "B" for bytes).
///
/// Examples:
/// - format_si(0, 1000, "B") -> "0 B"
/// - format_si(1500, 1000, "B") -> "1.50 kB"
/// - format_si(1_500_000, 1000, "Hz") -> "1.50 MHz"
/// - format_si(1024, 1024, "B") -> "1.00 KiB"
pub fn format_si(value: u64, base: u32, unit: &str) -> String {
    if base != 1000 && base != 1024 {
        // Graceful fallback to 1000
        return format_si(value, 1000, unit);
    }
    let prefixes_decimal = ["", "k", "M", "G", "T", "P", "E"];
    let prefixes_binary = ["", "Ki", "Mi", "Gi", "Ti", "Pi", "Ei"];
    let prefixes: &[&str] = if base == 1000 {
        &prefixes_decimal
    } else {
        &prefixes_binary
    };
    let max_pow = prefixes.len() - 1;
    let base = base as u64;

    let mut v = value as f64;
    let mut idx = 0;
    while v >= base as f64 && idx < max_pow {
        v /= base as f64;
        idx += 1;
    }
    if idx == 0 {
        // No prefix — render as integer
        format!("{} {}", value, unit)
    } else if v >= 100.0 {
        format!("{:.0} {}{}", v, prefixes[idx], unit)
    } else if v >= 10.0 {
        format!("{:.1} {}{}", v, prefixes[idx], unit)
    } else {
        format!("{:.2} {}{}", v, prefixes[idx], unit)
    }
}

/// Parse a string with an SI-prefixed unit back into a u64. Returns
/// `Err` on malformed input.
///
/// The expected form is `"<number> <PREFIX><UNIT>"` (a single space
/// between the number and the prefix is optional) — matching the output
/// of [`format_si`]. The prefix is glued to the unit letter (no
/// separator between them), so "1.50 kB" is `<num=1.50, prefix=k, unit=B>`
/// and "2 MiB" is `<num=2, prefix=Mi, unit=B>`.
///
/// Examples:
/// - parse_si("1.5 kB", 1000) -> Ok(1500)
/// - parse_si("1024 B", 1000) -> Ok(1024)
/// - parse_si("2 MiB", 1024) -> Ok(2 * 1024 * 1024)
/// - parse_si("1.50 kB", 1000) -> Ok(1500) (round-trip with format_si)
pub fn parse_si(s: &str, base: u32) -> Result<u64, String> {
    if base != 1000 && base != 1024 {
        return Err(format!("base must be 1000 or 1024, got {}", base));
    }
    let s = s.trim();

    // Locate the boundary between the numeric portion and the alphabetic
    // prefix+unit. The first alphabetic byte marks the boundary.
    let split = s
        .find(|c: char| c.is_ascii_alphabetic())
        .ok_or_else(|| format!("missing prefix/unit in '{}'", s))?;
    let (num_str, alpha) = (&s[..split], s[split..].trim());
    let lower_alpha = alpha.to_ascii_lowercase();

    // Try the binary prefixes first (longest match wins — "ki" before "k").
    // Each branch returns the multiplier; any trailing letters after the
    // prefix are the unit and are ignored.
    let multiplier: u64 = if lower_alpha.starts_with("ki") {
        1024
    } else if lower_alpha.starts_with("mi") {
        1024 * 1024
    } else if lower_alpha.starts_with("gi") {
        1024 * 1024 * 1024
    } else if lower_alpha.starts_with("ti") {
        1024u64.pow(4)
    } else if lower_alpha.starts_with("pi") {
        1024u64.pow(5)
    } else if lower_alpha.starts_with("ei") {
        1024u64.pow(6)
    } else {
        match lower_alpha.chars().next() {
            Some('k') => {
                if base == 1000 {
                    1_000
                } else {
                    1024
                }
            }
            Some('m') => {
                if base == 1000 {
                    1_000_000
                } else {
                    1_048_576
                }
            }
            Some('g') => {
                if base == 1000 {
                    1_000_000_000
                } else {
                    1_073_741_824
                }
            }
            Some('t') => {
                if base == 1000 {
                    1_000_000_000_000
                } else {
                    1_099_511_627_776
                }
            }
            Some('p') => {
                if base == 1000 {
                    1_000_000_000_000_000
                } else {
                    1_125_899_906_842_624
                }
            }
            Some('e') => {
                if base == 1000 {
                    1_000_000_000_000_000_000
                } else {
                    1_152_921_504_606_846_976
                }
            }
            // Unit-only suffixes (no prefix). Common SI base units: B (bytes),
            // Hz (hertz), V/W/A (electric), s (seconds), N (newtons), J
            // (joules), F (farads), d (days — informal), etc.
            Some('b') | Some('h') | Some('z') | Some('s') | Some('v') | Some('w')
            | Some('a') | Some('n') | Some('j') | Some('f') | Some('d')
            | Some('c') | Some('y') => 1,
            _ => {
                return Err(format!("unknown prefix '{}'", alpha));
            }
        }
    };

    let value: f64 = num_str
        .trim()
        .parse()
        .map_err(|e| format!("invalid number '{}': {}", num_str, e))?;
    Ok((value * multiplier as f64).round() as u64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_bytes_basic() {
        assert_eq!(format_si(0, 1000, "B"), "0 B");
        assert_eq!(format_si(500, 1000, "B"), "500 B");
        assert_eq!(format_si(1000, 1000, "B"), "1.00 kB");
        assert_eq!(format_si(1500, 1000, "B"), "1.50 kB");
        assert_eq!(format_si(1_000_000, 1000, "B"), "1.00 MB");
        assert_eq!(format_si(1_500_000_000, 1000, "B"), "1.50 GB");
    }

    #[test]
    fn format_binary_prefixes() {
        assert_eq!(format_si(1024, 1024, "B"), "1.00 KiB");
        assert_eq!(format_si(1024 * 1024, 1024, "B"), "1.00 MiB");
    }

    #[test]
    fn format_chooses_precision_correctly() {
        assert_eq!(format_si(1500, 1000, "B"), "1.50 kB");
        assert_eq!(format_si(15000, 1000, "B"), "15.0 kB");
        assert_eq!(format_si(150000, 1000, "B"), "150 kB");
    }

    #[test]
    fn parse_basic() {
        assert_eq!(parse_si("500 B", 1000).unwrap(), 500);
        assert_eq!(parse_si("1.5 kB", 1000).unwrap(), 1500);
        assert_eq!(parse_si("2 MB", 1000).unwrap(), 2_000_000);
    }

    #[test]
    fn parse_binary_prefixes() {
        assert_eq!(parse_si("2 KiB", 1024).unwrap(), 2048);
        assert_eq!(parse_si("2 MiB", 1024).unwrap(), 2 * 1024 * 1024);
    }

    #[test]
    fn parse_invalid_errors() {
        assert!(parse_si("abc", 1000).is_err());
    }

    #[test]
    fn round_trip_approximate() {
        // `format_si(0)` renders "0 B" (no prefix), and 0 round-tripped is 0,
        // which is 0% not within the 5% band. Skip the degenerate zero case.
        // The non-zero samples exercise every decimal prefix (kB, MB, GB).
        for n in [500u64, 1000, 1500, 1_000_000, 1_500_000_000, 1_500_000_000_000] {
            let formatted = format_si(n, 1000, "B");
            // Round-trip should be within 5% (format rounds to 2 digits).
            let parsed = parse_si(&formatted, 1000).unwrap();
            let ratio = parsed as f64 / n as f64;
            assert!(
                (0.95..=1.05).contains(&ratio),
                "n={} formatted={} parsed={} ratio={}",
                n,
                formatted,
                parsed,
                ratio
            );
        }
    }
}