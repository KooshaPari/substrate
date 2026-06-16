//! Golden tests: recorded forge dumps normalize to the expected result.

use substrate_core::domain::{ConversationDump, TaskState};
use substrate_core::ports::EnginePort;

fn load(name: &str) -> ConversationDump {
    let raw = std::fs::read_to_string(format!("tests/fixtures/{name}")).unwrap();
    ConversationDump {
        conversation_id: "fixture".into(),
        raw,
    }
}

#[test]
fn dump_without_pr_yields_completed_text_no_urls() {
    let engine = engine_forge::ForgeEngine::with_bin("unused");
    let result = engine.extract_result(&load("dump_no_pr.json")).unwrap();

    assert_eq!(result.text, "Sure. DONE: printed hi to stdout.");
    assert!(result.pr_urls.is_empty());
    assert_eq!(result.status, TaskState::Completed);
    assert!(result.artifacts.is_empty());
}

#[test]
fn dump_with_pr_extracts_url_and_artifact() {
    let engine = engine_forge::ForgeEngine::with_bin("unused");
    let result = engine.extract_result(&load("dump_with_pr.json")).unwrap();

    assert_eq!(
        result.pr_urls,
        vec!["https://github.com/KooshaPari/substrate/pull/1".to_string()]
    );
    assert_eq!(result.status, TaskState::Completed);
    assert_eq!(result.artifacts.len(), 1);
}

#[test]
fn dump_with_max_steps_marks_failed_even_with_pr() {
    let engine = engine_forge::ForgeEngine::with_bin("unused");
    let result = engine.extract_result(&load("dump_max_steps.json")).unwrap();

    // "max steps" is a soft failure, even when a PR URL was captured.
    assert_eq!(result.status, TaskState::Failed);
    assert_eq!(
        result.pr_urls,
        vec!["https://github.com/KooshaPari/substrate/pull/2".to_string()]
    );
}

#[test]
fn dump_with_nonzero_exit_marks_failed() {
    let engine = engine_forge::ForgeEngine::with_bin("unused");
    let result = engine
        .extract_result(&load("dump_nonzero_exit.json"))
        .unwrap();

    assert_eq!(result.status, TaskState::Failed);
    assert!(result.pr_urls.is_empty());
}
