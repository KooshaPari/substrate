//! OpenAI-compatible upstream proxy with streaming passthrough.
//!
//! The client wraps `reqwest`, streams upstream SSE bodies directly into axum
//! responses, and trips a local circuit breaker on upstream 5xx responses.

use std::fmt;
use std::sync::{Arc, Mutex};

use axum::{body::Body, response::Response};
use bytes::Bytes;
use futures::TryStreamExt;
use reqwest::Client;
use serde_json::{Map, Value};

use crate::circuit_breaker::CircuitBreaker;

/// Streaming OpenAI-compatible upstream client.
#[derive(Clone)]
pub struct UpstreamClient {
    http_client: Client,
    base_url: String,
    api_key: String,
    breaker: Arc<Mutex<CircuitBreaker>>,
}

impl UpstreamClient {
    /// Create a new upstream client.
    pub fn new(base_url: impl Into<String>, api_key: impl Into<String>) -> Self {
        Self {
            http_client: Client::new(),
            base_url: base_url.into().trim_end_matches('/').to_string(),
            api_key: api_key.into(),
            breaker: Arc::new(Mutex::new(CircuitBreaker::with_failure_threshold(1))),
        }
    }

    /// Forward a chat-completions request and stream the upstream response body.
    ///
    /// The request body is forwarded as JSON, preserving any extra OpenAI
    /// fields the caller supplied. If `stream` is missing, it is added as
    /// `true` so the upstream emits SSE instead of buffering a full completion.
    pub async fn chat_completions(&self, body: Value) -> Result<Response, UpstreamError> {
        self.ensure_available()?;
        let body = Self::prepare_body(body)?;
        let response = self.send_chat_completions(body).await?;
        Ok(response)
    }

    fn ensure_available(&self) -> Result<(), UpstreamError> {
        if self.breaker.lock().unwrap().is_open() {
            return Err(UpstreamError::CircuitOpen);
        }
        Ok(())
    }

    fn prepare_body(mut body: Value) -> Result<Value, UpstreamError> {
        let map: &mut Map<String, Value> = body.as_object_mut().ok_or_else(|| {
            UpstreamError::BadRequest("chat completions body must be a JSON object".to_string())
        })?;
        map.entry("stream".to_string())
            .or_insert_with(|| Value::Bool(true));
        Ok(body)
    }

    async fn send_chat_completions(&self, body: Value) -> Result<Response, UpstreamError> {
        let url = format!("{}/chat/completions", self.base_url);
        let response = self
            .http_client
            .post(url)
            .bearer_auth(&self.api_key)
            .header("accept", "text/event-stream")
            .json(&body)
            .send()
            .await
            .map_err(UpstreamError::Request)?;

        let status = response.status();
        let headers = response.headers().clone();
        if status.is_server_error() {
            self.record_failure();
        } else {
            self.record_success();
        }

        let body = Body::from_stream(
            response
                .bytes_stream()
                .map_ok(Bytes::from)
                .map_err(std::io::Error::other),
        );

        let mut builder = Response::builder().status(status);
        for (name, value) in &headers {
            if !is_hop_by_hop(name) {
                builder = builder.header(name, value);
            }
        }
        if status.is_success() && headers.get(axum::http::header::CONTENT_TYPE).is_none() {
            builder = builder.header(axum::http::header::CONTENT_TYPE, "text/event-stream");
        }
        builder
            .header(axum::http::header::TRANSFER_ENCODING, "chunked")
            .body(body)
            .map_err(|err| UpstreamError::BuildResponse(err.to_string()))
    }

    fn record_failure(&self) {
        if let Ok(mut breaker) = self.breaker.lock() {
            breaker.record_failure();
        }
    }

    fn record_success(&self) {
        if let Ok(mut breaker) = self.breaker.lock() {
            breaker.record_success();
        }
    }
}

fn is_hop_by_hop(name: &axum::http::HeaderName) -> bool {
    matches!(
        name.as_str(),
        "connection"
            | "content-length"
            | "keep-alive"
            | "proxy-authenticate"
            | "proxy-authorization"
            | "te"
            | "trailers"
            | "transfer-encoding"
            | "upgrade"
    )
}

/// Errors emitted by the upstream proxy.
#[derive(Debug)]
pub enum UpstreamError {
    /// The breaker is open and the upstream should not be called.
    CircuitOpen,
    /// The request body is not a JSON object.
    BadRequest(String),
    /// The HTTP client failed before receiving a response.
    Request(reqwest::Error),
    /// Building the downstream response failed.
    BuildResponse(String),
}

impl fmt::Display for UpstreamError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            UpstreamError::CircuitOpen => write!(f, "upstream circuit breaker open"),
            UpstreamError::BadRequest(msg) => write!(f, "{msg}"),
            UpstreamError::Request(err) => write!(f, "upstream request failed: {err}"),
            UpstreamError::BuildResponse(msg) => write!(f, "failed to build response: {msg}"),
        }
    }
}

impl std::error::Error for UpstreamError {}
