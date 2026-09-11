//! Chaos matrix for the decision layer (ADR-052 §4).
//!
//! `phenotype-router`'s decision layer must produce *deterministic* results
//! for the same `(adapter, request)` tuple even when a non-deterministic
//! fail-stop / fail-delay / fail-fault fault is injected at the adapter
//! boundary. The [`ChaosMatrix`] struct + [`ChaosInjector`] trait describe
//! the fault model the decision layer's tests use to validate determinism.
//!
//! Per ADR-052 §4, every v13 plugin must be tested against all 5 fault
//! categories below. The `tests/chaos_matrix.rs` integration test wires
//! each plugin through every fault and asserts:
//!
//! 1. The decision is *valid* (`Allow` or `Deny`; never a panic or a
//!    half-formed `Response`).
//! 2. The decision is *stable* across repeated invocations under the same
//!    fault (chaos is a *category* of behaviour, not a single outcome).
//! 3. The decision is *traceable* — at least one
//!    `phenotype.router.plugin.chaos` attribute is present on the
//!    `Response.trace` map.

use std::collections::BTreeMap;

/// Fault categories the chaos matrix must cover.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ChaosKind {
    /// Plugin times out (no response within `deadline`).
    Timeout,
    /// Plugin returns a malformed payload (truncated JSON, wrong shape).
    Malformed,
    /// Plugin is unreachable (network error, DNS failure, refused).
    Unreachable,
    /// Plugin returns a transient error (5xx, retryable).
    Transient,
    /// Plugin is overloaded and the substrate should shed load.
    Overload,
}

impl ChaosKind {
    /// Stable string used in spans / logs.
    pub fn as_str(self) -> &'static str {
        match self {
            ChaosKind::Timeout => "timeout",
            ChaosKind::Malformed => "malformed",
            ChaosKind::Unreachable => "unreachable",
            ChaosKind::Transient => "transient",
            ChaosKind::Overload => "overload",
        }
    }
}

impl std::fmt::Display for ChaosKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Configuration for a single chaos scenario.
#[derive(Debug, Clone)]
pub struct ChaosScenario {
    /// Fault kind to inject.
    pub kind: ChaosKind,
    /// Probability (0.0 - 1.0) that the fault fires for each call.
    /// 1.0 = always-fail (deterministic chaos); 0.0 = never-fail.
    pub probability: f64,
    /// Optional human label (e.g. `"net_partition"`,
    /// `"slow_loris"`); surfaced in spans + logs.
    pub label: Option<String>,
}

impl ChaosScenario {
    /// Deterministic always-fail scenario with the given kind.
    pub fn always(kind: ChaosKind) -> Self {
        Self {
            kind,
            probability: 1.0,
            label: None,
        }
    }

    /// Random-fail scenario (`probability = 0.5`) with a label.
    pub fn flaky(kind: ChaosKind, label: impl Into<String>) -> Self {
        Self {
            kind,
            probability: 0.5,
            label: Some(label.into()),
        }
    }
}

/// Chaos matrix: one scenario per [`ChaosKind`] (5 total). Used in
/// `tests/chaos_matrix.rs` to wire each plugin through every fault
/// category.
#[derive(Debug, Clone)]
pub struct ChaosMatrix {
    /// Scenarios indexed by `ChaosKind` (always contains all 5).
    pub scenarios: BTreeMap<ChaosKind, ChaosScenario>,
}

impl Default for ChaosMatrix {
    fn default() -> Self {
        let mut m: BTreeMap<ChausKindKey, ChaosScenario> = BTreeMap::new();
        m.insert(ChaosKind::Timeout, ChaosScenario::always(ChaosKind::Timeout));
        m.insert(
            ChaosKind::Malformed,
            ChaosScenario::always(ChaosKind::Malformed),
        );
        m.insert(
            ChaosKind::Unreachable,
            ChaosScenario::always(ChaosKind::Unreachable),
        );
        m.insert(
            ChaosKind::Transient,
            ChaosScenario::always(ChaosKind::Transient),
        );
        m.insert(ChaosKind::Overload, ChaosScenario::always(ChaosKind::Overload));
        Self {
            scenarios: m,
        }
    }
}

type ChausKindKey = ChaosKind;

impl ChaosMatrix {
    /// Iterate over the 5 (kind, scenario) pairs in deterministic order.
    pub fn iter(&self) -> impl Iterator<Item = (ChaosKind, &ChaosScenario)> {
        self.scenarios.iter().map(|(k, v)| (*k, v))
    }

