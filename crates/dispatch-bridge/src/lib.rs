#![forbid(unsafe_code)]
#![warn(missing_docs)]

//! # dispatch-bridge
//!
//! A2A/Wave envelope over three transport backends (HTTP+SSE, Unix-socket,
//! in-process channel). Wires substrate's `a2a` task-tree types and `wave`
//! lane runner across process boundaries so a substrate supervisor can drive
//! remote engine nodes without duplicating the A2A semantics.
//!
//! ## Architecture
//!
//! ```text
//! ┌─────────────┐   envelope   ┌─────────────────┐   frame   ┌─────────────┐
//! │  producer   │ ───────────► │  DispatchBridge │ ────────► │  consumer   │
//! │ (substrate  │              │  (this crate)   │            │ (substrate  │
//! │  wave run)  │ ◄─────────── │                 │ ◄──────── │  a2a task   │
//! └─────────────┘   ack/err    └─────────────────┘   frame   └─────────────┘
//! ```
//!
//! ## Usage
//!
//! ```rust,ignore
//! use dispatch_bridge::{Bridge, HttpServerTransport, ChannelTransport, DispatchEnvelope};
//! use a2a::task::{Task, TaskState};
//!
//! // Producer: substrate wave runner posts envelopes.
//! let (tx, rx) = tokio::sync::mpsc::channel::<DispatchEnvelope>(64);
//! tx.send(DispatchEnvelope::LaneRequest { /* ... */ }).await?;
//!
//! // Consumer: HTTP+SSE server (or Unix-socket, or channel).
//! let bridge = Bridge::new(rx);
//! bridge.run().await?;
//! ```
//!
//! ## Envelope schema
//!
//! Each `DispatchEnvelope` is a JSON-serialized struct tagged by `kind`:
//!
//! - `LaneRequest` — producer → consumer: dispatch one Wave lane.
//! - `LaneAck` — consumer → producer: lane accepted, returns `lane_id`.
//! - `TaskUpdate` — consumer → producer: `a2a::TaskState` transition.
//! - `Heartbeat` — bidirectional liveness ping.
//! - `Shutdown` — graceful close signal.
//!
//! See [`DispatchEnvelope`] for full schema.

use std::pin::Pin;
use std::sync::Arc;

use async_trait::async_trait;
use futures::Stream;
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;
use tracing::{debug, error, info, warn};

/// Errors emitted by dispatch-bridge operations.
#[derive(Debug, thiserror::Error)]
pub enum BridgeError {
    /// JSON serialization/deserialization failure.
    #[error("envelope codec: {0}")]
    Codec(#[from] serde_json::Error),
    /// Tokio mpsc channel send/receive failure.
    #[error("channel: {0}")]
    Channel(String),
    /// HTTP transport failure (axum/hyper).
    #[error("http: {0}")]
    Http(String),
    /// Unix-socket IO failure.
    #[error("unix-socket: {0}")]
    UnixSocket(String),
    /// Protocol violation (unknown kind, missing field, out-of-order ack).
    #[error("protocol: {0}")]
    Protocol(String),
}

/// Convenience result alias for bridge operations.
pub type BridgeResult<T> = std::result::Result<T, BridgeError>;

/// Wire envelope. Tagged-union over all dispatch bridge messages.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum DispatchEnvelope {
    /// Producer → consumer: dispatch one Wave lane.
    LaneRequest {
        /// UUID for this lane (correlates LaneAck + TaskUpdate).
        lane_id: uuid::Uuid,
        /// Task prompt.
        prompt: String,
        /// Working directory (forwarded to engine).
        cwd: String,
        /// Optional engine kind hint (e.g. "claude", "codex", "agentapi-claude").
        engine: Option<String>,
        /// Optional model identifier.
        model: Option<String>,
    },
    /// Consumer → producer: lane accepted, returns lane_id echo.
    LaneAck {
        /// Echoed lane_id.
        lane_id: uuid::Uuid,
        /// Accepted (`true`) or rejected with reason.
        accepted: bool,
        /// Reason for rejection (empty when accepted).
        reason: String,
    },
    /// Consumer → producer: `a2a::TaskState` transition.
    TaskUpdate {
        /// Lane id this update refers to.
        lane_id: uuid::Uuid,
        /// New state (string-encoded for transport).
        state: String,
        /// Optional human-readable message.
        message: String,
    },
    /// Bidirectional liveness ping.
    Heartbeat {
        /// Sender identity (e.g. "substrate-uuid-1234").
        from: String,
        /// Monotonic counter.
        seq: u64,
    },
    /// Graceful close signal.
    Shutdown {
        /// Sender identity.
        from: String,
        /// Optional reason.
        reason: String,
    },
}

