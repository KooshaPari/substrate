//! Gregorian calendar date arithmetic.
//!
//! [`Date`] is a simple proleptic Gregorian calendar date with
//! year/month/day fields. Supports construction from year-month-day,
//! comparison, day-of-week computation, and day-of-year.
//!
//! Does NOT support time-of-day, time zones, or non-Gregorian
//! calendars. Uses the proleptic Gregorian rule (year 1 = 1 CE).
//!
//! Reference: N. Dershowitz & E. Reingold, *Calendrical Calculations*.

/// A proleptic Gregorian date.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Date {
    pub year: i32,
    pub month: u8, // 1..=12
    pub day: u8,   // 1..=31
}

impl Date {
    /// Build a date from year/month/day. Returns `Err` if the date is
    /// not a valid Gregorian date.
    pub fn new(year: i32, month: u8, day: u8) -> Result<Self, String> {
        if !(1..=12).contains(&month) {
            return Err(format!("month {} out of range 1..=12", month));
        }
        let dim = days_in_month(year, month);
        if day < 1 || day > dim {
            return Err(format!("day {} out of range 1..={}", day, dim));
        }
        Ok(Self { year, month, day })
    }

    /// Day of year (1-indexed). Jan 1 = 1.
    pub fn day_of_year(&self) -> u32 {
        let mut doy = self.day as u32;
        for m in 1..self.month {
            doy += days_in_month(self.year, m) as u32;
        }
        doy
    }

    /// Day of week as 0=Monday .. 6=Sunday. Uses Zeller's congruence for
    /// the Gregorian calendar.
    pub fn day_of_week(&self) -> u8 {
        let mut y = self.year;
        let mut m = self.month as i32;
        if m < 3 {
            y -= 1;
            m += 12;
        }
        let k = self.day as i32;
        let h = (k + (13 * (m + 1)) / 5 + y + y / 4 - y / 100 + y / 400) % 7;
        // h: 0=Saturday, 1=Sunday, 2=Monday, ..., 6=Friday
        // remap to 0=Monday .. 6=Sunday
        ((h + 5) % 7) as u8
    }
}

fn is_leap_year(year: i32) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

fn days_in_month(year: i32, month: u8) -> u8 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 => {
            if is_leap_year(year) {
                29
            } else {
                28
            }
        }
        _ => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn construct_valid_dates() {
        assert!(Date::new(2024, 1, 1).is_ok());
        assert!(Date::new(2024, 2, 29).is_ok()); // leap year
        assert!(Date::new(2023, 2, 29).is_err()); // not leap year
    }

    #[test]
    fn leap_year_detection() {
        assert!(is_leap_year(2000)); // divisible by 400
        assert!(is_leap_year(2024)); // divisible by 4 not 100
        assert!(!is_leap_year(1900)); // divisible by 100 not 400
        assert!(!is_leap_year(2023)); // not divisible by 4
    }

    #[test]
    fn days_in_month_correctness() {
        assert_eq!(days_in_month(2024, 1), 31);
        assert_eq!(days_in_month(2024, 2), 29); // leap
        assert_eq!(days_in_month(2023, 2), 28);
        assert_eq!(days_in_month(2024, 4), 30);
    }

    #[test]
    fn day_of_year_basic() {
        assert_eq!(Date::new(2024, 1, 1).unwrap().day_of_year(), 1);
        assert_eq!(Date::new(2024, 12, 31).unwrap().day_of_year(), 366); // leap
        assert_eq!(Date::new(2023, 3, 1).unwrap().day_of_year(), 60); // 31+28+1
    }

    #[test]
    fn day_of_week_known_dates() {
        // 2024-01-01 was Monday
        assert_eq!(Date::new(2024, 1, 1).unwrap().day_of_week(), 0);
        // 2024-12-25 was Wednesday
        assert_eq!(Date::new(2024, 12, 25).unwrap().day_of_week(), 2);
        // 2000-01-01 was Saturday
        assert_eq!(Date::new(2000, 1, 1).unwrap().day_of_week(), 5);
    }

    #[test]
    fn invalid_month_rejected() {
        assert!(Date::new(2024, 0, 1).is_err());
        assert!(Date::new(2024, 13, 1).is_err());
    }

    #[test]
    fn invalid_day_for_month_rejected() {
        assert!(Date::new(2024, 4, 31).is_err()); // April has 30
        assert!(Date::new(2023, 2, 29).is_err()); // not leap
    }

    #[test]
    fn equality_and_hash() {
        let d1 = Date::new(2024, 1, 1).unwrap();
        let d2 = Date::new(2024, 1, 1).unwrap();
        assert_eq!(d1, d2);
        assert_eq!(d1.year, d2.year);
    }
}
