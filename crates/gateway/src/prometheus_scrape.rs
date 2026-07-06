<<<<<<< HEAD
//! Prometheus text exposition format parser (v0.0.4).
//!
//! Reference: <https://prometheus.io/docs/instrumenting/exposition_formats/#text-based-format>
//!
//! Parses a Prometheus text-format document into a flat list of samples. Each
//! sample is a 4-tuple:
//!
//! ```text
//! (metric_name, metric_kind, labels, value)
//! ```
//!
//! `metric_kind` is derived from the most recent `# TYPE <name> <kind>` line
//! preceding the sample. When a sample appears before any type directive (or
//! outside the scope of any previously seen `# TYPE` for its name), it defaults
//! to [`MetricKind::Gauge`] which is the implicit type for untyped samples per
//! the Prometheus spec.
//!
//! HELP/TYPE lines themselves do not produce samples; only data lines do.

use std::collections::HashMap;

/// Metric kind as encoded in `# TYPE <name> <kind>` directives.
///
/// The Prometheus text format spells these lowercase (`counter`, `gauge`,
/// `histogram`); this enum is the canonical Rust mapping used by [`parse_exposition`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MetricKind {
    Counter,
    Gauge,
    Histogram,
}

/// Parse a Prometheus text exposition document.
///
/// Returns one entry per sample line in the order encountered. Comments and
/// blank lines do not produce entries. Labels are returned as a list of
/// `(name, value)` pairs preserving document order.
///
/// # Errors
///
/// Returns `Err` with a human-readable message on:
/// * bad numeric value
/// * malformed label braces
/// * unknown `# TYPE` kind
/// * unterminated escape sequences in label values
///
/// Unknown `# TYPE` kinds are reported as an error rather than silently ignored.
pub fn parse_exposition(
    input: &str,
) -> Result<Vec<(String, MetricKind, Vec<(String, String)>, f64)>, String> {
    let mut out: Vec<(String, MetricKind, Vec<(String, String)>, f64)> = Vec::new();
    let mut kind_for_name: HashMap<String, MetricKind> = HashMap::new();
    let mut default_kind = MetricKind::Gauge;

    for (line_no, raw_line) in input.lines().enumerate() {
        let line = raw_line.trim();
        if line.is_empty() {
            continue;
        }

        if let Some(rest) = line.strip_prefix("# HELP ") {
            // HELP <name> <text> — record help but do not emit a sample.
            // We do not currently surface help text in the parsed result; we
            // simply consume the line so the parser advances.
            let _ = split_metric_name(rest).ok_or_else(|| {
                format!("line {}: malformed # HELP directive", line_no + 1)
            })?;
            continue;
        }

        if let Some(rest) = line.strip_prefix("# TYPE ") {
            let (name, kind_str) = split_metric_name(rest)
                .ok_or_else(|| format!("line {}: malformed # TYPE directive", line_no + 1))?;
            let kind = parse_kind(kind_str)
                .ok_or_else(|| format!("line {}: unknown metric kind `{}`", line_no + 1, kind_str))?;
            default_kind = kind;
            kind_for_name.insert(name.to_string(), kind);
            continue;
        }

        // Skip other comments (e.g. blank `#`, `# EOF`) without erroring.
        if line.starts_with('#') {
            continue;
        }

        let (metric_name, labels, value) = parse_sample_line(line, line_no + 1)?;
        let kind = kind_for_name
            .get(metric_name)
            .copied()
            .unwrap_or(default_kind);
        out.push((metric_name.to_string(), kind, labels, value));
    }

    Ok(out)
}

fn parse_kind(s: &str) -> Option<MetricKind> {
    match s.trim() {
        "counter" => Some(MetricKind::Counter),
        "gauge" => Some(MetricKind::Gauge),
        "histogram" => Some(MetricKind::Histogram),
        _ => None,
    }
}

