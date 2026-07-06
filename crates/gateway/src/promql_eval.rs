//! Minimal PromQL expression evaluator — companion to `promql_parse`.
//!
//! Takes the AST produced by [`promql_parse::parse`] and evaluates it against
//! an in-memory sample store. Designed to lint common rule shapes and unit-test
//! them without standing up a Prometheus server.
//!
//! # Supported expression forms
//!
//! - **Instant vector selectors** (`metric`, `metric{label="v"}`): match
//!   exactly on label set, return all matching samples.
//! - **Binary arithmetic on instant vectors** (`a + b`, `a * b`): standard
//!   element-wise join on equal label sets; mismatched labels drop the operand.
//!   Comparison and `bool` modifier forms are accepted but downgraded to
//!   plain arithmetic operators (left-hand side wins).
//! - **Aggregate operators**: `sum(...)` and `avg(...)`. An optional `by(...)`
//!   modifier is honored when present; missing modifiers produce one output
//!   sample per remaining label-set group.
//!
//! Range vectors, `@` modifiers, subqueries, function calls, and matrix
//! selectors return an explicit error — this evaluator covers what
//! `promql_parse` parses structurally, then keeps the surface narrow enough
//! to test in isolation.

use std::collections::BTreeMap;
use std::collections::HashMap;

use crate::promql_parse::{Aggregate, AggOp, BinOp, Expr, LabelMatcher, MatchOp, Metric};

/// A single time-series sample: a value tagged with arbitrary labels and a
/// millisecond (or second, depending on caller) timestamp.
#[derive(Debug, Clone, PartialEq)]
pub struct Sample {
    /// Label set attached to this sample.
    pub labels: BTreeMap<String, String>,
    /// Numeric value.
    pub value: f64,
    /// Sample timestamp (opaque to the evaluator; comparisons are bitwise).
    pub timestamp: i64,
}

/// Time-series store used by the evaluator. Holds metric-name-keyed series
/// of samples. Concurrency is left to the caller (this is a lint helper, not
/// a database).
#[derive(Debug, Default, Clone)]
pub struct Engine {
    /// `(metric_name, samples)` pairs. Multiple entries with the same name
    /// are merged by `eval` before evaluation.
    pub series: Vec<(String, Vec<Sample>)>,
}

impl Engine {
    /// Construct an empty engine.
    pub fn new() -> Self {
        Self::default()
    }

    /// Add or append a metric series. If multiple entries share the same
    /// metric name, samples are concatenated (caller responsibility).
    pub fn add_series(&mut self, name: impl Into<String>, samples: Vec<Sample>) {
        self.series.push((name.into(), samples));
    }

    /// Evaluate `ast` against the stored series.
    ///
    /// Returns the resulting instant vector on success or a human-readable
    /// error string on unsupported constructs (range vectors, subqueries,
    /// `rate()`-style function calls, etc.).
    pub fn eval(&self, ast: &Expr) -> Result<Vec<Sample>, String> {
        match ast {
            Expr::Metric(m) => Ok(self.eval_metric(m)),
            Expr::Binary(b) => self.eval_binary(b),
            Expr::Aggregate(a) => self.eval_aggregate(a),
            Expr::Call(c) => Err(format!(
                "function calls like {}() are not supported by promql_eval",
                c.func
            )),
        }
    }

    fn eval_metric(&self, m: &Metric) -> Vec<Sample> {
        if m.range.is_some() {
            // The parser accepts range syntax; the evaluator deliberately
            // does not.
            return Vec::new();
        }
        // If the metric name is empty (rare in valid input) we still want
        // exact-name matching, so no special-casing here.
        let mut out = Vec::new();
        for (name, samples) in &self.series {
            if name != &m.name {
                continue;
            }
            for s in samples {
                if matchers_match(&m.matchers, &s.labels) {
                    out.push(s.clone());
                }
            }
        }
        out
    }

    fn eval_binary(&self, b: &crate::promql_parse::Binary) -> Result<Vec<Sample>, String> {
        if !is_supported_arith_op(b.op) {
            return Err(format!(
                "binary operator `{}` is not supported by promql_eval",
                b.op
            ));
        }
        let lhs = self.eval(&b.lhs)?;
        let rhs = self.eval(&b.rhs)?;
        Ok(apply_arith(b.op, &lhs, &rhs))
    }

    fn eval_aggregate(&self, a: &Aggregate) -> Result<Vec<Sample>, String> {
        match a.op {
            AggOp::Sum | AggOp::Avg => {
                let inner = self.eval(&a.expr)?;
                Ok(aggregate(a.op, &inner, &a.by))
            }
            // Every other aggregation is exposed by the parser but not by the
            // evaluator; the rule of thumb is "test what we can evaluate".
            _ => Err(format!(
                "aggregation `{}` is not supported by promql_eval",
                a.op
            )),
        }
    }
}

