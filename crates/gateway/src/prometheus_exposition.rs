//! Prometheus text exposition format encoder (v0.0.4).
//!
//! Reference: <https://prometheus.io/docs/instrumenting/exposition_formats/#text-based-format>
//!
//! Each metric is rendered as:
//! ```text
//! # HELP <name> <help text>
//! # TYPE <name> <counter|gauge|histogram>
//! <name>{label="value",...} <number> [timestamp_ms]
//! ```
//!
//! Histogram metrics emit cumulative buckets: `_bucket{le="..."} <count>` for each
//! boundary plus the `+Inf` overflow bucket, then `_sum <total>` and `_count <total>`.
//! An empty metric (no samples, or no non-NaN samples) is omitted entirely.

/// Type of a metric, mapping directly to the `# TYPE` line in the output.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MetricType {
    Counter,
    Gauge,
    Histogram,
}

impl MetricType {
    fn as_str(self) -> &'static str {
        match self {
            MetricType::Counter => "counter",
            MetricType::Gauge => "gauge",
            MetricType::Histogram => "histogram",
        }
    }
}

/// A single metric definition.
///
/// `samples` is a list of `(labels, value)` pairs. `labels` is a vector of
/// `(name, value)` tuples in declaration order. For Counter / Gauge each entry
/// produces one line. For Histogram, samples are interpreted as
/// `(bucket_boundary_or_inf, cumulative_count)` and rendered as `_bucket{le="..."}`.
#[derive(Debug, Clone)]
pub struct Metric {
    pub name: String,
    pub help: String,
    pub metric_type: MetricType,
    pub samples: Vec<(Vec<(String, String)>, f64)>,
}

/// Render a list of metrics into Prometheus text exposition format.
///
/// Metrics with no samples are omitted. Lines are joined by `\n` and the output
/// is terminated with a trailing newline so it is ready to serve from an HTTP
/// endpoint.
pub fn render(metrics: &[Metric]) -> String {
    let mut out = String::new();
    for metric in metrics {
        if metric.samples.is_empty() {
            continue;
        }
        if !metric.help.is_empty() {
            out.push_str("# HELP ");
            out.push_str(&metric.name);
            out.push(' ');
            out.push_str(&escape_help(&metric.help));
            out.push('\n');
        }
        out.push_str("# TYPE ");
        out.push_str(&metric.name);
        out.push(' ');
        out.push_str(metric.metric_type.as_str());
        out.push('\n');

        match metric.metric_type {
            MetricType::Counter | MetricType::Gauge => {
                for (labels, value) in &metric.samples {
                    if value.is_nan() {
                        continue;
                    }
                    out.push_str(&metric.name);
                    if !labels.is_empty() {
                        out.push('{');
                        out.push_str(&format_labels(labels));
                        out.push('}');
                    }
                    out.push(' ');
                    out.push_str(&format_value(*value));
                    out.push('\n');
                }
            }
            MetricType::Histogram => {
                for (labels, value) in &metric.samples {
                    // For histograms, each sample's "labels" vector carries the
                    // bucket-boundary string in the first slot (with empty name)
                    // and any user labels in the subsequent slots.
                    if value.is_nan() {
                        continue;
                    }
                    let (bucket_label, extra_labels) = split_histogram_label(labels);
                    out.push_str(&metric.name);
                    out.push_str("_bucket{");
                    let mut first = true;
                    if let Some(boundary) = bucket_label {
                        out.push_str("le=\"");
                        out.push_str(&escape_label(&boundary));
                        out.push('"');
                        first = false;
                    }
                    for (k, v) in extra_labels {
                        if !first {
                            out.push(',');
                        }
                        out.push_str(&escape_label(&k));
                        out.push_str("=\"");
                        out.push_str(&escape_label(&v));
                        out.push('"');
                        first = false;
                    }
                    if first {
                        out.push_str("le=\"+Inf\"");
                    }
                    out.push_str("} ");
                    out.push_str(&format_value(*value));
                    out.push('\n');
                }
                let sum: f64 = metric
                    .samples
                    .iter()
                    .filter_map(|(_, v)| if v.is_nan() { None } else { Some(*v) })
                    .sum();
                let count = metric.samples.len() as f64;
                out.push_str(&metric.name);
                out.push_str("_sum ");
                out.push_str(&format_value(sum));
                out.push('\n');
                out.push_str(&metric.name);
                out.push_str("_count ");
                out.push_str(&format_value(count));
                out.push('\n');
            }
        }
    }
    out
}

