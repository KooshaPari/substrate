//! # omniroute-adapter
//!
//! [`RoutingPort`] adapter that configures and routes through the OmniRoute
//! proxy at `http://127.0.0.1:20128/v1`. The API key is read from the
//! `OMNIROUTE_API_KEY` environment variable (loaded from a `.env` file via
//! `dotenvy` if present).
//!
//! The default model routing target is
//! `accounts/fireworks/routers/kimi-k2p6-turbo`. The adapter implements
//! [`RoutingPort::route_decision`] and returns a structured
//! [`substrate_core::domain::RoutingDecision`] (engine + model + rationale).
//!
//! This crate compiles against `substrate-core` only — no `engine-*` dependency.
#![forbid(unsafe_code)]
#![warn(missing_docs)]

use async_trait::async_trait;
use substrate_core::domain::{RoutingDecision, Task};
use substrate_core::error::{Result, SubstrateError};
use substrate_core::ports::RoutingPort;

/// Default base URL for the OmniRoute local proxy.
pub const DEFAULT_BASE_URL: &str = "http://127.0.0.1:20128/v1";

/// Default model routing target (Phase 1's OmniRoute kimi router).
pub const DEFAULT_MODEL: &str = "accounts/fireworks/routers/kimi-k2p6-turbo";

/// Engine name this router targets.
pub const ENGINE: &str = "forge";

/// Configuration for the OmniRoute provider connection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderConfig {
    /// The base URL of the OpenAI-compatible endpoint.
    pub base_url: String,
    /// The API key for authentication.
    pub api_key: String,
}

impl ProviderConfig {
    /// Build a `ProviderConfig` from the process environment.
    ///
    /// Reads `OMNIROUTE_API_KEY` (required) and optionally
    /// `OMNIROUTE_BASE_URL` (defaults to [`DEFAULT_BASE_URL`]).
    /// Calls `dotenvy::dotenv().ok()` first to load a `.env` file (never
    /// hardcoded — the key is sourced from the environment).
    pub fn from_env() -> Result<Self> {
        let _ = dotenvy::dotenv();
        let api_key = std::env::var("OMNIROUTE_API_KEY")
            .map_err(|e| SubstrateError::Routing(format!("OMNIROUTE_API_KEY not set: {e}")))?;
        let base_url =
            std::env::var("OMNIROUTE_BASE_URL").unwrap_or_else(|_| DEFAULT_BASE_URL.to_string());
        Ok(ProviderConfig { base_url, api_key })
    }

    /// Build a `ProviderConfig` from explicit values (no environment
    /// lookup). Useful for tests and for callers that already have the
    /// key in hand.
    pub fn new(base_url: impl Into<String>, api_key: impl Into<String>) -> Self {
        ProviderConfig {
            base_url: base_url.into(),
            api_key: api_key.into(),
        }
    }
}

/// The OmniRoute-backed [`RoutingPort`] implementation.
///
/// `OmniRouteAdapter` is `Clone + Send + Sync`, holds no IO resources, and
/// never touches the network itself — the network call is performed by the
/// downstream engine (forge) once the provider has been configured.
#[derive(Debug, Clone)]
pub struct OmniRouteAdapter {
    model: String,
}

impl Default for OmniRouteAdapter {
    fn default() -> Self {
        Self {
            model: DEFAULT_MODEL.to_string(),
        }
    }
}

impl OmniRouteAdapter {
    /// Create a new adapter with the default model.
    pub fn new() -> Self {
        Self::default()
    }

    /// Create an adapter with a custom model id.
    pub fn with_model(model: impl Into<String>) -> Self {
        Self {
            model: model.into(),
        }
    }

    /// Return the model id this adapter will route to.
    pub fn model(&self) -> &str {
        &self.model
    }

    /// Build the forge provider configuration (base_url + api_key) without
    /// making any network call. The caller persists / applies it (e.g. by
    /// writing it to forge's provider config file or passing it via env).
    pub fn configure_forge_provider(&self) -> Result<ProviderConfig> {
        ProviderConfig::from_env()
    }
}

