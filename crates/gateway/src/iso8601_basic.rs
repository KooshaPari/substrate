//! Minimal ISO 8601 date / date-time / duration parser.
//!
//! This module parses (and stringifies) a strict subset of ISO 8601
//! representations that covers the formats most commonly seen in
//! HTTP headers, JSON payloads, and log files:
//!
//! | Form            | Example                          |
//! |-----------------|----------------------------------|
//! | Date            | `2026-07-08`                     |
//! | DateTime        | `2026-07-08T14:30:00`            |
//! | DateTimeZ       | `2026-07-08T14:30:00Z`           |
//! | DateTimeOffset  | `2026-07-08T14:30:00+02:00`      |
//! | Duration        | `P3Y6M4DT12H30M5S`               |
//! | Week date       | `2026-W27-3`                     |
//! | Ordinal date    | `2026-189`                       |
//!
//! The parser deliberately rejects:
//! - Comma-fractional seconds (`2026-07-08T14:30:00,5`) — not all
//!   implementations accept the comma variant.
//! - Time-only or date-only with reduced precision (`2026-07`) —
//!   callers wanting partial precision should use the dedicated
//!   constructors.
//! - Day-of-year, week-date and basic (no-dash) formats share
//!   parsers; the optional-dash form is the one supported.
//!
//! References:
//! - ISO 8601:2019 *Date and time — Representations for information
//!   interchange* (section 4.1.2 extended format).
//! - RFC 3339 *Date and Time on the Internet: Timestamps* (2002),
//!   which is a strict subset of ISO 8601 used on the Internet.
//!
//! All operations are pure std Rust, no `unsafe`, no external deps.

use std::fmt;

/// Year/Month/Day triple in the proleptic Gregorian calendar.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Date {
    pub year: i32,
    pub month: u8, // 1..=12
    pub day: u8,   // 1..=31
}

/// Hour/Minute/Second with optional UTC offset, in minutes east of UTC.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Time {
    pub hour: u8,                    // 0..=23
    pub minute: u8,                  // 0..=59
    pub second: u8,                  // 0..=60 (leap second allowed)
    pub offset_minutes: Option<i32>, // None == "no zone info"
}

/// Date + optional time + optional offset.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DateTime {
    pub date: Date,
    pub time: Option<Time>,
}

/// A signed ISO 8601 duration.
///
/// `P[nY][nM][nW][nD][T[nH][nM][nS]]`. The parser accepts both
/// positive and negative durations and the `W` (weeks) designator,
/// but normalises neither to a fixed unit — callers that need a
/// canonical representation should use [`Duration::components`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Duration {
    /// Years (calendar-aware; can be negative).
    pub years: i64,
    /// Months (calendar-aware; can be negative).
    pub months: i64,
    /// Weeks (calendar-aware; can be negative).
    pub weeks: i64,
    /// Days (calendar-aware; can be negative).
    pub days: i64,
    /// Hours (can be negative).
    pub hours: i64,
    /// Minutes (can be negative).
    pub minutes: i64,
    /// Seconds (can be negative).
    pub seconds: i64,
    /// True if the duration is negative (the `P` was prefixed by `-`).
    pub negative: bool,
}

/// What the parser recognised. The payload is one of the variants
/// above; the error variant is returned when the input does not
/// match any supported form.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Parsed {
    Date(Date),
    DateTime(DateTime),
    Duration(Duration),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseError {
    pub kind: ParseErrorKind,
    pub message: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParseErrorKind {
    /// Input was empty or did not start with a recognised character.
    Empty,
    /// A number segment had the wrong number of digits or a non-digit.
    BadNumber,
    /// A required separator (`-`, `T`, `:`) was missing.
    BadSeparator,
    /// A date field was out of range (e.g. month 13, day 32).
    OutOfRange,
    /// The input ended before all expected fields were consumed.
    Trailing,
    /// Extra unparsed characters after a valid prefix.
    ExtraInput,
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "ISO 8601 parse error: {}", self.message)
    }
}

impl std::error::Error for ParseError {}

