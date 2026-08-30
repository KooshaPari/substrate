//! Incremental streaming token accounting.
//!
//! Providers that return an SSE stream rarely emit per-sse-`usage` on every
//! chunk, and many gateways only receive the final usage payload (or none at
//! all) at the end of the stream.  As a result `prompt_tokens` /
//! `completion_tokens` were previously recorded as `0` for the entire *duration*
//! of a streaming session — the budget tracker only saw a final (empty) count.
//!
//! This module fixes that by:
//! 1. Accounting **incrementally** as content deltas arrive, using a cheap
//!    heuristic (`estimate_tokens`) when the provider does not report usage.
//! 2. Recording the estimated counts to the budget store *chunk-by-chunk* so a
//!    long-lived streaming session is charged as it runs, not only at the end.
//! 3. Overriding the running estimate with the provider's **exact** final usage
//!    payload (`apply_final_usage`) when one is present.
//!
//! Design note: token-heavy cultures (CJK etc.) would be more faithfully
//! counted on byte alignment, but a whitespace heuristic is intentionally
//! lightweight and dependency-free — the exact usage, when the upstream
//! supplies one, always wins over the estimate.

use super::budget::{estimate_cost, BudgetStore};
use bytes::Bytes;
use futures::stream::Stream;
use std::pin::Pin;
use std::task::{Context, Poll};

/// Estimate the number of tokens a text fragment contributes.
///
/// Uses the common *~4 bytes ≈ 1 token* approximation, rounded up so a single
/// non-empty fragment still counts as at least one token.  This is deliberately
/// cheap (no word-splitting allocation) and is only ever a fallback for
/// providers that omit `usage` in their stream chunks.
pub fn estimate_tokens(text: &str) -> u64 {
    if text.is_empty() {
        return 0;
    }
    // bytes/4 ≈ tokens; clamp to a minimum of 1 for non-empty fragments.
    ((text.len() as u64) / 4).max(1)
}

/// Estimate completion tokens emitted by a single SSE content delta.
///
/// `None` deltas (role frames, empty `finish_reason` frames) contribute zero.
pub fn estimate_completion_tokens(delta_content: Option<&str>) -> u64 {
    delta_content.map_or(0, estimate_tokens)
}

/// Accumulate one streaming content delta against the estimate.
///
/// Returns `None` when the delta carries no text (role / finish frames).
pub fn record_stream_delta(
    store: &BudgetStore,
    session_id: &str,
    model: &str,
    delta_content: Option<&str>,
) -> Option<u64> {
    let Some(delta) = delta_content else {
        return None;
    };
    let tokens = estimate_tokens(delta);
    if tokens > 0 {
        let cost = estimate_cost(model, 0, tokens);
        super::budget::record_usage(store, session_id, tokens, cost);
    }
    Some(tokens)
}

/// Override the running heuristic estimate with the provider's exact usage.
///
/// When an upstream stream ends with an exact `usage` payload (as OpenAI streams
/// do), the running heuristic estimate is discarded and the session budget is
/// reconciled to the true `prompt + completion` token count and the exact cost.
///
/// Returns the reconciled `(prompt_tokens, completion_tokens)` tuple.
pub fn apply_final_usage(
    store: &BudgetStore,
    session_id: &str,
    model: &str,
    prompt_tokens: u64,
    completion_tokens: u64,
) -> (u64, u64) {
    let total = prompt_tokens.saturating_add(completion_tokens);
    let exact_cost = estimate_cost(model, prompt_tokens, completion_tokens);

    // Discard the heuristic estimate, then record the exact usage in one step
    // so the session reflects true consumption, not the running estimate.
    super::budget::reset_session(store, session_id);
    super::budget::record_usage(store, session_id, total, exact_cost);

    (prompt_tokens, completion_tokens)
}

// ---------------------------------------------------------------------------
// Stream wrapper
// ---------------------------------------------------------------------------

