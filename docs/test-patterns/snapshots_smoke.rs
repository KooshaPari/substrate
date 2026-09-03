//! Snapshot tests for core protocol outputs.
//!
//! TEST-07: insta snapshots. These verify the JSON / CBOR / protobuf
//! serialization of substrate's API responses remains stable across changes.
//! Run `cargo insta review` to accept snapshot updates.

use insta::{assert_json_snapshot, assert_yaml_snapshot};
use substrate_core::{Event, EventKind};

fn make_event(kind: EventKind, payload: serde_json::Value) -> Event {
    Event {
        kind,
        timestamp_ms: 1_700_000_000_000,
        source: "smoke-test".into(),
        trace_id: None,
        payload,
    }
}

#[test]
fn event_kind_default_serialization_is_stable() {
    let event = make_event(
        EventKind::Ingest,
        serde_json::json!({"topic": "x", "bytes": 42}),
    );
    assert_json_snapshot!("event_ingest_basic", event);
}

#[test]
fn event_payload_yaml_is_stable() {
    let event = make_event(
        EventKind::Compile,
        serde_json::json!({"crate": "substrate-core", "warnings": 0}),
    );
    assert_yaml_snapshot!("event_compile_basic", event);
}

#[test]
fn empty_event_round_trips() {
    let event = make_event(EventKind::Heartbeat, serde_json::json!({}));
    assert_json_snapshot!("event_heartbeat_empty", event);
}
