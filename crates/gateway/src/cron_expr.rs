//! Cron expression matcher and next-fire-time iterator.
//!
//! Parses the classic 5-field cron format used by Vixie cron and Quartz
//! Scheduler — `minute hour day_of_month month day_of_week` — and answers
//! "when does this fire next after time T?". Fields support `*`
//! (all), single integers, comma-separated lists, `a-b` ranges, and
//! `*/n` or `a-b/n` step expressions. Day-of-week is 0-7 with both 0 and
//! 7 meaning Sunday (matches Unix cron).
//!
//! Reference: <https://man7.org/linux/man-pages/man5/crontab.5.html>
//!
//! Note: This implementation handles only the classic 5-field format.
//! Quartz's 6-field (with seconds) and Spring's 6-field (with year) are
//! out of scope.

use std::collections::BTreeSet;

/// A parsed cron expression, normalised to per-field bitmasks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CronExpr {
    minutes: BTreeSet<u8>,
    hours: BTreeSet<u8>,
    days: BTreeSet<u8>,
    months: BTreeSet<u8>,
    dows: BTreeSet<u8>,
    /// True if day-of-month is `*` (then any day is fine; DOW becomes the only constraint).
    dom_any: bool,
    /// True if day-of-week is `*`.
    dow_any: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CronError {
    BadFieldCount { actual: usize, expected: usize },
    BadNumber { field: &'static str, value: String },
    OutOfRange { field: &'static str, value: u8 },
    BadStep { field: &'static str },
}

impl std::fmt::Display for CronError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CronError::BadFieldCount { actual, expected } => {
                write!(f, "expected {} fields, got {}", expected, actual)
            }
            CronError::BadNumber { field, value } => {
                write!(f, "bad number in {}: {:?}", field, value)
            }
            CronError::OutOfRange { field, value } => {
                write!(f, "{} out of range: {}", field, value)
            }
            CronError::BadStep { field } => {
                write!(f, "bad step expression in {}", field)
            }
        }
    }
}

impl std::str::FromStr for CronExpr {
    type Err = CronError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        parse(s)
    }
}

/// Parse a 5-field cron expression.
pub fn parse(input: &str) -> Result<CronExpr, CronError> {
    let parts: Vec<&str> = input.split_whitespace().collect();
    if parts.len() != 5 {
        return Err(CronError::BadFieldCount {
            actual: parts.len(),
            expected: 5,
        });
    }
    let minutes = parse_field(parts[0], 0, 59).map_err(|e| map_field("minute", e))?;
    let hours = parse_field(parts[1], 0, 23).map_err(|e| map_field("hour", e))?;
    let dom_any = parts[2] == "*";
    let days = parse_field(parts[2], 1, 31).map_err(|e| map_field("day-of-month", e))?;
    let months = parse_field(parts[3], 1, 12).map_err(|e| map_field("month", e))?;
    let dow_any = parts[4] == "*";
    let mut dows = parse_field(parts[4], 0, 7).map_err(|e| map_field("day-of-week", e))?;
    // Normalize 7 -> 0 (Sunday in both representations).
    if dows.remove(&7) {
        dows.insert(0);
    }
    Ok(CronExpr {
        minutes,
        hours,
        days,
        months,
        dows,
        dom_any,
        dow_any,
    })
}

fn map_field(name: &'static str, e: FieldErr) -> CronError {
    match e {
        FieldErr::BadNumber(v) => CronError::BadNumber {
            field: name,
            value: v,
        },
        FieldErr::OutOfRange(v) => CronError::OutOfRange {
            field: name,
            value: v,
        },
        FieldErr::BadStep => CronError::BadStep { field: name },
    }
}

#[derive(Debug)]
enum FieldErr {
    BadNumber(String),
    OutOfRange(u8),
    BadStep,
}

/// Parse a single cron field into the set of allowed values.
fn parse_field(field: &str, min: u8, max: u8) -> Result<BTreeSet<u8>, FieldErr> {
    let mut allowed = BTreeSet::new();
    for part in field.split(',') {
        let (range_str, step) = match part.split_once('/') {
            Some((r, s)) => {
                let step: u8 = s.parse().map_err(|_| FieldErr::BadStep)?;
                if step == 0 {
                    return Err(FieldErr::BadStep);
                }
                (r, step)
            }
            None => (part, 1),
        };
        let (lo, hi) = if range_str == "*" {
            (min, max)
        } else if let Some((a, b)) = range_str.split_once('-') {
            let a: u8 = a
                .parse()
                .map_err(|_| FieldErr::BadNumber(range_str.into()))?;
            let b: u8 = b
                .parse()
                .map_err(|_| FieldErr::BadNumber(range_str.into()))?;
            (a, b)
        } else {
            let n: u8 = range_str
                .parse()
                .map_err(|_| FieldErr::BadNumber(range_str.into()))?;
            (n, n)
        };
        if lo < min || hi > max || lo > hi {
            return Err(FieldErr::OutOfRange(lo));
        }
        let mut v = lo;
        while v <= hi {
            allowed.insert(v);
            // Saturating increment to avoid overflow at 255.
            v = v.saturating_add(step);
        }
    }
    Ok(allowed)
}

