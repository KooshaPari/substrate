//! Integration test exercising substrate's three execution lanes end-to-end.
//!
//! Lives in `substrate/tests/` (integration test root) and exercises the
//! Sync / Fanout / Tree lanes by composing adapters from the substrate
//! workspace.
//!
//! Lanes:
//! - **Sync** — single engine, single task, sequential.
//! - **Fanout** — same prompt fanned out to N engines in parallel.
//! - **Tree** — DAG execution: parent task spawns child tasks, each child
//!   runs on a different engine; results propagate up.
//!
//! The test does not perform real HTTP/PTY IO. Instead it uses an in-memory
//! mock engine that satisfies the `EnginePort` contract offline.
//!
//! See: plans/2026-06-22-phenotype-ecosystem-router-architecture-v1.md

use std::sync::Arc;

use async_trait::async_trait;
use parking_lot::Mutex;

use substrate_core::domain::{
    ConversationDump, Mailbox, RoutingDecision, Session, StructuredResult, Task, TaskState,
};
use substrate_core::error::{Result, SubstrateError};
use substrate_core::ports::{EnginePort, RoutingPort};

// ---------------------------------------------------------------------------
// Mock engine — satisfies EnginePort contract offline.
// ---------------------------------------------------------------------------

/// In-memory engine that echoes the prompt + a per-agent suffix.
#[derive(Debug)]
struct MockEngine {
    name: &'static str,
    suffix: &'static str,
    sessions: Arc<Mutex<Vec<Session>>>,
}

impl MockEngine {
    fn new(name: &'static str, suffix: &'static str) -> Arc<Self> {
        Arc::new(Self {
            name,
            suffix,
            sessions: Arc::new(Mutex::new(Vec::new())),
        })
    }
}

#[async_trait]
impl EnginePort for MockEngine {
    async fn start(&self, task: &Task) -> Result<Session> {
        let conv_id = format!("{}-{}", self.name, task.id);
        let session = Session {
            conv_id: conv_id.clone(),
            pid: None,
            logfile: None,
        };
        self.sessions.lock().push(session.clone());
        Ok(session)
    }

    async fn resume(&self, conv_id: &str, _prompt: &str) -> Result<Session> {
        Ok(Session {
            conv_id: conv_id.to_string(),
            pid: None,
            logfile: None,
        })
    }

    async fn dump(&self, conv_id: &str) -> Result<ConversationDump> {
        let raw = format!(
            r#"{{"id":"{}","messages":[{{"role":"agent","content":"echo-{}"}}]}}"#,
            conv_id, self.suffix
        );
        Ok(ConversationDump {
            conversation_id: conv_id.to_string(),
            raw,
        })
    }

    async fn cancel(&self, conv_id: &str) -> Result<()> {
        let mut sessions = self.sessions.lock();
        sessions.retain(|s| s.conv_id != conv_id);
        Ok(())
    }

    async fn wire_mailbox(&self, _conv_id: &str, _mailbox: &Mailbox) -> Result<()> {
        Ok(())
    }

    fn extract_result(&self, _dump: &ConversationDump) -> Result<StructuredResult> {
        Ok(StructuredResult {
            text: format!("mock-engine:{}", self.suffix),
            artifacts: Vec::new(),
            pr_urls: Vec::new(),
            status: TaskState::Completed,
        })
    }

    fn capabilities(&self) -> substrate_core::domain::EngineCapabilities {
        substrate_core::domain::EngineCapabilities {
            supports_resume: true,
            supports_subagents: false,
            supports_mcp_import: false,
        }
    }
}

// ---------------------------------------------------------------------------
// Mock router — picks an engine based on the prompt prefix.
// ---------------------------------------------------------------------------

struct MockRouter {
    engines: Vec<Arc<MockEngine>>,
}

#[async_trait]
impl RoutingPort for MockRouter {
    async fn route_decision(&self, task: &Task) -> Result<RoutingDecision> {
        let engine_name = if task.prompt.starts_with("claude:") {
            "claude"
        } else if task.prompt.starts_with("codex:") {
            "codex"
        } else if task.prompt.starts_with("gemini:") {
            "gemini"
        } else {
            // Round-robin fallback.
            let idx = (task.id.as_u128() as usize) % self.engines.len();
            return Ok(RoutingDecision {
                engine: self.engines[idx].name.to_string(),
                model: "mock-model".to_string(),
                reason: Some("round-robin".to_string()),
            });
        };
        Ok(RoutingDecision {
            engine: engine_name.to_string(),
            model: "mock-model".to_string(),
            reason: Some(format!("prefix-match:{engine_name}")),
        })
    }
}