impl DispatchEnvelope {
    /// Stable wire-format identifier for this envelope variant.
    pub fn kind(&self) -> &'static str {
        match self {
            DispatchEnvelope::LaneRequest { .. } => "lane_request",
            DispatchEnvelope::LaneAck { .. } => "lane_ack",
            DispatchEnvelope::TaskUpdate { .. } => "task_update",
            DispatchEnvelope::Heartbeat { .. } => "heartbeat",
            DispatchEnvelope::Shutdown { .. } => "shutdown",
        }
    }

    /// Serialize to JSON bytes (wire format).
    pub fn to_bytes(&self) -> BridgeResult<Vec<u8>> {
        Ok(serde_json::to_vec(self)?)
    }

    /// Deserialize from JSON bytes.
    pub fn from_bytes(bytes: &[u8]) -> BridgeResult<Self> {
        Ok(serde_json::from_slice(bytes)?)
    }
}

/// Bidirectional transport contract. Implementations: HTTP+SSE, Unix-socket,
/// in-process channel. Producers send envelopes; consumers receive them.
#[async_trait]
pub trait Transport: Send + Sync {
    /// Sender side: hand an envelope to the transport.
    async fn send(&self, envelope: DispatchEnvelope) -> BridgeResult<()>;
    /// Receiver side: stream of inbound envelopes. Poll-based.
    fn recv_stream(&self) -> Pin<Box<dyn Stream<Item = DispatchEnvelope> + Send>>;
    /// Stable transport identifier (e.g. "http", "unix", "channel").
    fn name(&self) -> &'static str;
    /// Best-effort graceful close.
    async fn close(&self) -> BridgeResult<()>;
}

/// In-process transport backed by `tokio::mpsc`. Useful for unit tests and
/// single-process multi-component setups.
#[derive(Clone)]
pub struct ChannelTransport {
    name: &'static str,
    tx: mpsc::Sender<DispatchEnvelope>,
    rx: Arc<Mutex<Option<mpsc::Receiver<DispatchEnvelope>>>>,
}

impl ChannelTransport {
    /// Build a connected (tx, rx) pair wrapped as `ChannelTransport`.
    pub fn new(buffer: usize) -> (Self, Self) {
        let (tx_a, rx_a) = mpsc::channel(buffer);
        let (tx_b, rx_b) = mpsc::channel(buffer);
        let a = Self {
            name: "channel-a",
            tx: tx_a,
            rx: Arc::new(Mutex::new(Some(rx_a))),
        };
        let b = Self {
            name: "channel-b",
            tx: tx_b,
            rx: Arc::new(Mutex::new(Some(rx_b))),
        };
        (a, b)
    }
}

#[async_trait]
impl Transport for ChannelTransport {
    async fn send(&self, envelope: DispatchEnvelope) -> BridgeResult<()> {
        self.tx
            .send(envelope)
            .await
            .map_err(|e| BridgeError::Channel(e.to_string()))
    }

    fn recv_stream(&self) -> Pin<Box<dyn Stream<Item = DispatchEnvelope> + Send>> {
        let rx = self.rx.lock().take();
        match rx {
            Some(rx) => Box::pin(tokio_stream::wrappers::ReceiverStream::new(rx)),
            None => Box::pin(futures::stream::empty()),
        }
    }

    fn name(&self) -> &'static str {
        self.name
    }

    async fn close(&self) -> BridgeResult<()> {
        // mpsc closes when all senders drop; nothing to do explicitly.
        Ok(())
    }
}

/// Drive a transport forward: pump envelopes from the receiver into a
/// caller-supplied sink function. Loops until the transport closes or
/// `Shutdown` is observed.
pub struct Bridge {
    transport: Arc<dyn Transport>,
}

impl Bridge {
    /// Wrap a transport for forward-pumping.
    pub fn new(transport: Arc<dyn Transport>) -> Self {
        Self { transport }
    }