/// Split `rest` (the part after `# HELP ` or `# TYPE `) into `(name, tail)`.
/// The metric name must be a valid Prometheus identifier `[A-Za-z_][A-Za-z0-9_:]*`.
fn split_metric_name(rest: &str) -> Option<(&str, &str)> {
    let trimmed = rest.trim_start();
    let end = trimmed
        .find(|c: char| c.is_whitespace())
        .unwrap_or(trimmed.len());
    let name = &trimmed[..end];
    if name.is_empty() {
        return None;
    }
    let mut chars = name.chars();
    let first = chars.next()?;
    if !(first.is_ascii_alphabetic() || first == '_') {
        return None;
    }
    if !chars.all(|c| c.is_ascii_alphanumeric() || c == '_' || c == ':') {
        return None;
    }
    Some((name, trimmed[end..].trim()))
}

/// Parse a non-comment data line into `(metric_name, labels, value)`.
fn parse_sample_line(
    line: &str,
    line_no: usize,
) -> Result<(&str, Vec<(String, String)>, f64), String> {
    // Split the line into the head (up to and including the metric name,
    // optionally followed by `{...}`) and the value tail.
    let (name, labels_inner, value_str) = if let Some(open) = line.find('{') {
        let (name_part, after_open) = line.split_at(open);
        let name_part = name_part.trim_end();
        let (labels_str, after_brace) = split_around_brace(after_open, line_no)?;
        let labels = parse_labels(&labels_str, line_no)?;
        let value_str = after_brace.trim_start();
        (name_part, labels, value_str)
    } else {
        // No labels — find the first whitespace separating name from value.
        let split_at = line
            .find(|c: char| c.is_whitespace())
            .ok_or_else(|| format!("line {line_no}: missing value separator"))?;
        let (name_part, rest) = line.split_at(split_at);
        let name_part = name_part.trim_end();
        let value_str = rest.trim_start();
        if name_part.is_empty() {
            return Err(format!("line {line_no}: missing metric name"));
        }
        (name_part, Vec::new(), value_str)
    };

    let value = parse_value(value_str)
        .ok_or_else(|| format!("line {line_no}: invalid numeric value `{value_str}`"))?;
    Ok((name, labels_inner, value))
}

