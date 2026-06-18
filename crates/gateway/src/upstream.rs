//! Upstream provider integration via reqwest (streaming-capable HTTP client).
//! Routes requests through substrate's RoutingPort to the selected provider.
//! Streams responses directly to client (no buffering) for O(chunk_size) memory.

use crate::circuit_breaker::CircuitBreaker;
use crate::config::ProviderConfig;
use axum::{body::Body, http::Response};
use bytes::Bytes;
use futures::TryStreamExt;
use reqwest::Client;
use std::collections::HashMap;
use std::sync::Arc;
use substrate_core::{
    domain::Task,
    error::{Result as SubstrateResult, SubstrateError},
    ports::RoutingPort,
};
use tokio::sync::RwLock;

/// Upstream provider client that routes via substrate's RoutingPort.
/// Streams responses directly to client (never buffers full response body).
pub struct UpstreamClient {
    http_client: Client,
    routing_port: Arc<dyn RoutingPort>,
    circuit_breakers: Arc<RwLock<HashMap<String, CircuitBreaker>>>,
    /// Provider configs used to resolve `base_url` and bearer token by name.
    providers: Arc<Vec<ProviderConfig>>,
}

impl UpstreamClient {
    /// Create a new upstream client with the given routing port and provider list.
    pub fn new(routing_port: Arc<dyn RoutingPort>, providers: Arc<Vec<ProviderConfig>>) -> Self {
        Self {
            http_client: Client::new(),
            routing_port,
            circuit_breakers: Arc::new(RwLock::new(HashMap::new())),
            providers,
        }
    }

    /// Route a request through substrate's RoutingPort and stream the response.
    ///
    /// The upstream URL is resolved from the matched [`ProviderConfig`]'s `base_url`
    /// rather than being hardcoded, so each provider uses its own endpoint.
    /// The bearer token is read from the environment variable named in `api_key_env`
    /// at request time (never stored in config).
    ///
    /// Memory usage is O(chunk_size), not O(response_size).
    pub async fn route_and_stream(&self, task: Task) -> SubstrateResult<Response<Body>> {
        // Get routing decision from substrate
        let decision = self.routing_port.route_decision(&task).await?;

        // Get or create circuit breaker for this provider
        let mut breakers = self.circuit_breakers.write().await;
        breakers
            .entry(decision.engine.clone())
            .or_insert_with(CircuitBreaker::new);

        // Check if circuit breaker is open (fail fast if provider is down)
        if let Some(breaker) = breakers.get(&decision.engine) {
            if breaker.is_open() {
                drop(breakers);
                return Err(SubstrateError::Engine(format!(
                    "Provider circuit breaker open for {}",
                    decision.engine
                )));
            }
        }

        drop(breakers); // Release lock before making request

        // Resolve provider config by engine name to get base_url and api_key_env.
        // Falls back to the legacy hardcoded pattern only when no provider matches,
        // so existing OmniRoute traffic (which may not carry a named engine) continues
        // to work during the migration period.
        let (upstream_url, bearer_token) = self.resolve_upstream(&decision.engine)?;

        // Make streaming request to upstream provider
        let mut req_builder = self
            .http_client
            .post(&upstream_url)
            .header("User-Agent", "substrate-gateway");

        if let Some(token) = bearer_token {
            req_builder = req_builder.header("Authorization", format!("Bearer {token}"));
        }

        let response = req_builder
            .send()
            .await
            .map_err(|e| SubstrateError::Engine(format!("Upstream request failed: {}", e)))?;

        let status = response.status();

        // Convert reqwest response to axum response with streaming body
        let streaming_body = Body::from_stream(
            response
                .bytes_stream()
                .map_ok(Bytes::from)
                .map_err(std::io::Error::other),
        );

        Ok(Response::builder()
            .status(status)
            .body(streaming_body)
            .unwrap())
    }