fn format_value(v: f64) -> String {
    if v.is_nan() {
        "NaN".to_string()
    } else if v.is_infinite() {
        if v > 0.0 {
            "+Inf".to_string()
        } else {
            "-Inf".to_string()
        }
    } else if v == v.trunc() && v.abs() < 1e16 {
        format!("{}", v as i64)
    } else {
        format!("{}", v)
    }
}

fn format_labels(labels: &[(String, String)]) -> String {
    let mut s = String::new();
    for (i, (k, v)) in labels.iter().enumerate() {
        if i > 0 {
            s.push(',');
        }
        s.push_str(&escape_label(k));
        s.push_str("=\"");
        s.push_str(&escape_label(v));
        s.push('"');
    }
    s
}

fn split_histogram_label(labels: &[(String, String)]) -> (Option<String>, Vec<(String, String)>) {
    if labels.is_empty() {
        return (None, Vec::new());
    }
    if labels[0].0.is_empty() {
        (Some(labels[0].1.clone()), labels[1..].to_vec())
    } else {
        (None, labels.to_vec())
    }
}

fn escape_label(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '\\' => out.push_str(r"\\"),
            '"' => out.push_str(r#"\""#),
            '\n' => out.push_str(r"\n"),
            _ => out.push(c),
        }
    }
    out
}

fn escape_help(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '\\' => out.push_str(r"\\"),
            '\n' => out.push_str(r"\n"),
            _ => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counter_with_labels() {
        let m = Metric {
            name: "http_requests_total".to_string(),
            help: "Total HTTP requests".to_string(),
            metric_type: MetricType::Counter,
            samples: vec![(
                vec![
                    ("method".to_string(), "GET".to_string()),
                    ("status".to_string(), "200".to_string()),
                ],
                42.0,
            )],
        };
        let out = render(&[m]);
        let expected = r##"# HELP http_requests_total Total HTTP requests
# TYPE http_requests_total counter
http_requests_total{method="GET",status="200"} 42
"##;
        assert_eq!(out, expected);
    }

    #[test]
    fn gauge_single_value() {
        let m = Metric {
            name: "memory_bytes".to_string(),
            help: "Resident memory in bytes".to_string(),
            metric_type: MetricType::Gauge,
            samples: vec![(vec![], 1024.0)],
        };
        let out = render(&[m]);
        let expected = r##"# HELP memory_bytes Resident memory in bytes
# TYPE memory_bytes gauge
memory_bytes 1024
"##;
        assert_eq!(out, expected);
    }

    #[test]
    fn histogram_emits_buckets_sum_count() {
        let m = Metric {
            name: "request_duration_seconds".to_string(),
            help: "Request duration".to_string(),
            metric_type: MetricType::Histogram,
            samples: vec![
                (vec![(String::new(), "0.005".to_string())], 2.0),
                (vec![(String::new(), "0.01".to_string())], 5.0),
                (vec![(String::new(), "0.025".to_string())], 7.0),
                (vec![(String::new(), "+Inf".to_string())], 10.0),
            ],
        };
        let out = render(&[m]);
        assert!(out.contains(r##"# TYPE request_duration_seconds histogram"##));
        assert!(out.contains(r##"request_duration_seconds_bucket{le="0.005"} 2"##));
        assert!(out.contains(r##"request_duration_seconds_bucket{le="0.01"} 5"##));
        assert!(out.contains(r##"request_duration_seconds_bucket{le="0.025"} 7"##));
        assert!(out.contains(r##"request_duration_seconds_bucket{le="+Inf"} 10"##));
        assert!(out.contains("request_duration_seconds_sum 24"));
        assert!(out.contains("request_duration_seconds_count 4"));
    }

    #[test]
    fn empty_metric_is_omitted() {
        let m = Metric {
            name: "unused".to_string(),
            help: "Nothing here".to_string(),
            metric_type: MetricType::Counter,
            samples: vec![],
        };
        let out = render(&[m]);
        assert_eq!(out, "");
    }

    #[test]
    fn label_escaping_handles_backslash_and_quote() {
        let m = Metric {
            name: "x_total".to_string(),
            help: "".to_string(),
            metric_type: MetricType::Counter,
            samples: vec![(vec![("path".to_string(), r#"a\b"c"#.to_string())], 1.0)],
        };
        let out = render(&[m]);
        assert!(out.contains(r##"x_total{path="a\\b\"c"} 1"##));
    }
}