/// A parsed SSE record we care about for accounting.
#[derive(Debug, Default, Clone)]
struct SseRecord {
    /// `choices[0].delta.content`, if present.
    delta_content: Option<String>,
    /// Token `usage`, if the final chunk carries one.
    usage: Option<(u64, u64)>,
}

/// Parse a single `data: {...}` JSON payload into an [`SseRecord`].
///
/// Non-JSON / heartbeat `data:` lines and `[DONE]` are ignored gracefully.
fn parse_usage_sse(data: &str) -> SseRecord {
    let trimmed = data.trim();
    if trimmed.is_empty() || trimmed == "[DONE]" {
        return SseRecord::default();
    }
    let v: serde_json::Value = match serde_json::from_str(trimmed) {
        Ok(v) => v,
        Err(_) => return SseRecord::default(),
    };

    let delta_content = v["choices"][0]["delta"]["content"]
        .as_str()
        .map(|s| s.to_string());

    // usage: { "prompt_tokens": N, "completion_tokens": M }
    let usage = match (
        v["usage"]["prompt_tokens"].as_u64(),
        v["usage"]["completion_tokens"].as_u64(),
    ) {
        (Some(p), Some(c)) => Some((p, c)),
        _ => None,
    };

    SseRecord {
        delta_content,
        usage,
    }
}

/// State shared by a live [`CountingStream`] (moved into each poll via `Arc`).
struct CounterState {
    store: BudgetStore,
    session_id: String,
    model: String,
}

/// A [`Stream`] wrapper that forwards a byte stream unchanged while
/// accounting for token usage incrementally.
///
/// - Each SSE `data:` record is inspected for a content delta; the delta's
///   heuristic token estimate is recorded to the budget store.
/// - When a record carries an exact `usage` payload, the running estimate is
///   replaced by the exact `prompt + completion` count.
pub struct CountingStream<S> {
    inner: S,
    state: CounterState,
    /// Partial SSE text accumulated across chunk boundaries.
    buf: String,
}

impl<S> CountingStream<S> {
    /// Wrap `inner`, recording accounting against `session_id` for `model`.
    pub fn new(inner: S, store: BudgetStore, session_id: impl Into<String>, model: impl Into<String>) -> Self {
        Self {
            inner,
            state: CounterState {
                store,
                session_id: session_id.into(),
                model: model.into(),
            },
            buf: String::new(),
        }
    }
}

impl<S> Stream for CountingStream<S>
where
    S: Stream<Item = Result<Bytes, std::io::Error>> + Unpin,
{
    type Item = Result<Bytes, std::io::Error>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        match Pin::new(&mut self.inner).poll_next(cx) {
            Poll::Ready(Some(Ok(bytes))) => {
                // Append to the streaming buffer and process complete records.
                self.buf.push_str(&String::from_utf8_lossy(&bytes));
                while let Some(record) = take_next_record(&mut self.buf) {
                    // 1. Incremental estimation of content delta.
                    if let Some(delta) = record.delta_content {
                        record_stream_delta(
                            &self.state.store,
                            &self.state.session_id,
                            &self.state.model,
                            Some(&delta),
                        );
                    }
                    // 2. Exact usage overrides the estimate.
                    if let Some((p, c)) = record.usage {
                        apply_final_usage(
                            &self.state.store,
                            &self.state.session_id,
                            &self.state.model,
                            p,
                            c,
                        );
                    }
                }
                Poll::Ready(Some(Ok(bytes)))
            }
            Poll::Ready(other) => {
                self.buf.clear();
                Poll::Ready(other)
            }
            Poll::Pending => Poll::Pending,
        }
    }
}