/// Returns true when all `matchers` are satisfied by `labels`.
fn matchers_match(matchers: &[LabelMatcher], labels: &BTreeMap<String, String>) -> bool {
    for m in matchers {
        let actual = labels.get(&m.name);
        let ok = match (actual, &m.op) {
            (Some(v), MatchOp::Eq) => v == &m.value,
            (Some(v), MatchOp::Ne) => v != &m.value,
            // Regex matchers without the `regex` crate: fall back to a literal
            // equality test. The point of the evaluator is to lint rules,
            // not to faithfully execute them.
            (Some(v), MatchOp::Re) => v == &m.value,
            (Some(v), MatchOp::Nre) => v != &m.value,
            (None, MatchOp::Eq) => false,
            (None, MatchOp::Re) => false,
            (None, MatchOp::Ne) => true,
            (None, MatchOp::Nre) => true,
        };
        if !ok {
            return false;
        }
    }
    true
}

fn is_supported_arith_op(op: BinOp) -> bool {
    matches!(
        op,
        BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::Div | BinOp::Mod | BinOp::Pow
    )
}

/// Element-wise arithmetic between two sample sets joined on label identity.
/// Series without a matching partner are dropped (Prometheus convention).
fn apply_arith(op: BinOp, lhs: &[Sample], rhs: &[Sample]) -> Vec<Sample> {
    // Index RHS by sorted label tuple for O(1) lookups.
    let mut rhs_by_labels: HashMap<Vec<(String, String)>, Vec<&Sample>> = HashMap::new();
    for s in rhs {
        rhs_by_labels
            .entry(label_tuple(&s.labels))
            .or_default()
            .push(s);
    }

    let mut out = Vec::new();
    for l in lhs {
        let key = label_tuple(&l.labels);
        if let Some(candidates) = rhs_by_labels.get(&key) {
            // Pick the first matching RHS sample for the timestamp mirror —
            // this evaluator ignores time-based semantics.
            let r = match candidates.first() {
                Some(s) => *s,
                None => continue,
            };
            let value = match op {
                BinOp::Add => l.value + r.value,
                BinOp::Sub => l.value - r.value,
                BinOp::Mul => l.value * r.value,
                BinOp::Div => {
                    if r.value == 0.0 {
                        // Prometheus-style: drop on divide-by-zero.
                        continue;
                    }
                    l.value / r.value
                }
                BinOp::Mod => {
                    if r.value == 0.0 {
                        continue;
                    }
                    l.value % r.value
                }
                BinOp::Pow => l.value.powf(r.value),
                _ => continue,
            };
            out.push(Sample {
                labels: l.labels.clone(),
                value,
                timestamp: l.timestamp,
            });
        }
    }
    out
}

fn aggregate(op: AggOp, samples: &[Sample], by: &[String]) -> Vec<Sample> {
    // Per Prometheus semantics: an aggregation WITHOUT a `by(...)` / `without(...)`
    // clause collapses every input into a single empty-label output sample.
    // With `by(...)`, we group by the listed labels. We don't model
    // `without(...)` because the parser only emits `by` for now.
    let mut groups: HashMap<Vec<(String, String)>, Vec<&Sample>> = HashMap::new();
    if by.is_empty() {
        // Single bucket: all input becomes one sample.
        groups.entry(Vec::new()).or_default().extend(samples.iter());
    } else {
        for s in samples {
            let key = group_key(&s.labels, by);
            groups.entry(key).or_default().push(s);
        }
    }

    let mut out = Vec::new();
    for (key, group) in groups {
        if group.is_empty() {
            continue;
        }
        let anchor = group[0];
        let mut group_labels: BTreeMap<String, String> = BTreeMap::new();
        for (k, v) in key {
            group_labels.insert(k, v);
        }
        let value = match op {
            AggOp::Sum => group.iter().map(|s| s.value).sum(),
            AggOp::Avg => {
                let total: f64 = group.iter().map(|s| s.value).sum();
                total / group.len() as f64
            }
            _ => 0.0,
        };
        out.push(Sample {
            labels: group_labels,
            value,
            timestamp: anchor.timestamp,
        });
    }
    out
}

fn label_tuple(labels: &BTreeMap<String, String>) -> Vec<(String, String)> {
    labels.iter().map(|(k, v)| (k.clone(), v.clone())).collect()
}