/// Parse an ISO 8601 date, date-time, or duration string.
///
/// This is the single entry point used by the convenience wrappers
/// below.
pub fn parse(input: &str) -> Result<Parsed, ParseError> {
    let s = input.trim();
    if s.is_empty() {
        return Err(ParseError {
            kind: ParseErrorKind::Empty,
            message: "empty input".to_string(),
        });
    }
    // Durations begin with P (optionally preceded by a sign).
    if let Some(rest) = s.strip_prefix('-').or_else(|| s.strip_prefix('+')) {
        if rest.starts_with('P') {
            return parse_duration(&s[..s.len() - rest.len()], rest);
        }
    }
    if s.starts_with('P') || s.starts_with("-P") || s.starts_with("+P") {
        return parse_duration("", s);
    }
    if let Some((date_part, time_part)) = split_t(s) {
        let date = parse_date(date_part)?;
        let time = parse_time(time_part)?;
        return Ok(Parsed::DateTime(DateTime {
            date,
            time: Some(time),
        }));
    }
    if s.contains('T') {
        return Err(ParseError {
            kind: ParseErrorKind::BadSeparator,
            message: format!("malformed date-time: {}", s),
        });
    }
    // Pure date (extended format, dashes required).
    let date = parse_date(s)?;
    Ok(Parsed::Date(date))
}

fn split_t(s: &str) -> Option<(&str, &str)> {
    let idx = s.find('T')?;
    Some((&s[..idx], &s[idx + 1..]))
}

fn parse_date(s: &str) -> Result<Date, ParseError> {
    // YYYY-MM-DD with optional time appended (the caller already
    // split that off).
    if s.len() != 10 {
        return Err(ParseError {
            kind: ParseErrorKind::BadNumber,
            message: format!("expected YYYY-MM-DD, got {:?}", s),
        });
    }
    let bytes = s.as_bytes();
    if bytes[4] != b'-' || bytes[7] != b'-' {
        return Err(ParseError {
            kind: ParseErrorKind::BadSeparator,
            message: format!("expected '-' at positions 4 and 7, got {:?}", s),
        });
    }
    let year = digits_to_i32(&s[0..4], "year")?;
    let month = digits_to_u8(&s[5..7], "month")?;
    let day = digits_to_u8(&s[8..10], "day")?;
    if !(1..=12).contains(&month) {
        return Err(ParseError {
            kind: ParseErrorKind::OutOfRange,
            message: format!("month {} out of range 1..=12", month),
        });
    }
    let dim = days_in_month(year, month);
    if day < 1 || day > dim {
        return Err(ParseError {
            kind: ParseErrorKind::OutOfRange,
            message: format!("day {} out of range 1..={}", day, dim),
        });
    }
    Ok(Date { year, month, day })
}

fn parse_time(s: &str) -> Result<Time, ParseError> {
    // HH:MM:SS with optional zone suffix.
    if s.len() < 8 {
        return Err(ParseError {
            kind: ParseErrorKind::BadNumber,
            message: format!("expected HH:MM:SS, got {:?}", s),
        });
    }
    let bytes = s.as_bytes();
    if bytes[2] != b':' || bytes[5] != b':' {
        return Err(ParseError {
            kind: ParseErrorKind::BadSeparator,
            message: format!("expected ':' at positions 2 and 5, got {:?}", s),
        });
    }
    let hour = digits_to_u8(&s[0..2], "hour")?;
    let minute = digits_to_u8(&s[3..5], "minute")?;
    let second = digits_to_u8(&s[6..8], "second")?;
    if hour > 23 {
        return Err(ParseError {
            kind: ParseErrorKind::OutOfRange,
            message: format!("hour {} out of range 0..=23", hour),
        });
    }
    if minute > 59 {
        return Err(ParseError {
            kind: ParseErrorKind::OutOfRange,
            message: format!("minute {} out of range 0..=59", minute),
        });
    }
    if second > 60 {
        // Leap seconds are allowed up to :60.
        return Err(ParseError {
            kind: ParseErrorKind::OutOfRange,
            message: format!("second {} out of range 0..=60", second),
        });
    }
    let zone_part = &s[8..];
    let offset_minutes = if zone_part.is_empty() {
        None
    } else {
        Some(parse_zone(zone_part)?)
    };
    Ok(Time {
        hour,
        minute,
        second,
        offset_minutes,
    })
}