#[async_trait]
impl RoutingPort for OmniRouteAdapter {
    async fn route_decision(&self, _task: &Task) -> Result<RoutingDecision> {
        Ok(RoutingDecision {
            engine: ENGINE.to_string(),
            model: self.model.clone(),
            reason: Some("omniroute-adapter:default".to_string()),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A serial env-mutex: env::set_var/remove_var is process-global and
    /// these tests run in parallel by default. We take this lock to keep
    /// them deterministic.
    static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    #[test]
    fn default_model_is_kimi_router() {
        let adapter = OmniRouteAdapter::new();
        assert_eq!(adapter.model(), DEFAULT_MODEL);
        assert_eq!(
            adapter.model(),
            "accounts/fireworks/routers/kimi-k2p6-turbo"
        );
    }

    #[test]
    fn with_model_overrides() {
        let adapter = OmniRouteAdapter::with_model("my-custom-model");
        assert_eq!(adapter.model(), "my-custom-model");
    }

    #[test]
    fn provider_config_from_env_builds_default_base_url() {
        let _g = ENV_LOCK.lock().unwrap();
        let orig_key = std::env::var("OMNIROUTE_API_KEY").ok();
        let orig_url = std::env::var("OMNIROUTE_BASE_URL").ok();
        std::env::remove_var("OMNIROUTE_BASE_URL");
        std::env::set_var("OMNIROUTE_API_KEY", "test-key-123");

        let config = ProviderConfig::from_env().unwrap();
        assert_eq!(config.base_url, DEFAULT_BASE_URL);
        assert_eq!(config.api_key, "test-key-123");

        std::env::remove_var("OMNIROUTE_API_KEY");
        if let Some(k) = orig_key {
            std::env::set_var("OMNIROUTE_API_KEY", k);
        }
        if let Some(u) = orig_url {
            std::env::set_var("OMNIROUTE_BASE_URL", u);
        }
    }

    #[test]
    fn provider_config_respects_custom_base_url() {
        let _g = ENV_LOCK.lock().unwrap();
        let orig_key = std::env::var("OMNIROUTE_API_KEY").ok();
        let orig_url = std::env::var("OMNIROUTE_BASE_URL").ok();
        std::env::set_var("OMNIROUTE_API_KEY", "key");
        std::env::set_var("OMNIROUTE_BASE_URL", "http://custom:9999/v1");

        let config = ProviderConfig::from_env().unwrap();
        assert_eq!(config.base_url, "http://custom:9999/v1");

        std::env::remove_var("OMNIROUTE_API_KEY");
        std::env::remove_var("OMNIROUTE_BASE_URL");
        if let Some(k) = orig_key {
            std::env::set_var("OMNIROUTE_API_KEY", k);
        }
        if let Some(u) = orig_url {
            std::env::set_var("OMNIROUTE_BASE_URL", u);
        }
    }

    #[test]
    fn provider_config_missing_key_errors() {
        let _g = ENV_LOCK.lock().unwrap();
        let orig_key = std::env::var("OMNIROUTE_API_KEY").ok();
        std::env::remove_var("OMNIROUTE_API_KEY");

        let result = ProviderConfig::from_env();
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("OMNIROUTE_API_KEY"),
            "error should name the missing env var, got: {err_msg}"
        );

        if let Some(k) = orig_key {
            std::env::set_var("OMNIROUTE_API_KEY", k);
        }
    }

    #[test]
    fn configure_forge_provider_uses_env_not_network() {
        let _g = ENV_LOCK.lock().unwrap();
        let orig_key = std::env::var("OMNIROUTE_API_KEY").ok();
        std::env::set_var("OMNIROUTE_API_KEY", "test-api-key");

        let adapter = OmniRouteAdapter::new();
        let config = adapter.configure_forge_provider().unwrap();
        assert_eq!(config.base_url, DEFAULT_BASE_URL);
        assert_eq!(config.api_key, "test-api-key");
        // No network call: the base URL is the localhost default, the
        // adapter holds no socket, and the only side effect was the
        // dotenvy::dotenv() attempt (which is a no-op when no .env exists).
        assert_eq!(config.base_url, "http://127.0.0.1:20128/v1");

        std::env::remove_var("OMNIROUTE_API_KEY");
        if let Some(k) = orig_key {
            std::env::set_var("OMNIROUTE_API_KEY", k);
        }
    }

    #[test]
    fn provider_config_explicit_constructor() {
        let cfg = ProviderConfig::new("http://example.invalid/v1", "abc");
        assert_eq!(cfg.base_url, "http://example.invalid/v1");
        assert_eq!(cfg.api_key, "abc");
    }

    #[tokio::test]
    async fn route_decision_returns_default_forge_kimi() {
        let adapter = OmniRouteAdapter::new();
        let task = Task::new("anything", "/tmp");
        let decision = adapter.route_decision(&task).await.unwrap();
        assert_eq!(decision.engine, "forge");
        assert_eq!(decision.model, DEFAULT_MODEL);
        assert!(decision.reason.is_some());
    }

    #[tokio::test]
    async fn route_compat_returns_engine_name() {
        // The trait's default `route()` derives the engine from
        // `route_decision()` — adapters only implement the structured one.
        let adapter = OmniRouteAdapter::new();
        let task = Task::new("anything", "/tmp");
        let engine = adapter.route(&task).await.unwrap();
        assert_eq!(engine, "forge");
    }
}
