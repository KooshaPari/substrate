//! Minimal `vmstat` output parser.
//!
//! Parses the textual output of the Unix `vmstat` command. Supports both the
//! two-line form (header + memory/swap/io/system/cpu columns followed by rows)
//! and the single-line form produced by `vmstat 1` (a leading header followed
//! directly by numeric rows).
//!
//! The parser is deliberately tolerant: it skips blank lines, ignores the
//! human-readable header line, and produces one [`VmstatRow`] per numeric row.
//! Errors are returned only for structurally invalid input (a row with the
//! wrong number of fields, or non-numeric values in any field).
//!
//! Field meanings (matching `vmstat` / `proc(5)` / `sysstat` conventions):
//!
//! - `r`   — runnable processes (running or queued)
//! - `b`   — uninterruptible-sleep processes
//! - `swpd` — virtual memory used (KB)
//! - `free` — idle memory (KB)
//! - `buff` — memory used as buffers (KB)
//! - `cache` — memory used as cache (KB)
//! - `si` / `so` — swap-in / swap-out (KB/s)
//! - `bi` / `bo` — blocks received / blocks sent (per second)
//! - `us` / `sy` — user-time / system-time (% of CPU)
//! - `id` — idle CPU time (%)
//! - `wa` — I/O-wait CPU time (%)
//! - `st` — stolen CPU time (%)

/// A single row of parsed `vmstat` output.
///
/// All fields are `u64`; the parser does not produce negative values, which
/// `vmstat` itself does not emit on supported platforms.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VmstatRow {
    pub r: u64,
    pub b: u64,
    pub swpd: u64,
    pub free: u64,
    pub buff: u64,
    pub cache: u64,
    pub si: u64,
    pub so: u64,
    pub bi: u64,
    pub bo: u64,
    pub us: u64,
    pub sy: u64,
    pub id: u64,
    pub wa: u64,
    pub st: u64,
}

/// Parse the textual output of `vmstat` into a vector of [`VmstatRow`].
///
/// Blank lines and the human-readable header line are skipped. Every numeric
/// row must contain exactly 15 whitespace-separated fields; any deviation
/// (wrong count, non-numeric token) yields an error string that names the
/// offending line.
pub fn parse(input: &str) -> Result<Vec<VmstatRow>, String> {
    let mut rows = Vec::new();
    for (lineno, raw_line) in input.lines().enumerate() {
        let line = raw_line.trim();
        if line.is_empty() {
            continue;
        }
        // Skip the human-readable header line. Two-line `vmstat` output starts
        // with "procs ---" or similar; single-line `vmstat N` has a single
        // header line. We detect it by the presence of any non-numeric token
        // that is not the leading column name "procs".
        if is_header_line(line) {
            continue;
        }
        let cols: Vec<&str> = line.split_whitespace().collect();
        if cols.len() != 15 {
            return Err(format!(
                "line {}: expected 15 columns, got {} ({:?})",
                lineno + 1,
                cols.len(),
                line
            ));
        }
        let row = VmstatRow {
            r: parse_field(cols[0], lineno, "r")?,
            b: parse_field(cols[1], lineno, "b")?,
            swpd: parse_field(cols[2], lineno, "swpd")?,
            free: parse_field(cols[3], lineno, "free")?,
            buff: parse_field(cols[4], lineno, "buff")?,
            cache: parse_field(cols[5], lineno, "cache")?,
            si: parse_field(cols[6], lineno, "si")?,
            so: parse_field(cols[7], lineno, "so")?,
            bi: parse_field(cols[8], lineno, "bi")?,
            bo: parse_field(cols[9], lineno, "bo")?,
            us: parse_field(cols[10], lineno, "us")?,
            sy: parse_field(cols[11], lineno, "sy")?,
            id: parse_field(cols[12], lineno, "id")?,
            wa: parse_field(cols[13], lineno, "wa")?,
            st: parse_field(cols[14], lineno, "st")?,
        };
        rows.push(row);
    }
    Ok(rows)
}

/// Detect the human-readable header line. The two-line `vmstat` form begins
/// with `procs`; we treat any line that contains a non-numeric token AND does
/// not start with a digit as a header. Numeric rows always start with a digit.
fn is_header_line(line: &str) -> bool {
    let first = line.split_whitespace().next().unwrap_or("");
    if first.is_empty() || !first.chars().next().map_or(false, |c| c.is_ascii_digit()) {
        return true;
    }
    false
}