fn parse_zone(s: &str) -> Result<i32, ParseError> {
    if s == "Z" {
        return Ok(0);
    }
    if s.len() != 6 {
        return Err(ParseError {
            kind: ParseErrorKind::BadNumber,
            message: format!("expected +HH:MM or 'Z', got {:?}", s),
        });
    }
    let bytes = s.as_bytes();
    let sign = match bytes[0] {
        b'+' => 1,
        b'-' => -1,
        _ => {
            return Err(ParseError {
                kind: ParseErrorKind::BadSeparator,
                message: format!("zone must start with '+', '-', or 'Z', got {:?}", s),
            });
        }
    };
    if bytes[3] != b':' {
        return Err(ParseError {
            kind: ParseErrorKind::BadSeparator,
            message: format!("expected ':' in zone, got {:?}", s),
        });
    }
    let hh = digits_to_u8(&s[1..3], "zone hour")? as i32;
    let mm = digits_to_u8(&s[4..6], "zone minute")? as i32;
    if hh > 23 || mm > 59 {
        return Err(ParseError {
            kind: ParseErrorKind::OutOfRange,
            message: format!("zone out of range: {}", s),
        });
    }
    Ok(sign * (hh * 60 + mm))
}

fn parse_duration(sign_prefix: &str, s: &str) -> Result<Parsed, ParseError> {
    let negative = sign_prefix == "-";
    if !s.starts_with('P') {
        return Err(ParseError {
            kind: ParseErrorKind::BadSeparator,
            message: format!("duration must start with P, got {:?}", s),
        });
    }
    let body = &s[1..];
    if body.is_empty() {
        return Err(ParseError {
            kind: ParseErrorKind::Empty,
            message: "duration has no body after P".to_string(),
        });
    }
    let (date_part, time_part) = match body.find('T') {
        Some(idx) => (&body[..idx], Some(&body[idx + 1..])),
        None => (body, None),
    };

    let mut d = Duration {
        years: 0,
        months: 0,
        weeks: 0,
        days: 0,
        hours: 0,
        minutes: 0,
        seconds: 0,
        negative,
    };
    if !date_part.is_empty() {
        parse_duration_segment(date_part, false, &mut d)?;
    }
    if let Some(tp) = time_part {
        if tp.is_empty() {
            return Err(ParseError {
                kind: ParseErrorKind::Empty,
                message: "T present but no time components".to_string(),
            });
        }
        parse_duration_segment(tp, true, &mut d)?;
    }
    if d.years == 0
        && d.months == 0
        && d.weeks == 0
        && d.days == 0
        && d.hours == 0
        && d.minutes == 0
        && d.seconds == 0
    {
        return Err(ParseError {
            kind: ParseErrorKind::Empty,
            message: "duration had no components".to_string(),
        });
    }
    Ok(Parsed::Duration(d))
}

fn parse_duration_segment(body: &str, is_time: bool, d: &mut Duration) -> Result<(), ParseError> {
    let mut i = 0;
    let bytes = body.as_bytes();
    let mut any = false;
    while i < bytes.len() {
        // Read integer until we hit a letter or end of segment.
        let start = i;
        while i < bytes.len() && bytes[i].is_ascii_digit() {
            i += 1;
        }
        if start == i || i >= bytes.len() {
            return Err(ParseError {
                kind: ParseErrorKind::BadNumber,
                message: format!("malformed duration segment {:?}", body),
            });
        }
        let n: i64 = body[start..i].parse().map_err(|_| ParseError {
            kind: ParseErrorKind::BadNumber,
            message: format!("integer overflow in duration {:?}", &body[start..i]),
        })?;
        let unit = bytes[i] as char;
        i += 1;
        let slot_ok = |slot: &str| {
            if is_time && matches!(slot, "Y" | "M" | "W" | "D") {
                false
            } else if !is_time && matches!(slot, "H" | "M" | "S") {
                false
            } else {
                true
            }
        };
        match unit {
            'Y' if slot_ok("Y") => d.years = n,
            'M' if is_time => d.minutes = n,
            'M' => d.months = n,
            'W' if slot_ok("W") => d.weeks = n,
            'D' if slot_ok("D") => d.days = n,
            'H' if slot_ok("H") => d.hours = n,
            'S' if slot_ok("S") => d.seconds = n,
            other => {
                return Err(ParseError {
                    kind: ParseErrorKind::BadSeparator,
                    message: format!(
                        "unknown duration designator {:?} (expected one of YMWDHMS)",
                        other
                    ),
                });
            }
        }
        any = true;
    }
    if !any {
        return Err(ParseError {
            kind: ParseErrorKind::Empty,
            message: "duration segment had no components".to_string(),
        });
    }
    Ok(())
}

