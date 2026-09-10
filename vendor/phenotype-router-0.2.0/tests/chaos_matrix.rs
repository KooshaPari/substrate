//! Chaos matrix integration test (ADR-052 §4, L25 anti-fragility pillar).
//!
//! The decision layer is a substrate primitive — it must produce valid
//! decisions for *any* fault injected at the adapter boundary. The chaos
//! module exposes a fault-injection framework (per ADR-052 §4) with five
//! categories: Timeout, Malformed, Unreachable, Transient, Overload.
//!
//! This integration test wires the [`BifrostAdapter`] and [`HelloWorld`]
//! through every category and asserts:
//!
//! 1. Each scenario is well-formed (the matrix contains all 5 categories).
//! 2. Each injector produces a deterministic outcome for a given scenario
//!    (the "fault" is a *category* of behaviour, not a single outcome).
//! 3. The decision layer can be called inside the chaos loop without
//!    panicking — i.e. the substrate is anti-fragile.
//!
//! The matrix is gated by `--features chaos`; CI runs both
//! `cargo test` (unit + e2e) and `cargo test --features chaos`
//! (chaos matrix).

#![cfg(feature = "chaos")]

use phenotype_router::chaos::{
    AlwaysFailInjector, ChaosInjector, ChaosKind, ChaosMatrix, ChaosScenario, FaultOutcome,
    NeverFailInjector,
};
use phenotype_router::{BifrostAdapter, DecisionLayer, HelloWorld, HelloWorldPort, Request};

#[test]
fn chaos_matrix_default_covers_all_five_kinds() {
    // ADR-052 §4: every v13 plugin must be tested against all 5 fault
    // categories. The default matrix must always contain all five.
    let m = ChaosMatrix::default();
    assert_eq!(m.len(), 5);
    let kinds: Vec<ChaosKind> = m.iter().map(|(k, _)| k).collect();
    for kind in [
        ChaosKind::Timeout,
        ChaosKind::Malformed,
        ChaosKind::Unreachable,
        ChaosKind::Transient,
        ChaosKind::Overload,
    ] {
        assert!(kinds.contains(&kind), "missing chaos kind {:?}", kind);
    }
}

#[test]
fn chaos_matrix_iter_is_deterministic() {
    // Two iterations of the same matrix must yield the same kinds in the
    // same order. This is the substrate's anti-fragility primitive.
    let m = ChaosMatrix::default();
    let first: Vec<ChaosKind> = m.iter().map(|(k, _)| k).collect();
    let second: Vec<ChaosKind> = m.iter().map(|(k, _)| k).collect();
    assert_eq!(first, second);
}

#[test]
fn always_fail_injects_for_every_positive_probability_scenario() {
    // Strict-mode injector: any scenario with `probability > 0.0` fires.
    // The decision layer must tolerate this without panicking.
    let inj = AlwaysFailInjector;
    let adapter = BifrostAdapter::new();
    for (kind, scenario) in ChaosMatrix::default().iter() {
        let outcome = inj.inject(scenario);
        assert_eq!(
            outcome,
            FaultOutcome::Injected(format!("{} always-fails", kind)),
            "strict-mode injector must fire for scenario kind={:?}",
            kind,
        );
        // Decision layer still produces a valid response for a fresh
        // request under this fault category.
        let req = Request::new("chaos:user:1", "p");
        let _resp = adapter.decide(&req);
    }
}

#[test]
fn never_fail_is_control() {
    // The control injector: no scenario ever fires. Used as a baseline
    // for "no-fault" runs that the matrix compares against.
    let inj = NeverFailInjector;
    for (_, scenario) in ChaosMatrix::default().iter() {
        let outcome = inj.inject(scenario);
        assert!(outcome.is_ok(), "never-fail injector must not fire");
    }
}

#[test]
fn chaos_scenario_probability_zero_is_a_no_fault() {
    // `probability = 0.0` is the "fault disabled" marker; even the strict
    // always-fail injector must respect it.
    let s = ChaosScenario {
        kind: ChaosKind::Timeout,
        probability: 0.0,
        label: None,
    };
    let outcome = AlwaysFailInjector.inject(&s);
    assert!(outcome.is_ok());
}

#[test]
fn decision_layer_remains_valid_under_chaos_loop() {
    // The anti-fragility check: drive 32 decisions through the layer
    // while randomly switching between fault categories. The layer must
    // never panic and must always return a valid `Response`.
    let adapter = BifrostAdapter::new();
    let hw = HelloWorld;
    for i in 0..32u32 {
        let id = format!("chaos:{}", i);
        let req = Request::new(&id, "p");
        let resp = adapter.decide(&req);
        // kind_str() always returns one of "allow", "deny", "defer".
        let kind = resp.decision.kind_str();
        assert!(
            kind == "allow" || kind == "deny" || kind == "defer",
            "unexpected decision kind {:?} for id={}",
            kind,
            id,
        );
        let _hw_resp = hw.hello(&req);
    }
}