    /// Number of scenarios in the matrix (always 5 for the default matrix).
    pub fn len(&self) -> usize {
        self.scenarios.len()
    }

    /// True iff the matrix is empty.
    pub fn is_empty(&self) -> bool {
        self.scenarios.is_empty()
    }
}

/// A fault outcome the chaos layer can return.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FaultOutcome {
    /// No fault fired — the plugin produced a normal response.
    Ok,
    /// A fault fired; the plugin's response is the contained outcome
    /// (typically `Deny` for the chaos matrix's strict-mode
    /// "always-fail" scenarios).
    Injected(String),
}

impl FaultOutcome {
    /// True iff no fault fired.
    pub fn is_ok(&self) -> bool {
        matches!(self, FaultOutcome::Ok)
    }
}

/// Chaos injector: takes a [`ChaosScenario`] and returns the
/// [`FaultOutcome`] for one call. Plugin tests implement this trait
/// for each fault kind to validate that the plugin fails safely and
/// deterministically.
pub trait ChaosInjector {
    /// Apply one round of chaos. Returns `Ok` if the plugin produced a
    /// normal response, or `Injected(reason)` if a fault fired.
    fn inject(&self, scenario: &ChaosScenario) -> FaultOutcome;
}

/// Always-fail injector (used in the strict-mode chaos matrix test).
/// `Injected` for any scenario with `probability > 0.0`.
pub struct AlwaysFailInjector;
impl ChaosInjector for AlwaysFailInjector {
    fn inject(&self, scenario: &ChaosScenario) -> FaultOutcome {
        if scenario.probability > 0.0 {
            FaultOutcome::Injected(format!("{} always-fails", scenario.kind))
        } else {
            FaultOutcome::Ok
        }
    }
}

/// Never-fail injector (control: no fault ever fires).
pub struct NeverFailInjector;
impl ChaosInjector for NeverFailInjector {
    fn inject(&self, _: &ChaosScenario) -> FaultOutcome {
        FaultOutcome::Ok
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chaos_kind_as_str_matches_documented_names() {
        assert_eq!(ChaosKind::Timeout.as_str(), "timeout");
        assert_eq!(ChaosKind::Malformed.as_str(), "malformed");
        assert_eq!(ChaosKind::Unreachable.as_str(), "unreachable");
        assert_eq!(ChaosKind::Transient.as_str(), "transient");
        assert_eq!(ChaosKind::Overload.as_str(), "overload");
    }

    #[test]
    fn default_matrix_contains_all_five_kinds() {
        let m = ChaosMatrix::default();
        assert_eq!(m.len(), 5);
        assert!(!m.is_empty());
        let kinds: Vec<ChaosKind> = m.iter().map(|(k, _)| k).collect();
        assert!(kinds.contains(&ChaosKind::Timeout));
        assert!(kinds.contains(&ChaosKind::Malformed));
        assert!(kinds.contains(&ChaosKind::Unreachable));
        assert!(kinds.contains(&ChaosKind::Transient));
        assert!(kinds.contains(&ChaosKind::Overload));
    }

    #[test]
    fn default_matrix_iter_is_deterministic() {
        let m = ChaosMatrix::default();
        let first: Vec<ChaosKind> = m.iter().map(|(k, _)| k).collect();
        let second: Vec<ChaosKind> = m.iter().map(|(k, _)| k).collect();
        assert_eq!(first, second);
    }

    #[test]
    fn always_fail_injects_when_probability_positive() {
        let inj = AlwaysFailInjector;
        let s = ChaosScenario::always(ChaosKind::Timeout);
        let res = inj.inject(&s);
        assert_eq!(res, FaultOutcome::Injected("timeout always-fails".to_string()));
        assert!(!res.is_ok());
    }

    #[test]
    fn never_fail_never_injects() {
        let inj = NeverFailInjector;
        let s = ChaosScenario::always(ChaosKind::Malformed);
        let res = inj.inject(&s);
        assert_eq!(res, FaultOutcome::Ok);
        assert!(res.is_ok());
    }

    #[test]
    fn chaos_scenario_always_helper() {
        let s = ChaosScenario::always(ChaosKind::Overload);
        assert_eq!(s.kind, ChaosKind::Overload);
        assert_eq!(s.probability, 1.0);
        assert!(s.label.is_none());
    }

    #[test]
    fn chaos_scenario_flaky_helper() {
        let s = ChaosScenario::flaky(ChaosKind::Transient, "net_blip");
        assert_eq!(s.kind, ChaosKind::Transient);
        assert_eq!(s.probability, 0.5);
        assert_eq!(s.label.as_deref(), Some("net_blip"));
    }
}
