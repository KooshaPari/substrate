//! Fuzz target: `psub_gateway::cron_parser::parse`.
//!
//! Cron parsing has unbounded ranges per field (0-59 for minute,
//! 0-23 for hour, 1-31 for day-of-month, 1-12 for month, 0-6 for dow).
//! A naive parser can mis-handle `*/` or `-`/`/` combinations. This
//! fuzz target never panics — it either returns Ok or an Err String.

#![no_main]
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    // Limit to printable ASCII + tabs to keep the parser honest.
    if let Ok(s) = std::str::from_utf8(data) {
        let _ = psub_gateway::cron_parser::parse(s);
    }
});
