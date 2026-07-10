//! Fuzz target: `POST /v1/chat/completions` body parser.
//!
//! `psub_gateway::openai::ChatCompletionRequest` is the entry point for
//! every inbound chat completion. A panic in its parser means a single
//! attacker-controlled request can take down the gateway.

#![no_main]
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    // serde_json::from_slice must never panic; any decoding error must
    // surface as Err, not as a panic / unreachable.
    let _ = serde_json::from_slice::<psub_gateway::openai::ChatCompletionRequest>(data);
});