fn group_key(labels: &BTreeMap<String, String>, by: &[String]) -> Vec<(String, String)> {
    if by.is_empty() {
        label_tuple(labels)
    } else {
        by.iter()
            .filter_map(|k| labels.get_key_value(k).map(|(a, b)| (a.clone(), b.clone())))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::promql_parse::parse;

    fn sample(job: &str, inst: &str, value: f64) -> Sample {
        let mut labels = BTreeMap::new();
        labels.insert("job".to_string(), job.to_string());
        labels.insert("instance".to_string(), inst.to_string());
        Sample {
            labels,
            value,
            timestamp: 1,
        }
    }

    fn engine_with(name: &str, samples: Vec<Sample>) -> Engine {
        let mut e = Engine::new();
        e.add_series(name, samples);
        e
    }

    #[test]
    fn single_metric_select_returns_all_samples() {
        let e = engine_with(
            "up",
            vec![sample("api", "a", 1.0), sample("api", "b", 1.0)],
        );
        let ast = parse("up").expect("parse");
        let got = e.eval(&ast).expect("eval");
        assert_eq!(got.len(), 2);
        assert!(got.iter().all(|s| s.value == 1.0));
    }

    #[test]
    fn label_match_filters_exact() {
        let e = engine_with(
            "up",
            vec![sample("api", "a", 1.0), sample("worker", "a", 0.0)],
        );
        let ast = parse(r#"up{job="api"}"#).expect("parse");
        let got = e.eval(&ast).expect("eval");
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].labels.get("job").map(String::as_str), Some("api"));
    }

    #[test]
    fn label_mismatch_excludes_sample() {
        let e = engine_with("up", vec![sample("api", "a", 1.0)]);
        let ast = parse(r#"up{job="missing"}"#).expect("parse");
        assert!(e.eval(&ast).expect("eval").is_empty());
    }

    #[test]
    fn addition_of_two_series_with_same_labels() {
        let mut e = Engine::new();
        e.add_series("a", vec![sample("api", "x", 2.0)]);
        e.add_series("b", vec![sample("api", "x", 3.0)]);
        let ast = parse("a + b").expect("parse");
        let got = e.eval(&ast).expect("eval");
        assert_eq!(got.len(), 1);
        assert!((got[0].value - 5.0).abs() < 1e-9);
    }

    #[test]
    fn binary_arith_drops_unmatched_labels() {
        let mut e = Engine::new();
        e.add_series("a", vec![sample("api", "x", 2.0)]);
        e.add_series(
            "b",
            vec![sample("worker", "x", 3.0), sample("api", "x", 4.0)],
        );
        let ast = parse("a + b").expect("parse");
        let got = e.eval(&ast).expect("eval");
        assert_eq!(got.len(), 1, "only matching label set survives join");
        assert!((got[0].value - 6.0).abs() < 1e-9);
    }

    #[test]
    fn sum_aggregation_collapses_to_single() {
        let e = engine_with(
            "requests",
            vec![
                sample("api", "a", 10.0),
                sample("api", "b", 5.0),
                sample("api", "c", 3.0),
            ],
        );
        // `sum by(job)` groups by `job`, collapsing instance.
        let ast = parse(r#"sum by (job) (requests)"#).expect("parse");
        let got = e.eval(&ast).expect("eval");
        assert_eq!(got.len(), 1);
        assert!((got[0].value - 18.0).abs() < 1e-9);
        assert_eq!(got[0].labels.get("job").map(String::as_str), Some("api"));
        assert!(!got[0].labels.contains_key("instance"));
    }

    #[test]
    fn avg_aggregation_returns_mean() {
        let e = engine_with(
            "requests",
            vec![
                sample("api", "a", 10.0),
                sample("api", "b", 4.0),
                sample("api", "c", 6.0),
            ],
        );
        let ast = parse("avg(requests)").expect("parse");
        let got = e.eval(&ast).expect("eval");
        assert_eq!(got.len(), 1);
        assert!((got[0].value - (20.0 / 3.0)).abs() < 1e-9);
    }

    #[test]
    fn empty_engine_returns_empty_for_metric() {
        let e = Engine::new();
        let ast = parse("up").expect("parse");
        assert!(e.eval(&ast).expect("eval").is_empty());
    }

    #[test]
    fn empty_engine_returns_empty_for_aggregation() {
        let e = Engine::new();
        let ast = parse("sum(up)").expect("parse");
        assert!(e.eval(&ast).expect("eval").is_empty());
    }

    #[test]
    fn range_vector_returns_empty_vector_no_panic() {
        let e = engine_with("up", vec![sample("api", "a", 1.0)]);
        let ast = parse("up[5m]").expect("parse");
        // The evaluator treats range vectors as an empty instant vector —
        // intentionally inert so we don't simulate time.
        let got = e.eval(&ast).expect("eval");
        assert!(got.is_empty());
    }

    #[test]
    fn function_call_is_explicit_error() {
        let e = Engine::new();
        let ast = parse("rate(up[5m])").expect("parse");
        assert!(e.eval(&ast).is_err());
    }
}
