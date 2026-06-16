#![forbid(unsafe_code)]
#![warn(missing_docs)]

//! Cron/interval/daily/weekly scheduling via [`SchedulePort`].

use chrono::{DateTime, Datelike, Duration, Utc, Weekday as ChronoWeekday};
use croner::Cron;
use substrate_core::error::{Result, SubstrateError};
use substrate_core::schedule_port::{ScheduleInstant, SchedulePort, ScheduleTrigger, Weekday};

/// [`SchedulePort`] backed by the `croner` crate for cron expressions.
#[derive(Debug, Default, Clone, Copy)]
pub struct CronSchedule;

impl CronSchedule {
    /// Create a new scheduler.
    pub fn new() -> Self {
        Self
    }
}

fn to_instant(dt: DateTime<Utc>) -> ScheduleInstant {
    ScheduleInstant {
        secs: dt.timestamp(),
    }
}

fn from_instant(inst: ScheduleInstant) -> DateTime<Utc> {
    DateTime::from_timestamp(inst.secs, 0).unwrap_or_else(Utc::now)
}

fn weekday_to_chrono(w: Weekday) -> ChronoWeekday {
    match w {
        Weekday::Sun => ChronoWeekday::Sun,
        Weekday::Mon => ChronoWeekday::Mon,
        Weekday::Tue => ChronoWeekday::Tue,
        Weekday::Wed => ChronoWeekday::Wed,
        Weekday::Thu => ChronoWeekday::Thu,
        Weekday::Fri => ChronoWeekday::Fri,
        Weekday::Sat => ChronoWeekday::Sat,
    }
}

fn validate_hm(hour: u8, minute: u8) -> Result<()> {
    if hour > 23 || minute > 59 {
        return Err(SubstrateError::InvalidSchedule(format!(
            "hour/minute out of range: {hour}:{minute}"
        )));
    }
    Ok(())
}

fn next_daily(after: DateTime<Utc>, hour: u8, minute: u8) -> Result<ScheduleInstant> {
    validate_hm(hour, minute)?;
    let mut candidate = after
        .date_naive()
        .and_hms_opt(hour.into(), minute.into(), 0)
        .unwrap()
        .and_utc();
    if candidate <= after {
        candidate += Duration::days(1);
    }
    Ok(to_instant(candidate))
}

fn next_weekly(
    after: DateTime<Utc>,
    weekday: Weekday,
    hour: u8,
    minute: u8,
) -> Result<ScheduleInstant> {
    validate_hm(hour, minute)?;
    let target = weekday_to_chrono(weekday);
    let mut candidate = after
        .date_naive()
        .and_hms_opt(hour.into(), minute.into(), 0)
        .unwrap()
        .and_utc();
    for _ in 0..8 {
        if candidate.weekday() == target && candidate > after {
            return Ok(to_instant(candidate));
        }
        candidate += Duration::days(1);
    }
    Err(SubstrateError::InvalidSchedule(
        "could not compute next weekly run".into(),
    ))
}

impl SchedulePort for CronSchedule {
    fn next_run(&self, trigger: &ScheduleTrigger, after: ScheduleInstant) -> Result<ScheduleInstant> {
        let after_dt = from_instant(after);
        match trigger {
            ScheduleTrigger::Cron { expr } => {
                let cron = Cron::new(expr)
                    .with_seconds_optional()
                    .parse()
                    .map_err(|e| SubstrateError::InvalidSchedule(format!("cron parse: {e}")))?;
                let next = cron
                    .find_next_occurrence(&after_dt, false)
                    .map_err(|e| SubstrateError::InvalidSchedule(format!("cron next: {e}")))?;
                Ok(to_instant(next))
            }
            ScheduleTrigger::Interval { every_secs } => {
                if *every_secs == 0 {
                    return Err(SubstrateError::InvalidSchedule(
                        "interval every_secs must be > 0".into(),
                    ));
                }
                let next = after_dt + Duration::seconds(*every_secs as i64);
                Ok(to_instant(next))
            }
            ScheduleTrigger::Daily { hour, minute } => next_daily(after_dt, *hour, *minute),
            ScheduleTrigger::Weekly {
                weekday,
                hour,
                minute,
            } => next_weekly(after_dt, *weekday, *hour, *minute),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Timelike};

    fn sched() -> CronSchedule {
        CronSchedule::new()
    }

    fn at(y: i32, m: u32, d: u32, h: u32, min: u32) -> ScheduleInstant {
        to_instant(
            Utc.with_ymd_and_hms(y, m, d, h, min, 0)
                .single()
                .unwrap(),
        )
    }

    #[test]
    fn cron_next_run_hourly() {
        let s = sched();
        let after = at(2026, 6, 15, 10, 30);
        let next = s
            .next_run(
                &ScheduleTrigger::Cron {
                    expr: "0 * * * *".into(),
                },
                after,
            )
            .unwrap();
        assert_eq!(from_instant(next).minute(), 0);
        assert!(from_instant(next) > from_instant(after));
    }

    #[test]
    fn interval_next_run() {
        let s = sched();
        let after = at(2026, 6, 15, 10, 0);
        let next = s
            .next_run(
                &ScheduleTrigger::Interval {
                    every_secs: 300,
                },
                after,
            )
            .unwrap();
        assert_eq!(next.secs - after.secs, 300);
    }

    #[test]
    fn daily_next_run() {
        let s = sched();
        let after = at(2026, 6, 15, 8, 0);
        let next = s
            .next_run(
                &ScheduleTrigger::Daily {
                    hour: 9,
                    minute: 0,
                },
                after,
            )
            .unwrap();
        let dt = from_instant(next);
        assert_eq!(dt.hour(), 9);
        assert_eq!(dt.day(), 15);
    }

    #[test]
    fn weekly_next_run() {
        let s = sched();
        // 2026-06-15 is a Monday
        let after = at(2026, 6, 15, 10, 0);
        let next = s
            .next_run(
                &ScheduleTrigger::Weekly {
                    weekday: Weekday::Wed,
                    hour: 9,
                    minute: 0,
                },
                after,
            )
            .unwrap();
        let dt = from_instant(next);
        assert_eq!(dt.weekday(), ChronoWeekday::Wed);
        assert_eq!(dt.hour(), 9);
    }
}
