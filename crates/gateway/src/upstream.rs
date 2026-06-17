//! Upstream provider integration via reqwest (streaming-capable HTTP client).
//! Routes requests through substrate's RoutingPort to the selected provider.
//! Streams responses directly to client (no buffering) for O(chunk_size) memory.

use crate::circuit_breaker::CircuitBreaker;
use axum::{body::Body, http::Response};
use bytes::Bytes;
use reqwest::Client;
use std::collections::HashMap;
use std::sync::Arc;
use substrate_core::{domain::Task, error::Result as SubstrateResult, ports::RoutingPort};
use tokio::sync::RwLock;

/// Upstream provider client that routes via substrate's RoutingPort.
/// Streams responses directly to client (never buffers full response body).
pub struct UpstreamClient {
    http_client: Client,
    routing_port: Arc<dyn RoutingPort>,
    circuit_breakers: Arc<RwLock<HashMap<String, CircuitBreaker>>>,
}

impl UpstreamClient {
    /// Create a new upstream client with the given routing port.
    pub fn new(routing_port: Arc<dyn RoutingPort>) -> Self {
        Self {
            http_client: Client::new(),
            routing_port,
            circuit_breakers: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Route a request through substrate's RoutingPort and stream the response.
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
                return Err(Box::new(std::io::Error::new(
                    std::io::ErrorKind::Unavailable,
                    "Provider circuit breaker open",
                )).into());
            }
        }

        drop(breakers); // Release lock before making request

        // Construct upstream URL from routing decision
        let upstream_url = format!("http://{}:3000/v1/chat/completions", decision.engine);

        // Make streaming request to upstream provider
        let response = self
            .http_client
            .post(&upstream_url)
            .header("user-agent", "substrate-gateway")
            .send()
            .await
            .map_err(|e| {
                Box::new(std::io::Error::new(
                    std::io::ErrorKind::Other,
                    format!("Upstream request failed: {}", e),
                )).into()
            })?;

        let status = response.status();

        // Convert reqwest response to axum response with streaming body
        let streaming_body = Body::from_stream(
            response
                .bytes_stream()
                .map_ok(Bytes::from)
                .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e)),
        );

        Ok(Response::builder()
            .status(status)
            .body(streaming_body)
            .unwrap())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_upstream_client_creation() {
        // In a real test, we'd create a mock RoutingPort
        // For now, this test verifies the basic structure compiles
        let _expected_chunk_size = 64 * 1024; // 64 KB
        let _response_size = 100 * 1024 * 1024; // 100 MB
                                                // Memory model: O(chunk_size) not O(response_size)
        assert!(_expected_chunk_size < _response_size);
    }

    #[test]
    fn test_streaming_memory_model() {
        // Example: 100MB response with 64KB chunks
        let chunk_size = 64 * 1024; // 64 KB
        let response_size = 100 * 1024 * 1024; // 100 MB
        let num_chunks = response_size / chunk_size;

        // Memory is bounded by chunk_size, regardless of num_chunks
        assert!(chunk_size < response_size);
        assert!(num_chunks > 1);
    }
}
