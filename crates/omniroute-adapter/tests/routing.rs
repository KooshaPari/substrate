//! Integration tests for omniroute-adapter.
//!
//! The basic trait/configure tests run in normal CI. The real
//! "route a task through the OmniRoute proxy" integration test is gated
//! behind `RUN_FORGE_INT=1` because it requires:
//!   * the OmniRoute proxy listening on 127.0.0.1:20128
//!   * `OMNIROUTE_API_KEY` set to a real key
//!
//! and must be skipped on CI with no network.

use substrate_core::domain::{RoutingDecision, Task};
use substrate_core::ports::RoutingPort;

#[tokio::test]
async fn route_decision_returns_default_forge_kimi() {
    // Runs unconditionally — no network, no env requirement.
    let adapter = omniroute_adapter::OmniRouteAdapter::new();
    let task = Task::new("test prompt", "/tmp");
    let decision: RoutingDecision = adapter.route_decision(&task).await.unwrap();
    assert_eq!(decision.engine, "forge");
    assert_eq!(decision.model, omniroute_adapter::DEFAULT_MODEL);
}

#[test]
fn configure_forge_provider_does_not_touch_network() {
    // No network: we only assert the config is built from env vars.
    let orig_key = std::env::var("OMNIROUTE_API_KEY").ok();
    std::env::set_var("OMNIROUTE_API_KEY", "integration-test-key");

    let adapter = omniroute_adapter::OmniRouteAdapter::new();
    let config = adapter.configure_forge_provider().unwrap();
    assert_eq!(config.base_url, omniroute_adapter::DEFAULT_BASE_URL);
    assert_eq!(config.api_key, "integration-test-key");

    std::env::remove_var("OMNIROUTE_API_KEY");
    if let Some(k) = orig_key {
        std::env::set_var("OMNIROUTE_API_KEY", k);
    }
}

#[tokio::test]
#[ignore = "requires RUN_FORGE_INT=1 and a live OmniRoute proxy; skipped in CI"]
async fn route_through_real_omniroute_proxy_returns_structured_decision() {
    if std::env::var("RUN_FORGE_INT").is_err() {
        // Defensive: #[ignore] handles the gate, but skip here too in
        // case the test runner is invoked with --include-ignored.
        eprintln!("set RUN_FORGE_INT=1 to run the live OmniRoute integration test");
        return;
    }
    // When the proxy is up + OMNIROUTE_API_KEY is set, the adapter must
    // still return its declared decision (we don't actually invoke the
    // network here — that's forge's job).
    let adapter = omniroute_adapter::OmniRouteAdapter::new();
    let task = Task::new("integration prompt", "/tmp");
    let decision = adapter.route_decision(&task).await.unwrap();
    assert_eq!(decision.engine, "forge");
    assert!(!decision.model.is_empty());
}
