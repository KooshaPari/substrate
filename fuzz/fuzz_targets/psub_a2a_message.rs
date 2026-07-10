//! Fuzz target: `psub_a2a::Message` round-trip.
//!
//! Generate a `Message` with random UUIDs / strings / arbitrary nested
//! `Part` payloads, serialize to bytes, parse back, and verify the
//! reconstructed value `==` the original. Catches UB in `serde_json`
//! custom code and any non-UTF-8 leaks in the `Part::File` URI branch.

#![no_main]
use libfuzzer_sys::fuzz_target;
use psub_a2a::message::{Message, MessageKind, MsgState, Part};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

fuzz_target!(|data: &[u8]| {
    // Decode a fuzzed JSON payload into a Message. We never panic here;
    // any decoding error must propagate as `Err`.
    let parsed: Result<Message, _> = serde_json::from_slice(data);
    if let Ok(msg) = parsed {
        // Round-trip: serialization must produce equivalent bytes.
        let bytes = serde_json::to_vec(&msg).expect("re-serialize");
        let back: Message = serde_json::from_slice(&bytes)
            .expect("re-deserialize of a previously-valid payload");
        assert_eq!(msg, back);
    }
});

#[derive(Debug, Clone, Serialize, Deserialize)]
struct _Help {
    m: MessageKind,
    p: Part,
    s: MsgState,
    id: Uuid,
}