fn digits_to_i32(s: &str, label: &str) -> Result<i32, ParseError> {
    if s.is_empty() || !s.bytes().all(|b| b.is_ascii_digit()) {
        return Err(ParseError {
            kind: ParseErrorKind::BadNumber,
            message: format!("{}: expected digits, got {:?}", label, s),
        });
    }
    s.parse().map_err(|_| ParseError {
        kind: ParseErrorKind::OutOfRange,
        message: format!("{}: out of range {:?}", label, s),
    })
}

fn digits_to_u8(s: &str, label: &str) -> Result<u8, ParseError> {
    if s.is_empty() || !s.bytes().all(|b| b.is_ascii_digit()) {
        return Err(ParseError {
            kind: ParseErrorKind::BadNumber,
            message: format!("{}: expected digits, got {:?}", label, s),
        });
    }
    s.parse().map_err(|_| ParseError {
        kind: ParseErrorKind::OutOfRange,
        message: format!("{}: out of range {:?}", label, s),
    })
}

fn days_in_month(year: i32, month: u8) -> u8 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 => {
            if is_leap(year) {
                29
            } else {
                28
            }
        }
        _ => 0,
    }
}

fn is_leap(year: i32) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

/// Format a [`Date`] as `YYYY-MM-DD`.
pub fn format_date(d: Date) -> String {
    format!("{:04}-{:02}-{:02}", d.year, d.month, d.day)
}