/// Extract the first complete `data: <json>` record from `buf`.
fn take_next_record(buf: &mut String) -> Option<SseRecord> {
    let end = buf.find("\n\n")?;
    let record = buf[..end].to_string();
    buf.drain(..end + 2);

    let mut payload: Option<String> = None;
    for line in record.lines() {
        if let Some(rest) = line.strip_prefix("data:") {
            let piece = rest.trim();
            if piece.is_empty() {
                continue;
            }
            if piece == "[DONE]" {
                return Some(SseRecord::default());
            }
            // Concatenate multi-line data payloads.
            match payload.as_mut() {
                Some(p) => {
                    p.push('\n');
                    p.push_str(piece);
                }
                None => payload = Some(piece.to_string()),
            }
        }
    }

    let record = match payload {
        Some(payload) => parse_usage_sse(&payload),
        None => SseRecord::default(),
    };
    Some(record)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::budget::BudgetConfig;

    #[test]
    fn estimate_tokens_empty_is_zero() {
        assert_eq!(estimate_tokens(""), 0);
    }

    #[test]
    fn estimate_tokens_approximates_bytes_over_four() {
        // "hello " is 6 bytes -> 6/4 = 1
        assert_eq!(estimate_tokens("hello "), 1);
        // 20 bytes -> 20/4 = 5
        assert_eq!(estimate_tokens("abcdefghijklmnopqrst"), 5);
    }

    #[test]
    fn estimate_tokens_minimum_one_for_non_empty() {
        assert_eq!(estimate_tokens("a"), 1);
    }

    #[test]
    fn estimate_completion_tokens_none_is_zero() {
        assert_eq!(estimate_completion_tokens(None), 0);
    }

    #[test]
    fn record_stream_delta_counts_incrementally() {
        let store = BudgetStore::new();
        // Three deltas -> tokens should accumulate and be non-zero.
        record_stream_delta(&store, "sess", "gpt-4o", Some("hello "));
        record_stream_delta(&store, "sess", "gpt-4o", Some("world"));
        record_stream_delta(&store, "sess", "gpt-4o", Some("!!!"));
        let snap = crate::budget::get_session(&store, "sess").expect("session recorded");
        assert!(
            snap.tokens_used > 0,
            "streaming deltas must produce a non-zero incremental token count, got {}",
            snap.tokens_used
        );
    }

    #[test]
    fn record_stream_delta_none_does_not_add() {
        let store = BudgetStore::new();
        record_stream_delta(&store, "sess", "m", None);
        assert!(crate::budget::get_session(&store, "sess").is_none());
    }

    #[test]
    fn apply_final_usage_overrides_estimate() {
        let store = BudgetStore::new();
        // Simulate a streaming session that accumulated an estimate.
        record_stream_delta(&store, "sess", "gpt-4o", Some("hello "));
        record_stream_delta(&store, "sess", "gpt-4o", Some("world"));

        let (p, c) = apply_final_usage(&store, "sess", "gpt-4o", 10, 7);
        assert_eq!((p, c), (10, 7));

        let snap = crate::budget::get_session(&store, "sess").expect("session recorded");
        assert_eq!(
            snap.tokens_used, 17,
            "final exact usage (10 prompt + 7 completion) must override the estimate"
        );
    }

    #[test]
    fn apply_final_usage_reconciles_cost() {
        let store = BudgetStore::new();
        let config = BudgetConfig {
            max_tokens_per_session: Some(1_000),
            max_cost_usd_per_session: None,
        };
        record_stream_delta(&store, "sess", "gpt-4o", Some("a fairly long streamed fragment"));

        // Only the final exact usage should be charged.
        apply_final_usage(&store, "sess", "gpt-4o", 5, 3);

        // Cost for 5 prompt + 3 completion on gpt-4o must equal the exact cost.
        let snap = crate::budget::get_session(&store, "sess").unwrap();
        let exact = estimate_cost("gpt-4o", 5, 3);
        assert!((snap.cost_usd - exact).abs() < 1e-9);
        assert!(crate::budget::check_budget(&store, "sess", &config).is_ok());
    }

    // --- CountingStream (stream wrapper) ---

    fn sse_content_delta(text: &str) -> Bytes {
        let payload = serde_json::json!({
            "id": "chatcmpl-stream",
            "object": "chat.completion.chunk",
            "model": "gpt-4o",
            "choices": [{ "index": 0, "delta": { "content": text }, "finish_reason": null }]
        });
        Bytes::from(format!("data: {payload}\n\n"))
    }

    fn sse_usage_delta(prompt: u64, completion: u64) -> Bytes {
        let payload = serde_json::json!({
            "id": "chatcmpl-stream",
            "object": "chat.completion.chunk",
            "model": "gpt-4o",
            "choices": [],
            "usage": { "prompt_tokens": prompt, "completion_tokens": completion }
        });
        Bytes::from(format!("data: {payload}\n\n"))
    }

    #[tokio::test]
    async fn counting_stream_produces_non_zero_estimate() {
        use futures::stream;
        use futures::StreamExt;

        let stream = stream::iter(vec![
            Ok(sse_content_delta("Hello, ")),
            Ok(sse_content_delta("world!")),
            Ok(sse_content_delta(" How are you today?")),
            Ok(Bytes::from("data: [DONE]\n\n")),
        ]);
        let store = BudgetStore::new();
        let mut counted = CountingStream::new(stream, store.clone(), "sess", "gpt-4o");

        while let Some(chunk) = counted.next().await {
            let _ = chunk.expect("chunk ok");
        }

        let snap = crate::budget::get_session(&store, "sess").expect("session recorded");
        assert!(
            snap.tokens_used > 0,
            "a simulated stream of chunks must yield a non-zero incremental token estimate, got {}",
            snap.tokens_used
        );
        assert!(
            snap.cost_usd > 0.0,
            "incremental estimate must also accrue a non-zero cost"
        );
    }

    #[tokio::test]
    async fn counting_stream_final_usage_overrides_estimate() {
        use futures::stream;
        use futures::StreamExt;

        // Long text that would produce a large heuristic estimate ...
        let long_delta = sse_content_delta(&"w".repeat(4000)); // ~1000 estimated tokens
        let stream = stream::iter(vec![
            Ok(long_delta),
            Ok(sse_content_delta(", trailing")),
            // ... then an exact final usage that is much smaller than the estimate.
            Ok(sse_usage_delta(12, 5)),
            Ok(Bytes::from("data: [DONE]\n\n")),
        ]);
        let store = BudgetStore::new();
        let mut counted = CountingStream::new(stream, store.clone(), "sess", "gpt-4o");

        while let Some(chunk) = counted.next().await {
            let _ = chunk.expect("chunk ok");
        }

        let snap = crate::budget::get_session(&store, "sess").expect("session recorded");
        assert_eq!(
            snap.tokens_used, 17,
            "exact final usage (12 prompt + 5 completion) must override the heuristic estimate"
        );
        let exact = estimate_cost("gpt-4o", 12, 5);
        assert!(
            (snap.cost_usd - exact).abs() < 1e-9,
            "cost must be reconciled to exact usage"
        );
    }

    #[tokio::test]
    async fn counting_stream_buffers_across_chunk_boundaries() {
        use futures::stream;
        use futures::StreamExt;

        // Split one SSE record across two byte chunks to exercise buffering.
        let full = sse_content_delta("chunk-boundary-test");
        let full = format!("{}", String::from_utf8_lossy(&full));
        let (a, b) = full.split_at(full.len() / 2);
        let stream = stream::iter(vec![
            Ok(Bytes::from(a.to_string())),
            Ok(Bytes::from(b.to_string())),
            Ok(Bytes::from("data: [DONE]\n\n")),
        ]);
        let store = BudgetStore::new();
        let mut counted = CountingStream::new(stream, store.clone(), "sess", "gpt-4o");

        while let Some(chunk) = counted.next().await {
            let _ = chunk.expect("chunk ok");
        }

        let snap = crate::budget::get_session(&store, "sess").expect("session recorded");
        assert!(snap.tokens_used > 0, "record split across chunks must still be counted");
    }
}