/// Split a string starting with `{` into `(label_inner, after_brace)`.
fn split_around_brace(s: &str, line_no: usize) -> Result<(String, &str), String> {
    debug_assert!(s.starts_with('{'));
    let bytes = s.as_bytes();
    let mut depth = 0i32;
    let mut in_str = false;
    let mut escape = false;
    for (i, &b) in bytes.iter().enumerate() {
        if escape {
            escape = false;
            continue;
        }
        match b {
            b'\\' if in_str => escape = true,
            b'"' => in_str = !in_str,
            b'{' if !in_str => depth += 1,
            b'}' if !in_str => {
                depth -= 1;
                if depth == 0 {
                    let inner = s[1..i].to_string();
                    let after = &s[i + 1..];
                    return Ok((inner, after));
                }
=======
// Prometheus text exposition format parser.
#[derive(Debug, PartialEq, Clone)]
pub enum MetricKind { Counter, Gauge, Histogram, Summary, Untyped }

#[derive(Debug, PartialEq, Clone)]
pub struct Sample {
    pub labels: Vec<(String, String)>,
    pub value: f64,
}

#[derive(Debug, PartialEq, Clone)]
pub struct Metric {
    pub name: String,
    pub help: Option<String>,
    pub kind: MetricKind,
    pub samples: Vec<Sample>,
}

pub fn parse(input: &str) -> Result<Vec<Metric>, String> {
    let mut out: Vec<Metric> = Vec::new();
    let mut current: Option<Metric> = None;
    for raw_line in input.lines() {
        let line = raw_line.trim();
        if line.is_empty() { continue; }
        if let Some(rest) = line.strip_prefix("# HELP ") {
            let mut parts = rest.splitn(2, char::is_whitespace);
            let name = parts.next().ok_or("malformed HELP")?.to_string();
            let help = parts.next().unwrap_or("").to_string();
            if let Some(m) = current.as_mut() {
                if m.name == name { m.help = Some(help); continue; }
            }
            out.push(Metric { name, help: Some(help), kind: MetricKind::Untyped, samples: vec![] });
            current = None;
            continue;
        }
        if let Some(rest) = line.strip_prefix("# TYPE ") {
            let mut parts = rest.splitn(2, char::is_whitespace);
            let name = parts.next().ok_or("malformed TYPE")?.to_string();
            let kind_str = parts.next().ok_or("missing type value")?;
            let kind = match kind_str {
                "counter" => MetricKind::Counter,
                "gauge" => MetricKind::Gauge,
                "histogram" => MetricKind::Histogram,
                "summary" => MetricKind::Summary,
                "untyped" => MetricKind::Untyped,
                other => return Err(format!("unknown metric type: {}", other)),
            };
            // attach to existing metric with same name, or create new
            if let Some(m) = out.iter_mut().find(|m| m.name == name) {
                m.kind = kind.clone();
            } else {
                out.push(Metric { name: name.clone(), help: None, kind: kind.clone(), samples: vec![] });
            }
            current = None;
            continue;
        }
        if line.starts_with('#') { continue; }
        // sample line: name{labels} value [timestamp]
        let sample = parse_sample_line(line)?;
        let name = sample.0.clone();
        // ensure metric exists (auto-create as Untyped if not declared via HELP/TYPE)
        if out.iter().all(|m| m.name != name) {
            out.push(Metric { name: name.clone(), help: None, kind: MetricKind::Untyped, samples: vec![] });
        }
        if let Some(m) = out.iter_mut().find(|m| m.name == name) {
            m.samples.push(Sample { labels: sample.1, value: sample.2 });
        }
    }
    if let Some(c) = current { out.push(c); }
    Ok(out)
}
fn parse_sample_line(line: &str) -> Result<(String, Vec<(String, String)>, f64), String> {
    let (head, value_str) = line.split_once(' ').ok_or("missing space in sample")?;
    let value: f64 = value_str.trim().parse().map_err(|e| format!("bad value: {}", e))?;
    if let Some((name, label_str)) = head.split_once('{') {
        if !label_str.ends_with('}') { return Err("unclosed label".into()); }
        let inner = &label_str[..label_str.len()-1];
        let labels = parse_labels(inner);
        Ok((name.to_string(), labels, value))
    } else {
        Ok((head.to_string(), vec![], value))
    }
}
fn parse_labels(s: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut depth = 0;
    let mut start = 0;
    let bytes = s.as_bytes();
    for (i, &b) in bytes.iter().enumerate() {
        match b {
            b'"' => depth = 1 - depth,
            b',' if depth == 0 => {
                if let Some(pair) = parse_label(&s[start..i]) {
                    out.push(pair);
                }
                start = i + 1;
>>>>>>> 01c0243 (feat(gateway): Prometheus text exposition parser (counter/gauge/histogram/summary))
            }
            _ => {}
        }
    }
<<<<<<< HEAD
    Err(format!("line {line_no}: unbalanced brace in label set"))
}

/// Parse `key1="v1",key2="v2"` into a label vector.
fn parse_labels(inner: &str, line_no: usize) -> Result<Vec<(String, String)>, String> {
    let mut out = Vec::new();
    if inner.trim().is_empty() {
        return Ok(out);
    }
    let bytes = inner.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        // skip leading whitespace and commas between labels
        while i < bytes.len() && (bytes[i] == b',' || bytes[i].is_ascii_whitespace()) {
            i += 1;
        }
        if i >= bytes.len() {
            break;
        }
        // key runs until '='
        let key_start = i;
        while i < bytes.len() && bytes[i] != b'=' {
            i += 1;
        }
        if i >= bytes.len() {
            return Err(format!("line {line_no}: label missing `=`"));
        }
        let key = inner[key_start..i].trim().to_string();
        i += 1; // consume '='
        // expect opening quote
        if i >= bytes.len() || bytes[i] != b'"' {
            return Err(format!("line {line_no}: label value must be quoted"));
        }
        i += 1;
        let value_start = i;
        let mut value = String::new();
        let mut escape = false;
        let mut closed = false;
        while i < bytes.len() {
            let b = bytes[i];
            if escape {
                match b {
                    b'"' => value.push('"'),
                    b'\\' => value.push('\\'),
                    b'n' => value.push('\n'),
                    _ => {
                        return Err(format!(
                            "line {line_no}: unknown label escape `\\{}`",
                            b as char
                        ))
                    }
                }
                escape = false;
                i += 1;
                continue;
            }
            match b {
                b'\\' => {
                    escape = true;
                    i += 1;
                }
                b'"' => {
                    closed = true;
                    i += 1;
                    break;
                }
                _ => {
                    value.push(b as char);
                    i += 1;
                }
            }
        }
        if !closed {
            return Err(format!("line {line_no}: unterminated label value"));
        }
        let _ = value_start;
        out.push((key, value));
    }
    Ok(out)
}

fn parse_value(s: &str) -> Option<f64> {
    let s = s.trim();
    if s.is_empty() {
        return None;
    }
    match s {
        "NaN" | "+NaN" | "-NaN" => return Some(f64::NAN),
        "+Inf" | "Inf" => return Some(f64::INFINITY),
        "-Inf" => return Some(f64::NEG_INFINITY),
        _ => {}
    }
    s.parse::<f64>().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_counter_with_labels() {
        let text = r##"# HELP http_requests_total Total HTTP requests
# TYPE http_requests_total counter
http_requests_total{method="GET",status="200"} 42
http_requests_total{method="POST",status="500"} 7
"##;
        let out = parse_exposition(text).unwrap();
        assert_eq!(out.len(), 2);
        let (name, kind, labels, val) = &out[0];
        assert_eq!(name, "http_requests_total");
        assert_eq!(*kind, MetricKind::Counter);
        assert_eq!(
            labels,
            &vec![
                ("method".to_string(), "GET".to_string()),
                ("status".to_string(), "200".to_string()),
            ]
        );
        assert_eq!(*val, 42.0);
        let (name2, kind2, labels2, val2) = &out[1];
        assert_eq!(name2, "http_requests_total");
        assert_eq!(*kind2, MetricKind::Counter);
        assert_eq!(
            labels2,
            &vec![
                ("method".to_string(), "POST".to_string()),
                ("status".to_string(), "500".to_string()),
            ]
        );
        assert_eq!(*val2, 7.0);
    }

    #[test]
    fn parse_gauge_no_labels() {
        let text = r##"# TYPE memory_bytes gauge
memory_bytes 1024
"##;
        let out = parse_exposition(text).unwrap();
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].0, "memory_bytes");
        assert_eq!(out[0].1, MetricKind::Gauge);
        assert!(out[0].2.is_empty());
        assert_eq!(out[0].3, 1024.0);
    }

    #[test]
    fn parse_histogram_buckets() {
        let text = r##"# TYPE request_duration_seconds histogram
request_duration_seconds_bucket{le="0.005"} 2
request_duration_seconds_bucket{le="0.01"} 5
request_duration_seconds_bucket{le="+Inf"} 10
request_duration_seconds_sum 24
request_duration_seconds_count 10
"##;
        let out = parse_exposition(text).unwrap();
        assert_eq!(out.len(), 5);
        for entry in &out {
            assert_eq!(entry.1, MetricKind::Histogram);
        }
        assert_eq!(out[2].0, "request_duration_seconds_bucket");
        assert_eq!(out[2].2, vec![("le".to_string(), "+Inf".to_string())]);
        assert_eq!(out[2].3, 10.0);
        assert_eq!(out[3].3, 24.0);
        assert_eq!(out[4].3, 10.0);
    }

    #[test]
    fn unknown_type_kind_is_error() {
        let text = "# TYPE foo summary\nfoo 1\n";
        assert!(parse_exposition(text).is_err());
    }

    #[test]
    fn blank_lines_and_comments_are_skipped() {
        let text = "# this is a free-form comment\n\n# TYPE x gauge\nx 1\n";
        let out = parse_exposition(text).unwrap();
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].0, "x");
    }

    #[test]
    fn bad_numeric_value_errors() {
        let text = "foo_total not_a_number\n";
        assert!(parse_exposition(text).is_err());
    }

    #[test]
    fn default_kind_for_sample_before_type_is_gauge() {
        let text = "mystery_metric 3.14\n";
        let out = parse_exposition(text).unwrap();
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].1, MetricKind::Gauge);
        assert_eq!(out[0].3, 3.14);
    }

    #[test]
    fn label_value_with_escaped_quote() {
        let text = r##"# TYPE x_total counter
x_total{path="a\"b"} 1
"##;
        let out = parse_exposition(text).unwrap();
        assert_eq!(out[0].2, vec![("path".to_string(), "a\"b".to_string())]);
    }
}
=======
    if let Some(pair) = parse_label(&s[start..]) {
        out.push(pair);
    }
    out
}
fn parse_label(s: &str) -> Option<(String, String)> {
    let mut parts = s.splitn(2, '=');
    let k = parts.next()?.trim().to_string();
    let v = parts.next()?.trim();
    let v = v.trim_matches('"').to_string();
    Some((k, v))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn parse_counter() {
        let s = "# HELP http_requests_total Total\n# TYPE http_requests_total counter\nhttp_requests_total{method=\"GET\",code=\"200\"} 42\nhttp_requests_total{method=\"POST\",code=\"500\"} 1\n";
        let m = parse(s).unwrap();
        assert_eq!(m.len(), 1);
        assert_eq!(m[0].name, "http_requests_total");
        assert_eq!(m[0].kind, MetricKind::Counter);
        assert_eq!(m[0].help, Some("Total".into()));
        assert_eq!(m[0].samples.len(), 2);
        assert_eq!(m[0].samples[0].value, 42.0);
        assert_eq!(m[0].samples[0].labels[0], ("method".into(), "GET".into()));
    }
    #[test] fn parse_gauge_no_labels() {
        let s = "# TYPE mem_bytes gauge\nmem_bytes 1024\n";
        let m = parse(s).unwrap();
        assert_eq!(m.len(), 1);
        assert_eq!(m[0].kind, MetricKind::Gauge);
        assert_eq!(m[0].samples[0].value, 1024.0);
        assert!(m[0].samples[0].labels.is_empty());
    }
    #[test] fn parse_histogram_buckets() {
        // Real prometheus emits buckets/_sum/_count as sibling metrics with the
        // same base name; TYPE only appears once for the base metric.
        let s = "# TYPE latency histogram\nlatency_bucket{le=\"0.5\"} 5\nlatency_bucket{le=\"1\"} 10\nlatency_sum 100\nlatency_count 12\n";
        let m = parse(s).unwrap();
        assert_eq!(m.len(), 4);
        let bucket_count = m.iter().filter(|x| x.name == "latency_bucket").count();
        assert_eq!(bucket_count, 1);
        assert_eq!(m.iter().find(|x| x.name == "latency_bucket").unwrap().samples.len(), 2);
    }
    #[test] fn parse_multiple_metrics() {
        let s = "# TYPE a counter\n# TYPE b gauge\na 1\nb 2\n";
        let m = parse(s).unwrap();
        assert_eq!(m.len(), 2);
        assert_eq!(m[0].name, "a");
        assert_eq!(m[1].name, "b");
    }
    #[test] fn parse_empty() {
        let m = parse("").unwrap();
        assert!(m.is_empty());
    }
    #[test] fn parse_comments_only() {
        let m = parse("# this is a comment\n# another comment\n").unwrap();
        assert!(m.is_empty());
    }
    #[test] fn parse_unknown_type_errors() {
        let s = "# TYPE x weird\nx 1\n";
        assert!(parse(s).is_err());
    }
    #[test] fn parse_label_with_quote() {
        let s = "# TYPE x gauge\nx{label=\"a,b\"} 5\n";
        let m = parse(s).unwrap();
        assert_eq!(m[0].samples[0].labels[0].1, "a,b");
    }
}
>>>>>>> 01c0243 (feat(gateway): Prometheus text exposition parser (counter/gauge/histogram/summary))