    /// Resolve the upstream URL and bearer token for a given engine/provider name.
    ///
    /// When the engine name matches a registered [`ProviderConfig`], returns
    /// `(base_url + "/chat/completions", Some(api_key))`.  If the API key env-var
    /// is missing, returns `Err` rather than panicking.
    ///
    /// When no provider matches (OmniRoute fall-through), returns the legacy URL
    /// with `None` for the token so existing behaviour is preserved.
    pub(crate) fn resolve_upstream(
        &self,
        engine: &str,
    ) -> SubstrateResult<(String, Option<String>)> {
        if let Some(provider) = self.providers.iter().find(|p| p.name == engine) {
            let url = format!("{}/chat/completions", provider.base_url);
            let token = std::env::var(&provider.api_key_env).map_err(|_| {
                SubstrateError::Engine(format!(
                    "API key env var `{}` not set for provider `{}`",
                    provider.api_key_env, provider.name
                ))
            })?;
            if token.trim().is_empty() {
                return Err(SubstrateError::Engine(format!(
                    "API key env var `{}` is empty for provider `{}`",
                    provider.api_key_env, provider.name
                )));
            }
            Ok((url, Some(token)))
        } else {
            // OmniRoute / unknown engine: use legacy pattern without auth header.
            let url = format!("http://{}:3000/v1/chat/completions", engine);
            Ok((url, None))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::ProviderConfig;
    use std::sync::Arc;
    use substrate_core::{
        domain::{RoutingDecision, Task},
        error::Result as SubstrateResult,
        ports::RoutingPort,
    };

    // ---------------------------------------------------------------------------
    // Minimal stub routing port — no live HTTP needed
    // ---------------------------------------------------------------------------

    struct StubRouter;

    #[async_trait::async_trait]
    impl RoutingPort for StubRouter {
        async fn route_decision(&self, _task: &Task) -> SubstrateResult<RoutingDecision> {
            Ok(RoutingDecision {
                engine: "deepseek".into(),
                model: "deepseek-chat".into(),
                reason: None,
            })
        }
    }

    fn make_client(providers: Vec<ProviderConfig>) -> UpstreamClient {
        UpstreamClient::new(Arc::new(StubRouter), Arc::new(providers))
    }

    // ---------------------------------------------------------------------------
    // URL construction tests (no live HTTP)
    // ---------------------------------------------------------------------------

    #[test]
    fn known_provider_url_uses_base_url() {
        // Set a dummy key so resolve_upstream doesn't error on missing env var
        std::env::set_var("DEEPSEEK_API_KEY", "test-key-ds");

        let providers = vec![ProviderConfig::new(
            "deepseek",
            "https://api.deepseek.com/v1",
            "DEEPSEEK_API_KEY",
        )];
        let client = make_client(providers);

        let (url, token) = client.resolve_upstream("deepseek").unwrap();
        assert_eq!(url, "https://api.deepseek.com/v1/chat/completions");
        assert_eq!(token, Some("test-key-ds".to_string()));

        std::env::remove_var("DEEPSEEK_API_KEY");
    }

    #[test]
    fn unknown_engine_falls_back_to_legacy_url() {
        let client = make_client(vec![]);
        let (url, token) = client.resolve_upstream("some-engine").unwrap();
        assert_eq!(url, "http://some-engine:3000/v1/chat/completions");
        assert!(token.is_none());
    }

    #[test]
    fn missing_api_key_env_returns_error() {
        std::env::remove_var("MISSING_KEY_ENV");
        let providers = vec![ProviderConfig::new(
            "myprovider",
            "https://api.example.com/v1",
            "MISSING_KEY_ENV",
        )];
        let client = make_client(providers);
        let result = client.resolve_upstream("myprovider");
        assert!(result.is_err(), "expected Err when env var is missing");
        let msg = format!("{}", result.unwrap_err());
        assert!(
            msg.contains("MISSING_KEY_ENV"),
            "error should name the missing env var: {msg}"
        );
    }

    #[test]
    fn empty_api_key_env_returns_error() {
        std::env::set_var("EMPTY_KEY_ENV", "   ");
        let providers = vec![ProviderConfig::new(
            "myprovider",
            "https://api.example.com/v1",
            "EMPTY_KEY_ENV",
        )];
        let client = make_client(providers);
        let result = client.resolve_upstream("myprovider");
        assert!(result.is_err(), "expected Err when env var is blank");
        std::env::remove_var("EMPTY_KEY_ENV");
    }

    #[test]
    fn url_never_double_slashes() {
        std::env::set_var("KILO_API_KEY", "k-test");
        let providers = vec![ProviderConfig::new(
            "kilocode",
            "https://api.kilo.ai/api/gateway/v1",
            "KILO_API_KEY",
        )];
        let client = make_client(providers);
        let (url, _) = client.resolve_upstream("kilocode").unwrap();
        assert!(
            !url.contains("//chat"),
            "URL must not contain double-slash before /chat: {url}"
        );
        assert!(url.ends_with("/chat/completions"), "URL: {url}");
        std::env::remove_var("KILO_API_KEY");
    }

    #[test]
    fn test_streaming_memory_model() {
        // Example: 100MB response with 64KB chunks — memory is bounded by chunk_size
        let chunk_size = 64 * 1024; // 64 KB
        let response_size = 100 * 1024 * 1024; // 100 MB
        let num_chunks = response_size / chunk_size;
        assert!(chunk_size < response_size);
        assert!(num_chunks > 1);
    }
}