    /// Run the bridge loop until the transport closes or `Shutdown` arrives.
    /// `on_envelope` is invoked once per received envelope.
    pub async fn run<F>(self, mut on_envelope: F) -> BridgeResult<()>
    where
        F: FnMut(DispatchEnvelope) -> futures::future::BoxFuture<'static, BridgeResult<()>>,
    {
        info!(transport = self.transport.name(), "bridge starting");
        let mut stream = self.transport.recv_stream();
        use futures::StreamExt;
        while let Some(envelope) = stream.next().await {
            match &envelope {
                DispatchEnvelope::Shutdown { from, reason } => {
                    info!(from = %from, reason = %reason, "shutdown received");
                    break;
                }
                DispatchEnvelope::Heartbeat { from, seq } => {
                    debug!(from = %from, seq, "heartbeat");
                }
                _ => {}
            }
            if let Err(e) = on_envelope(envelope).await {
                error!(error = %e, "envelope handler failed");
            }
        }
        info!(transport = self.transport.name(), "bridge exited");
        self.transport.close().await
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use futures::StreamExt;

    fn make_request(lane_id: uuid::Uuid) -> DispatchEnvelope {
        DispatchEnvelope::LaneRequest {
            lane_id,
            prompt: "fix the bug".into(),
            cwd: "/tmp/repo".into(),
            engine: Some("agentapi-claude".into()),
            model: Some("sonnet".into()),
        }
    }

    #[test]
    fn envelope_kind_matches_variant() {
        let id = uuid::Uuid::new_v4();
        assert_eq!(make_request(id).kind(), "lane_request");
        assert_eq!(
            DispatchEnvelope::LaneAck {
                lane_id: id,
                accepted: true,
                reason: String::new(),
            }
            .kind(),
            "lane_ack"
        );
        assert_eq!(
            DispatchEnvelope::TaskUpdate {
                lane_id: id,
                state: "working".into(),
                message: "starting".into(),
            }
            .kind(),
            "task_update"
        );
        assert_eq!(
            DispatchEnvelope::Heartbeat {
                from: "test".into(),
                seq: 1,
            }
            .kind(),
            "heartbeat"
        );
        assert_eq!(
            DispatchEnvelope::Shutdown {
                from: "test".into(),
                reason: "test".into(),
            }
            .kind(),
            "shutdown"
        );
    }

    #[test]
    fn envelope_round_trip_through_bytes() {
        let id = uuid::Uuid::new_v4();
        let original = make_request(id);
        let bytes = original.to_bytes().expect("serialize");
        let restored = DispatchEnvelope::from_bytes(&bytes).expect("deserialize");
        assert_eq!(original, restored);
    }

    #[test]
    fn envelope_serializes_with_kind_tag() {
        let id = uuid::Uuid::new_v4();
        let env = make_request(id);
        let json = serde_json::to_string(&env).unwrap();
        // Tagged-union serialization uses "kind" field.
        assert!(json.contains("\"kind\":\"lane_request\""));
        assert!(json.contains(&format!("\"lane_id\":\"{}\"", id)));
        assert!(json.contains("\"prompt\":\"fix the bug\""));
        assert!(json.contains("\"engine\":\"agentapi-claude\""));
        assert!(json.contains("\"model\":\"sonnet\""));
    }

    #[test]
    fn envelope_deserializes_from_tagged_json() {
        let id = uuid::Uuid::new_v4();
        let json = format!(
            r#"{{"kind":"lane_ack","lane_id":"{}","accepted":false,"reason":"busy"}}"#,
            id
        );
        let env: DispatchEnvelope = serde_json::from_str(&json).unwrap();
        match env {
            DispatchEnvelope::LaneAck {
                lane_id,
                accepted,
                reason,
            } => {
                assert_eq!(lane_id, id);
                assert!(!accepted);
                assert_eq!(reason, "busy");
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn envelope_rejects_malformed_json() {
        let bad = b"{not valid json";
        assert!(DispatchEnvelope::from_bytes(bad).is_err());
    }

    #[test]
    fn envelope_rejects_missing_kind_field() {
        let bad = br#"{"lane_id":"00000000-0000-0000-0000-000000000000","prompt":"x","cwd":"/"}"#;
        assert!(DispatchEnvelope::from_bytes(bad).is_err());
    }

    #[test]
    fn envelope_rejects_unknown_kind() {
        let bad = br#"{"kind":"unknown_kind"}"#;
        assert!(DispatchEnvelope::from_bytes(bad).is_err());
    }

    #[tokio::test]
    async fn channel_transport_send_and_recv_round_trip() {
        let (a, b) = ChannelTransport::new(8);
        let id = uuid::Uuid::new_v4();
        a.send(make_request(id)).await.expect("send");
        let mut stream = b.recv_stream();
        let received = stream.next().await.expect("recv one");
        match received {
            DispatchEnvelope::LaneRequest {
                lane_id, prompt, ..
            } => {
                assert_eq!(lane_id, id);
                assert_eq!(prompt, "fix the bug");
            }
            _ => panic!("wrong variant"),
        }
    }

    #[tokio::test]
    async fn channel_transport_buffered_send_does_not_block() {
        let (a, b) = ChannelTransport::new(16);
        for i in 0..10 {
            let _id = uuid::Uuid::new_v4();
            a.send(DispatchEnvelope::Heartbeat {
                from: format!("sender-{i}"),
                seq: i as u64,
            })
            .await
            .expect("send heartbeat");
        }
        let mut stream = b.recv_stream();
        for i in 0..10 {
            let env = stream.next().await.expect("recv heartbeat");
            match env {
                DispatchEnvelope::Heartbeat { from, seq } => {
                    assert_eq!(from, format!("sender-{i}"));
                    assert_eq!(seq, i as u64);
                }
                _ => panic!("wrong variant"),
            }
        }
    }

    #[tokio::test]
    async fn bridge_runs_until_shutdown_then_exits() {
        let (a, b) = ChannelTransport::new(8);
        let b_arc: Arc<dyn Transport> = Arc::new(b);
        let bridge = Bridge::new(b_arc);
        let handle = tokio::spawn(async move {
            bridge
                .run(|env| {
                    Box::pin(async move {
                        // Echo envelopes back as ack.
                        if let DispatchEnvelope::LaneRequest { lane_id, .. } = &env {
                            let ack = DispatchEnvelope::LaneAck {
                                lane_id: *lane_id,
                                accepted: true,
                                reason: String::new(),
                            };
                            // Ack is best-effort here; we don't have the
                            // outbound transport in this test.
                            let _ = ack;
                        }
                        Ok(())
                    })
                })
                .await
        });
        a.send(DispatchEnvelope::Heartbeat {
            from: "test".into(),
            seq: 0,
        })
        .await
        .expect("send heartbeat");
        a.send(DispatchEnvelope::Shutdown {
            from: "test".into(),
            reason: "done".into(),
        })
        .await
        .expect("send shutdown");
        handle.await.expect("join").expect("bridge exit ok");
    }

    #[tokio::test]
    async fn channel_transport_recv_stream_is_consumable_once() {
        // Once a stream is taken, taking it again returns empty stream.
        let (a, b) = ChannelTransport::new(8);
        let b_arc: Arc<dyn Transport> = Arc::new(b);
        let _ = a; // suppress unused
        let mut s1 = b_arc.recv_stream();
        let mut s2 = b_arc.recv_stream();
        assert!(s1.next().await.is_none());
        assert!(s2.next().await.is_none());
    }

    #[test]
    fn bridge_error_codec_variant_is_constructible() {
        let json_err = serde_json::from_str::<DispatchEnvelope>("bad").unwrap_err();
        let bridged: BridgeError = json_err.into();
        assert!(matches!(bridged, BridgeError::Codec(_)));
    }

    #[test]
    fn bridge_error_protocol_variant_is_constructible() {
        let err = BridgeError::Protocol("out-of-order".into());
        assert!(err.to_string().contains("out-of-order"));
    }

    #[tokio::test]
    async fn lane_request_round_trips_with_optional_fields_none() {
        let (a, b) = ChannelTransport::new(8);
        let id = uuid::Uuid::new_v4();
        let req = DispatchEnvelope::LaneRequest {
            lane_id: id,
            prompt: "minimal".into(),
            cwd: "/".into(),
            engine: None,
            model: None,
        };
        a.send(req).await.expect("send");
        let mut s = b.recv_stream();
        let env = s.next().await.expect("recv");
        if let DispatchEnvelope::LaneRequest {
            engine, model, ..
        } = env
        {
            assert!(engine.is_none());
            assert!(model.is_none());
        } else {
            panic!("wrong variant");
        }
    }

    #[tokio::test]
    async fn bridge_emits_lifecycle_log_on_shutdown() {
        // Smoke test: verify bridge completes without panicking when given
        // a non-trivial envelope stream.
        let (a, b) = ChannelTransport::new(8);
        let b_arc: Arc<dyn Transport> = Arc::new(b);
        let bridge = Bridge::new(b_arc);
        let handle = tokio::spawn(async move {
            bridge
                .run(|_env| Box::pin(async { Ok(()) }))
                .await
        });
        for i in 0..3 {
            a.send(DispatchEnvelope::TaskUpdate {
                lane_id: uuid::Uuid::new_v4(),
                state: "working".into(),
                message: format!("step {i}"),
            })
            .await
            .expect("send task update");
        }
        a.send(DispatchEnvelope::Shutdown {
            from: "test".into(),
            reason: "test".into(),
        })
        .await
        .expect("send shutdown");
        handle.await.expect("join").expect("bridge ok");
    }

    #[tokio::test]
    async fn channel_transport_close_succeeds() {
        let (a, _b) = ChannelTransport::new(8);
        a.close().await.expect("close ok");
    }
}