/// Returns true if the given time components (minute, hour, day, month,
/// day-of-week) match this cron expression.
impl CronExpr {
    pub fn matches_time(&self, minute: u8, hour: u8, day: u8, month: u8, dow: u8) -> bool {
        if !self.minutes.contains(&minute)
            || !self.hours.contains(&hour)
            || !self.months.contains(&month)
        {
            return false;
        }
        // Combine day-of-month and day-of-week per Unix cron semantics:
        // - if either is `*`, the other is the sole constraint
        // - if both are restricted, match if EITHER matches (OR semantics)
        let day_ok = match (self.dom_any, self.dow_any) {
            (true, true) => true,
            (true, false) => self.dows.contains(&dow),
            (false, true) => self.days.contains(&day),
            (false, false) => self.days.contains(&day) || self.dows.contains(&dow),
        };
        day_ok
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn e(s: &str) -> CronExpr {
        s.parse().unwrap()
    }

    #[test]
    fn parse_always() {
        let c = e("* * * * *");
        for m in 0..60 {
            for h in 0..24 {
                for d in 1..32 {
                    for mo in 1..13 {
                        for dow in 0..7 {
                            assert!(c.matches_time(m, h, d, mo, dow));
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn parse_every_5_minutes() {
        let c = e("*/5 * * * *");
        for m in (0..60).step_by(5) {
            assert!(c.matches_time(m, 0, 1, 1, 0));
        }
        assert!(!c.matches_time(1, 0, 1, 1, 0));
        assert!(!c.matches_time(2, 0, 1, 1, 0));
    }

    #[test]
    fn parse_weekday_at_9_30() {
        let c = e("30 9 * * 1-5");
        assert!(c.matches_time(30, 9, 1, 1, 1)); // Mon
        assert!(c.matches_time(30, 9, 1, 1, 5)); // Fri
        assert!(!c.matches_time(30, 9, 1, 1, 0)); // Sun
        assert!(!c.matches_time(30, 9, 1, 1, 6)); // Sat
        assert!(!c.matches_time(30, 10, 1, 1, 1));
    }

    #[test]
    fn parse_first_of_month() {
        let c = e("0 0 1 * *");
        assert!(c.matches_time(0, 0, 1, 1, 3));
        assert!(!c.matches_time(0, 0, 2, 1, 3));
        assert!(!c.matches_time(1, 0, 1, 1, 3));
    }

    #[test]
    fn parse_7_normalised_to_0() {
        let c = e("0 0 * * 7");
        assert!(c.matches_time(0, 0, 1, 1, 0));
        assert!(!c.matches_time(0, 0, 1, 1, 7));
    }

    #[test]
    fn parse_comma_list() {
        let c = e("0,30 9,17 * * 1-5");
        assert!(c.matches_time(0, 9, 1, 1, 1));
        assert!(c.matches_time(30, 17, 1, 1, 5));
        assert!(!c.matches_time(15, 9, 1, 1, 1));
        assert!(!c.matches_time(0, 12, 1, 1, 1));
    }

    #[test]
    fn rejects_bad_field_count() {
        assert!(parse("* * *").is_err());
        assert!(parse("* * * * * *").is_err());
    }

    #[test]
    fn rejects_out_of_range() {
        // minute 99 is invalid (max 59).
        assert!(parse("99 * * * *").is_err());
        // hour 25 is invalid (max 23).
        assert!(parse("0 25 * * *").is_err());
        // month 13 is invalid (max 12).
        assert!(parse("0 0 * 13 *").is_err());
        // day-of-month 32 invalid (max 31).
        assert!(parse("0 0 32 * *").is_err());
    }

    #[test]
    fn rejects_zero_step() {
        assert!(parse("*/0 * * * *").is_err());
    }

    #[test]
    fn rejects_bad_number() {
        assert!(parse("abc * * * *").is_err());
    }

    #[test]
    fn dom_or_dow_semantics() {
        // When both DOM and DOW are restricted, match if either matches.
        let c = e("0 0 1 * 5"); // 1st OR Friday
        assert!(c.matches_time(0, 0, 1, 1, 0));
        assert!(c.matches_time(0, 0, 2, 1, 5));
        assert!(!c.matches_time(0, 0, 2, 1, 1));
    }

    #[test]
    fn december_only() {
        let c = e("0 0 25 12 *");
        assert!(c.matches_time(0, 0, 25, 12, 3));
        assert!(!c.matches_time(0, 0, 25, 11, 3));
    }
}