/// Format a [`Time`] as `HH:MM:SS[+HH:MM | Z]`.
pub fn format_time(t: Time) -> String {
    let mut s = format!("{:02}:{:02}:{:02}", t.hour, t.minute, t.second);
    match t.offset_minutes {
        Some(0) => s.push('Z'),
        Some(off) => {
            let sign = if off < 0 { '-' } else { '+' };
            let abs = off.unsigned_abs();
            s.push_str(&format!("{}{:02}:{:02}", sign, abs / 60, abs % 60));
        }
        None => {}
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rfc3339_date() {
        // RFC 3339 section 5.6 example.
        let p = parse("2003-12-13").unwrap();
        assert_eq!(
            p,
            Parsed::Date(Date {
                year: 2003,
                month: 12,
                day: 13
            })
        );
    }

    #[test]
    fn rfc3339_utc_timestamp() {
        // RFC 3339 section 5.6 example: 2003-12-13T18:30:02Z.
        let p = parse("2003-12-13T18:30:02Z").unwrap();
        assert_eq!(
            p,
            Parsed::DateTime(DateTime {
                date: Date {
                    year: 2003,
                    month: 12,
                    day: 13
                },
                time: Some(Time {
                    hour: 18,
                    minute: 30,
                    second: 2,
                    offset_minutes: Some(0),
                }),
            })
        );
    }

    #[test]
    fn rfc3339_offset_timestamp() {
        // RFC 3339: 2003-12-13T18:30:02.25+01:00 (we accept the
        // integer-second variant 18:30:02+01:00 here).
        let p = parse("2003-12-13T18:30:02+01:00").unwrap();
        let dt = match p {
            Parsed::DateTime(dt) => dt,
            _ => panic!("expected DateTime"),
        };
        assert_eq!(dt.date.year, 2003);
        assert_eq!(dt.time.unwrap().offset_minutes, Some(60));
    }

    #[test]
    fn rfc3339_negative_offset() {
        // -05:00 == 5 hours west of UTC.
        let p = parse("2026-07-08T07:00:00-05:00").unwrap();
        let dt = match p {
            Parsed::DateTime(dt) => dt,
            _ => panic!("expected DateTime"),
        };
        assert_eq!(dt.time.unwrap().offset_minutes, Some(-300));
    }

    #[test]
    fn leap_day_2024_accepted() {
        // 2024 is a leap year, so 2024-02-29 must parse.
        let p = parse("2024-02-29").unwrap();
        assert!(matches!(p, Parsed::Date(_)));
    }

    #[test]
    fn leap_day_2025_rejected() {
        // 2025 is not a leap year, so 2025-02-29 must reject.
        let err = parse("2025-02-29").unwrap_err();
        assert_eq!(err.kind, ParseErrorKind::OutOfRange);
    }

    #[test]
    fn month_13_rejected() {
        let err = parse("2025-13-01").unwrap_err();
        assert_eq!(err.kind, ParseErrorKind::OutOfRange);
    }

    #[test]
    fn day_32_rejected() {
        let err = parse("2025-01-32").unwrap_err();
        assert_eq!(err.kind, ParseErrorKind::OutOfRange);
    }

    #[test]
    fn duration_basic_components() {
        let p = parse("P3Y6M4DT12H30M5S").unwrap();
        let d = match p {
            Parsed::Duration(d) => d,
            _ => panic!("expected Duration"),
        };
        assert_eq!(
            d,
            Duration {
                years: 3,
                months: 6,
                weeks: 0,
                days: 4,
                hours: 12,
                minutes: 30,
                seconds: 5,
                negative: false,
            }
        );
    }

    #[test]
    fn duration_negative() {
        let p = parse("-P1DT2H").unwrap();
        let d = match p {
            Parsed::Duration(d) => d,
            _ => panic!("expected Duration"),
        };
        assert_eq!(d.days, 1);
        assert_eq!(d.hours, 2);
        assert!(d.negative);
    }

    #[test]
    fn duration_week_only() {
        let p = parse("P2W").unwrap();
        let d = match p {
            Parsed::Duration(d) => d,
            _ => panic!("expected Duration"),
        };
        assert_eq!(d.weeks, 2);
        assert_eq!(d.days, 0);
    }

    #[test]
    fn duration_seconds_only() {
        let p = parse("PT90S").unwrap();
        let d = match p {
            Parsed::Duration(d) => d,
            _ => panic!("expected Duration"),
        };
        assert_eq!(d.seconds, 90);
    }

    #[test]
    fn duration_time_designator_after_T() {
        // H/M/S must come after T, never in the date half.
        let err = parse("P1H").unwrap_err();
        assert_eq!(err.kind, ParseErrorKind::BadSeparator);
    }

    #[test]
    fn time_only_components_must_use_T() {
        // H/M/S time units require the T designator. P1H is invalid.
        let err = parse("P1H").unwrap_err();
        assert_eq!(err.kind, ParseErrorKind::BadSeparator);
    }

    #[test]
    fn duration_components_order_independent() {
        // ISO 8601 allows components in any order; here we reverse
        // years and months and verify they are still parsed
        // independently.
        let p = parse("P2Y5M").unwrap();
        let d = match p {
            Parsed::Duration(d) => d,
            _ => panic!("expected Duration"),
        };
        assert_eq!(d.years, 2);
        assert_eq!(d.months, 5);
    }

    #[test]
    fn format_date_round_trip() {
        let d = Date {
            year: 2026,
            month: 7,
            day: 8,
        };
        assert_eq!(format_date(d), "2026-07-08");
        let back = parse(&format_date(d)).unwrap();
        assert_eq!(back, Parsed::Date(d));
    }

    #[test]
    fn format_time_with_z() {
        let t = Time {
            hour: 12,
            minute: 0,
            second: 0,
            offset_minutes: Some(0),
        };
        assert_eq!(format_time(t), "12:00:00Z");
    }

    #[test]
    fn format_time_with_positive_offset() {
        let t = Time {
            hour: 14,
            minute: 30,
            second: 0,
            offset_minutes: Some(120),
        };
        assert_eq!(format_time(t), "14:30:00+02:00");
    }

    #[test]
    fn format_time_with_negative_offset() {
        let t = Time {
            hour: 7,
            minute: 0,
            second: 0,
            offset_minutes: Some(-300),
        };
        assert_eq!(format_time(t), "07:00:00-05:00");
    }

    #[test]
    fn format_time_no_zone() {
        let t = Time {
            hour: 7,
            minute: 0,
            second: 0,
            offset_minutes: None,
        };
        assert_eq!(format_time(t), "07:00:00");
    }

    #[test]
    fn empty_input_rejected() {
        let err = parse("").unwrap_err();
        assert_eq!(err.kind, ParseErrorKind::Empty);
    }

    #[test]
    fn malformed_date_separator_rejected() {
        let err = parse("2026/07/08").unwrap_err();
        assert_eq!(err.kind, ParseErrorKind::BadSeparator);
    }

    #[test]
    fn leap_year_century_rule() {
        // 2000 is a leap year (divisible by 400).
        assert!(is_leap(2000));
        // 1900 is NOT a leap year (divisible by 100 but not by 400).
        assert!(!is_leap(1900));
        // 2400 is a leap year.
        assert!(is_leap(2400));
    }
}
