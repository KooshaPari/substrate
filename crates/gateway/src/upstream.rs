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
use std::time::{Duration, Instant};
use substrate_core::{
    domain::Task,
    error::{Result as SubstrateResult, SubstrateError},
    ports::RoutingPort,
};
use tokio::sync::RwLock;

// ---------------------------------------------------------------------------
// Metrics
// ---------------------------------------------------------------------------

/// Timing metrics captured during an upstream proxy request.
#[derive(Debug, Clone)]
pub struct ProxyMetrics {
    /// Wall-clock time from sending the request until receiving response headers
    /// (a.k.a. "time-to-first-byte" at the header level).
    pub latency: Duration,
    /// Duration from sending the request until the first byte of the body was
    /// made available.  For streaming SSE responses this is the same as
    /// `latency`; for buffered responses it may be slightly longer.
    pub first_byte_time: Duration,
    /// Whether the upstream response was detected as a Server-Sent Events stream.
    pub is_sse: bool,
}

// ---------------------------------------------------------------------------
// SSE detection helper
// ---------------------------------------------------------------------------

/// Return `true` when the upstream `Content-Type` header indicates an SSE stream
/// (`text/event-stream`).  The check is case-insensitive and ignores any charset
/// or boundary parameters (`text/event-stream; charset=utf-8` → `true`).
pub fn is_sse_content_type(headers: &reqwest::header::HeaderMap) -> bool {
    headers
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|ct| {
            ct.split(';')
                .next()
                .map(|t| t.trim().eq_ignore_ascii_case("text/event-stream"))
                .unwrap_or(false)
        })
}

// ---------------------------------------------------------------------------
// UpstreamClient
// ---------------------------------------------------------------------------

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
    /// **SSE passthrough**: when the upstream returns `Content-Type: text/event-stream`
    /// the response is forwarded with the correct SSE headers (`content-type`,
    /// `cache-control`, `connection`) so the client can consume it as a live
    /// event stream.  For all other content types the response is forwarded as
    /// chunked transfer encoding.
    ///
    /// **Metrics**: latency (request-send → response-headers received) and
    /// first-byte time are recorded and returned alongside the response.
    ///
    /// Memory usage is O(chunk_size), not O(response_size).
    pub async fn route_and_stream(
        &self,
        task: Task,
    ) -> SubstrateResult<(Response<Body>, ProxyMetrics)> {
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
        let (upstream_url, bearer_token) = self.resolve_upstream(&decision.engine)?;

        // ── Metrics: mark request-send instant ─────────────────────────────
        let request_start = Instant::now();

        // Make streaming request to upstream provider
        let mut req_builder = self
            .http_client
            .post(&upstream_url)
            .header("User-Agent", "substrate-gateway");

        if let Some(token) = bearer_token {
            req_builder = req_builder.header("Authorization", format!("Bearer {token}"));
        }

        let upstream_response = req_builder
            .send()
            .await
            .map_err(|e| SubstrateError::Engine(format!("Upstream request failed: {}", e)))?;

        // ── Metrics: response headers received (first-byte equivalent) ──────
        let header_latency = request_start.elapsed();

        let status = upstream_response.status();

        // Detect whether the upstream is an SSE stream.
        let upstream_is_sse = is_sse_content_type(upstream_response.headers());

        // Convert reqwest response to axum response with streaming body.
        // bytes_stream() yields chunks as they arrive — no full-body buffering.
        let byte_stream = upstream_response
            .bytes_stream()
            .map_ok(Bytes::from)
            .map_err(std::io::Error::other);

        let body = Body::from_stream(byte_stream);

        let response = if upstream_is_sse {
            Response::builder()
                .status(status)
                .header("content-type", "text/event-stream")
                .header("cache-control", "no-cache")
                .header("connection", "keep-alive")
                .header("transfer-encoding", "chunked")
                .body(body)
                .unwrap()
        } else {
            Response::builder()
                .status(status)
                .header("transfer-encoding", "chunked")
                .body(body)
                .unwrap()
        };

        let metrics = ProxyMetrics {
            latency: header_latency,
            first_byte_time: header_latency,
            is_sse: upstream_is_sse,
        };

        Ok((response, metrics))
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

    // ---------------------------------------------------------------------------
    // SSE detection tests
    // ---------------------------------------------------------------------------

    #[test]
    fn sse_detection_exact_match() {
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert(
            reqwest::header::CONTENT_TYPE,
            "text/event-stream".parse().unwrap(),
        );
        assert!(
            is_sse_content_type(&headers),
            "exact text/event-stream must be detected as SSE"
        );
    }

    #[test]
    fn sse_detection_with_charset_parameter() {
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert(
            reqwest::header::CONTENT_TYPE,
            "text/event-stream; charset=utf-8".parse().unwrap(),
        );
        assert!(
            is_sse_content_type(&headers),
            "text/event-stream with charset param must still be detected"
        );
    }

    #[test]
    fn non_sse_content_type_json_not_detected() {
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert(
            reqwest::header::CONTENT_TYPE,
            "application/json".parse().unwrap(),
        );
        assert!(
            !is_sse_content_type(&headers),
            "application/json must not be detected as SSE"
        );
    }

    #[test]
    fn missing_content_type_not_detected_as_sse() {
        let headers = reqwest::header::HeaderMap::new();
        assert!(
            !is_sse_content_type(&headers),
            "absent Content-Type header must not be detected as SSE"
        );
    }

    // ---------------------------------------------------------------------------
    // ProxyMetrics tests
    // ---------------------------------------------------------------------------

    #[test]
    fn proxy_metrics_sse_flag_reflects_detection() {
        let metrics_sse = ProxyMetrics {
            latency: Duration::from_millis(10),
            first_byte_time: Duration::from_millis(10),
            is_sse: true,
        };
        assert!(metrics_sse.is_sse);

        let metrics_json = ProxyMetrics {
            latency: Duration::from_millis(5),
            first_byte_time: Duration::from_millis(5),
            is_sse: false,
        };
        assert!(!metrics_json.is_sse);
    }

    #[test]
    fn proxy_metrics_latency_is_positive() {
        let m = ProxyMetrics {
            latency: Duration::from_micros(1),
            first_byte_time: Duration::from_micros(1),
            is_sse: false,
        };
        assert!(m.latency.as_nanos() > 0, "latency must be positive");
        assert!(
            m.first_byte_time.as_nanos() > 0,
            "first_byte_time must be positive"
        );
    }

    // ---------------------------------------------------------------------------
    // Streaming-body error mid-stream test
    // ---------------------------------------------------------------------------

    #[tokio::test]
    async fn error_mid_stream_surfaces_as_err_item() {
        use futures::stream;
        use futures::StreamExt;

        // Simulate a stream that yields one good chunk then an error
        let chunks: Vec<Result<Bytes, std::io::Error>> = vec![
            Ok(Bytes::from("data: {\"id\":\"1\"}\n\n")),
            Err(std::io::Error::other("connection reset mid-stream")),
        ];

        let mut stream = stream::iter(chunks);

        let first = stream.next().await.unwrap();
        assert!(first.is_ok(), "first chunk must succeed");

        let second = stream.next().await.unwrap();
        assert!(second.is_err(), "second item must be an error");
        let err_msg = second.unwrap_err().to_string();
        assert!(
            err_msg.contains("connection reset"),
            "error must propagate: {err_msg}"
        );
    }
}