fn parse_field(token: &str, lineno: usize, name: &str) -> Result<u64, String> {
    token.parse::<u64>().map_err(|e| {
        format!(
            "line {}, field {} ({}): cannot parse {:?}: {}",
            lineno + 1,
            name,
            token,
            token,
            e
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_single_row() {
        // A minimal header followed by a single 15-column numeric row matching
        // the struct field layout (r b swpd free buff cache si so bi bo us sy id wa st).
        let input = "r  b swpd free buff cache si so bi bo us sy id wa st\n\
                     1  0    0 123456 78901 234567 0  0  0  0  5  2 92  1  0\n";
        let rows = parse(input).expect("parse should succeed");
        assert_eq!(rows.len(), 1);
        let r = rows[0];
        assert_eq!(r.r, 1);
        assert_eq!(r.b, 0);
        assert_eq!(r.swpd, 0);
        assert_eq!(r.free, 123456);
        assert_eq!(r.buff, 78901);
        assert_eq!(r.cache, 234567);
        assert_eq!(r.si, 0);
        assert_eq!(r.so, 0);
        assert_eq!(r.bi, 0);
        assert_eq!(r.bo, 0);
        assert_eq!(r.us, 5);
        assert_eq!(r.sy, 2);
        assert_eq!(r.id, 92);
        assert_eq!(r.wa, 1);
        assert_eq!(r.st, 0);
    }

    #[test]
    fn parse_multiple_rows_with_blank_lines() {
        let input = "\n\
                     r  b swpd free buff cache si so bi bo us sy id wa st\n\
                     \n\
                     0  0    0 100000 10000 200000 0  0  0  0  3  1 96  0  0\n\
                     1  0    0  99500  9990 200500 0  0  5 10  4  2 94  0  0\n\
                     \n";
        let rows = parse(input).expect("parse should succeed");
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].free, 100000);
        assert_eq!(rows[1].free, 99500);
        assert_eq!(rows[1].bi, 5);
        assert_eq!(rows[1].bo, 10);
    }

    #[test]
    fn parse_no_header() {
        // A raw 15-column row with no human-readable header.
        let input = "2 1 1024 8192 4096 16384 100 200 5 10 70 20 5 5 0\n";
        let rows = parse(input).expect("parse should succeed");
        assert_eq!(rows.len(), 1);
        let r = rows[0];
        assert_eq!(r.r, 2);
        assert_eq!(r.b, 1);
        assert_eq!(r.swpd, 1024);
        assert_eq!(r.free, 8192);
        assert_eq!(r.buff, 4096);
        assert_eq!(r.cache, 16384);
        assert_eq!(r.si, 100);
        assert_eq!(r.so, 200);
        assert_eq!(r.bi, 5);
        assert_eq!(r.bo, 10);
        assert_eq!(r.us, 70);
        assert_eq!(r.sy, 20);
        assert_eq!(r.id, 5);
        assert_eq!(r.wa, 5);
        assert_eq!(r.st, 0);
    }

    #[test]
    fn parse_empty_input() {
        let rows = parse("").expect("empty input should parse");
        assert!(rows.is_empty());
    }

    #[test]
    fn parse_only_blank_lines() {
        let rows = parse("\n\n   \n\n").expect("blank input should parse");
        assert!(rows.is_empty());
    }

    #[test]
    fn parse_wrong_column_count_errors() {
        let input = "1 2 3\n";
        let err = parse(input).expect_err("should reject 3-column row");
        assert!(err.contains("expected 15 columns"), "unexpected error: {}", err);
        assert!(err.contains("got 3"), "unexpected error: {}", err);
    }

    #[test]
    fn parse_non_numeric_field_errors() {
        let input = "1 0 0 0 0 0 0 0 0 0 0 0 92 0 abc\n";
        let err = parse(input).expect_err("should reject non-numeric");
        assert!(err.contains("cannot parse"), "unexpected error: {}", err);
        assert!(err.contains("abc"), "unexpected error: {}", err);
    }

    #[test]
    fn parse_large_values() {
        // Exercise a row with values that overflow smaller integer types.
        let input = "999999 999999 18446744073709551610 18446744073709551610 0 0 0 0 0 0 100 0 0 0 0\n";
        let rows = parse(input).expect("large values should parse as u64");
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].free, 18446744073709551610);
        assert_eq!(rows[0].swpd, 18446744073709551610);
    }

    #[test]
    fn parse_tolerates_extra_whitespace() {
        // Multiple spaces between columns should still split correctly.
        // Position mapping: r=0 b=1 swpd=2 free=3 buff=4 cache=5 si=6 so=7 bi=8 bo=9 us=10 sy=11 id=12 wa=13 st=14
        let input = "  1   0    0    0    0    0    0    0    0    0    0    0    92   0   0\n";
        let rows = parse(input).expect("extra whitespace should parse");
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].r, 1);
        assert_eq!(rows[0].id, 92);
    }

    #[test]
    fn parse_only_header() {
        // Pure header input should yield zero rows without error.
        let input = "r  b swpd free buff cache si so bi bo us sy id wa st\n";
        let rows = parse(input).expect("header-only should parse");
        assert!(rows.is_empty());
    }
}