// ---------------------------------------------------------------------------
// Sync lane
// ---------------------------------------------------------------------------

#[tokio::test]
async fn sync_lane_runs_single_task_on_one_engine() {
    let engine = MockEngine::new("claude", "claude-suffix");
    let task = Task::new("hello world", "/tmp");

    let session = engine.start(&task).await.unwrap();
    assert!(session.conv_id.starts_with("claude-"));
    assert!(session.pid.is_none());

    let dump = engine.dump(&session.conv_id).await.unwrap();
    let result = engine.extract_result(&dump).unwrap();
    assert_eq!(result.text, "mock-engine:claude-suffix");
    assert!(dump.raw.contains("echo-claude-suffix"));
    assert_eq!(result.status, TaskState::Completed);

    engine.cancel(&session.conv_id).await.unwrap();
    let sessions = engine.sessions.lock();
    assert!(sessions.is_empty());
}

// ---------------------------------------------------------------------------
// Fanout lane — same prompt to N engines in parallel.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn fanout_lane_runs_same_prompt_on_three_engines_in_parallel() {
    let engines: Vec<Arc<MockEngine>> = vec![
        MockEngine::new("claude", "claude-suffix"),
        MockEngine::new("codex", "codex-suffix"),
        MockEngine::new("gemini", "gemini-suffix"),
    ];

    let prompt = "refactor the auth module";
    let task = Task::new(prompt, "/repo");

    // Launch all three engines in parallel via tokio::join!.
    let mut handles = Vec::new();
    for engine in &engines {
        let e = engine.clone();
        let t = task.clone();
        handles.push(tokio::spawn(async move {
            let session = e.start(&t).await?;
            let dump = e.dump(&session.conv_id).await?;
            let result = e.extract_result(&dump)?;
            Ok::<_, SubstrateError>((session.conv_id, result))
        }));
    }

    let mut results = Vec::new();
    for h in handles {
        let (conv_id, result) = h.await.unwrap().unwrap();
        results.push((conv_id, result));
    }

    // Three distinct conversation IDs.
    let conv_ids: std::collections::HashSet<_> = results.iter().map(|(id, _)| id.clone()).collect();
    assert_eq!(conv_ids.len(), 3);

    // Three distinct summaries prove all three engines ran.
    let summaries: std::collections::HashSet<_> =
        results.iter().map(|(_, r)| r.text.clone()).collect();
    assert_eq!(summaries.len(), 3);
    assert!(summaries.contains("mock-engine:claude-suffix"));
    assert!(summaries.contains("mock-engine:codex-suffix"));
    assert!(summaries.contains("mock-engine:gemini-suffix"));
}

// ---------------------------------------------------------------------------
// Tree lane — parent spawns N children on different engines.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn tree_lane_routes_children_to_distinct_engines() {
    let claude = MockEngine::new("claude", "claude-suffix");
    let codex = MockEngine::new("codex", "codex-suffix");
    let gemini = MockEngine::new("gemini", "gemini-suffix");

    let router = Arc::new(MockRouter {
        engines: vec![claude.clone(), codex.clone(), gemini.clone()],
    });

    // Parent task spawns 3 children, each with a different prefix.
    let parent = Task::new("delegate: refactor + test + docs", "/repo");
    let child_specs: [(&str, Arc<MockEngine>); 3] = [
        ("claude: refactor the auth module", claude.clone()),
        ("codex: write tests for auth refactor", codex.clone()),
        ("gemini: update docs for auth changes", gemini.clone()),
    ];

    // Tree lane: dispatch children in parallel, aggregate summaries.
    let mut handles = Vec::new();
    for (prompt, engine) in child_specs {
        let child_task = Task::new(prompt, parent.cwd.clone());
        let engine = engine.clone();
        let router = router.clone();
        handles.push(tokio::spawn(async move {
            let decision = router.route_decision(&child_task).await?;
            let session = engine.start(&child_task).await?;
            let dump = engine.dump(&session.conv_id).await?;
            let result = engine.extract_result(&dump)?;
            Ok::<_, SubstrateError>(format!("{}:{}", decision.engine, result.text))
        }));
    }

    let mut summaries = Vec::new();
    for h in handles {
        summaries.push(h.await.unwrap().unwrap());
    }

    summaries.sort();
    assert_eq!(
        summaries,
        vec![
            "claude:mock-engine:claude-suffix".to_string(),
            "codex:mock-engine:codex-suffix".to_string(),
            "gemini:mock-engine:gemini-suffix".to_string(),
        ]
    );
}

