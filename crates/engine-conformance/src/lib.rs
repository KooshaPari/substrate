//! # engine-conformance
//!
//! A reusable contract-test suite for [`substrate_core::ports::EnginePort`].
//!
//! Call [`assert_engine_conformance`] from any adapter crate's test suite to
//! verify that the adapter satisfies the harness-agnostic contract without
//! making real network or process calls.
//!
//! ## What is tested
//!
//! 1. `start()` returns a session with a non-empty `conv_id`.
//! 2. `dump()` returns a `ConversationDump` whose `conversation_id` matches
//!    what `start()` gave back.
//! 3. `extract_result()` succeeds and the status is a valid [`TaskState`].
//! 4. `resume()` returns a session reusing the same `conv_id`.
//! 5. `cancel()` completes without error.
//! 6. `wire_mailbox()` completes without error.
//! 7. `capabilities()` returns a structurally valid value.
#![forbid(unsafe_code)]
#![warn(missing_docs)]

use substrate_core::domain::{Mailbox, Task, TaskState};
use substrate_core::ports::EnginePort;

/// Run the full conformance suite against `engine`.
///
/// Each assertion is described in the crate-level doc. This function is
/// intentionally synchronous at the call-site signature level — the caller
/// must drive it from a `#[tokio::test]` context.
///
/// # Panics
///
/// Panics with a descriptive message on the first failing assertion.
pub async fn assert_engine_conformance<E: EnginePort>(engine: &E) {
    let task = Task::new("conformance probe", "/tmp");

    // 1. start() → non-empty conv_id
    let session = engine
        .start(&task)
        .await
        .expect("conformance: start() must succeed");
    assert!(
        !session.conv_id.is_empty(),
        "conformance: start() must return a non-empty conv_id"
    );

    let conv_id = session.conv_id.clone();

    // 2. dump() → conversation_id matches
    let dump = engine
        .dump(&conv_id)
        .await
        .expect("conformance: dump() must succeed");
    assert_eq!(
        dump.conversation_id, conv_id,
        "conformance: dump().conversation_id must match session.conv_id"
    );

    // 3. extract_result() → succeeds and returns a valid TaskState
    let result = engine
        .extract_result(&dump)
        .expect("conformance: extract_result() must succeed on the dump");
    let valid = matches!(
        result.status,
        TaskState::Completed | TaskState::Failed | TaskState::Working
    );
    assert!(
        valid,
        "conformance: extract_result().status must be a plausible terminal/live state, got {result:?}"
    );

    // 4. resume() → same conv_id echoed back
    let resumed = engine
        .resume(&conv_id, "follow-up")
        .await
        .expect("conformance: resume() must succeed");
    assert_eq!(
        resumed.conv_id, conv_id,
        "conformance: resume() must return the same conv_id"
    );

    // 5. cancel() → no error
    engine
        .cancel(&conv_id)
        .await
        .expect("conformance: cancel() must succeed");

    // 6. wire_mailbox() → no error
    let mailbox = Mailbox {
        owner: "conformance-test".to_string(),
        messages: vec![],
    };
    engine
        .wire_mailbox(&conv_id, &mailbox)
        .await
        .expect("conformance: wire_mailbox() must succeed");

    // 7. capabilities() → structurally valid (no panic)
    let caps = engine.capabilities();
    // Just ensure the fields are readable; the values are adapter-specific.
    let _ = caps.supports_resume;
    let _ = caps.supports_subagents;
    let _ = caps.supports_mcp_import;
}