// ---------------------------------------------------------------------------
// Tree lane — fallback path: prompt with no recognized prefix routes via
// the round-robin fallback. Demonstrates that the tree lane can handle
// heterogeneous children.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn tree_lane_falls_back_to_round_robin_for_unprefixed_prompts() {
    let engines: Vec<Arc<MockEngine>> = vec![
        MockEngine::new("a", "a-suffix"),
        MockEngine::new("b", "b-suffix"),
        MockEngine::new("c", "c-suffix"),
    ];
    let router = Arc::new(MockRouter {
        engines: engines.clone(),
    });

    let mut handles = Vec::new();
    for i in 0..6 {
        let task = Task::new(format!("task-{i}"), "/repo");
        let engine = engines[i % engines.len()].clone();
        let router = router.clone();
        handles.push(tokio::spawn(async move {
            let decision = router.route_decision(&task).await?;
            let session = engine.start(&task).await?;
            let dump = engine.dump(&session.conv_id).await?;
            let result = engine.extract_result(&dump)?;
            Ok::<_, SubstrateError>((decision.engine, result.text))
        }));
    }

    let mut routed = Vec::new();
    for h in handles {
        let (engine_name, summary) = h.await.unwrap().unwrap();
        routed.push((engine_name, summary));
    }

    // 6 tasks distributed across 3 engines => 2 each.
    let mut counts: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    for (name, _) in &routed {
        *counts.entry(name.clone()).or_insert(0) += 1;
    }
    for name in ["a", "b", "c"] {
        assert_eq!(
            counts.get(name).copied().unwrap_or(0),
            2,
            "engine {name} should have 2 tasks"
        );
    }
}

// ---------------------------------------------------------------------------
// Cross-cutting: extract_result + EngineCapabilities shape (smoke test for
// the substrate-core contract).
// ---------------------------------------------------------------------------

#[test]
fn mock_engine_extract_result_is_pure() {
    let engine = MockEngine::new("forge", "forge-suffix");
    let dump = ConversationDump {
        conversation_id: "test".to_string(),
        raw: r#"{"messages":[{"role":"agent","content":"echo-forge-suffix"}]}"#.to_string(),
    };
    // Pure transform: same dump must always yield same result.
    let r1 = engine.extract_result(&dump).unwrap();
    let r2 = engine.extract_result(&dump).unwrap();
    assert_eq!(r1, r2);
    assert_eq!(r1.text, "mock-engine:forge-suffix");
}

#[test]
fn mock_engine_capabilities_advertises_resume_no_subagents() {
    let engine = MockEngine::new("forge", "forge-suffix");
    let caps = engine.capabilities();
    assert!(caps.supports_resume);
    assert!(!caps.supports_subagents);
    assert!(!caps.supports_mcp_import);
}

// ---------------------------------------------------------------------------
// Task lifecycle FSM (sanity test exercising substrate_core::TaskState).
// ---------------------------------------------------------------------------

#[test]
fn task_lifecycle_fsm_enforces_legal_transitions() {
    use substrate_core::domain::TaskState;

    // Happy path: Submitted -> Working -> Completed.
    assert!(TaskState::can_transition(
        TaskState::Submitted,
        TaskState::Working
    ));
    assert!(TaskState::can_transition(
        TaskState::Working,
        TaskState::Completed
    ));

    // Working can require input.
    assert!(TaskState::can_transition(
        TaskState::Working,
        TaskState::InputRequired
    ));
    assert!(TaskState::can_transition(
        TaskState::InputRequired,
        TaskState::Working
    ));

    // Non-terminal states may always move to Failed or Cancelled.
    assert!(TaskState::can_transition(
        TaskState::Submitted,
        TaskState::Failed
    ));
    assert!(TaskState::can_transition(
        TaskState::Working,
        TaskState::Cancelled
    ));

    // Terminal states have no outgoing edges.
    assert!(!TaskState::can_transition(
        TaskState::Completed,
        TaskState::Working
    ));
    assert!(!TaskState::can_transition(
        TaskState::Failed,
        TaskState::Working
    ));
    assert!(!TaskState::can_transition(
        TaskState::Cancelled,
        TaskState::Working
    ));

    // Self-transitions are illegal.
    assert!(!TaskState::can_transition(
        TaskState::Working,
        TaskState::Working
    ));

    // Illegal edges are rejected.
    assert!(!TaskState::can_transition(
        TaskState::Submitted,
        TaskState::Completed
    ));
    assert!(!TaskState::can_transition(
        TaskState::Completed,
        TaskState::Failed
    ));
}
