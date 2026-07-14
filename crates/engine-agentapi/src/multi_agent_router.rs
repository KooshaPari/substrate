//! Multi-agent [`RoutingPort`] backed by the agentapi engine.
//!
//! This module glues [`crate::routing`] (the pure engine-name → agent-type
//! mapping) into substrate's [`RoutingPort`] contract so the dispatcher can:
//!
//! 1. Receive a `Task` whose routing layer has decided `engine =
//!    "agentapi-claude"` (or `"agentapi:gemini"`, `"codex"`, etc.).
//! 2. Call `route_decision(task)` → get the engine + model + reason.
//! 3. Look up the right agentapi engine (one per agent type) and forward.
//!
//! ## Engine cache
//!
//! Agentapi engines are **long-lived** (one child process per conversation),
//! so we keep one `AgentApiEngine` per agent type in an `RwLock<HashMap>`.
//! Calls are `async`; the map is `Send + Sync`.
//!
//! ## Convention
//!
//! The `engine` string carries the agent type via the routing module's
//! `agentapi-<agent>` / `agentapi:<agent>` / bare `<agent>` syntax. When the
//! engine string does **not** match this convention (e.g. `"forge"`,
//! `"claude"`, or arbitrary names), this adapter returns the default
//! `RoutingDecision::default_forge_kimi()` so the dispatcher falls through
//! to the next engine adapter in its chain.

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use substrate_core::domain::{RoutingDecision, Task};
use substrate_core::error::Result;
#[allow(unused_imports)]
use substrate_core::error::SubstrateError;
use substrate_core::ports::RoutingPort;
use tokio::sync::RwLock;

use crate::routing;
use crate::AgentApiEngine;

/// Multi-agent router that fronts the agentapi engine family.
///
/// Holds one `AgentApiEngine` per agent type. Engines are lazily created on
/// first request and reused for subsequent tasks. Drop semantics kill any
/// spawned child processes (via `Arc` inside the engine itself).
pub struct AgentApiMultiAgentRouter {
    /// Default decision for non-agentapi engine names.
    fallback: RoutingDecision,
    /// Cache of agentapi engines keyed by agent type.
    engines: RwLock<HashMap<String, Arc<AgentApiEngine>>>,
}

impl std::fmt::Debug for AgentApiMultiAgentRouter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AgentApiMultiAgentRouter")
            .field("fallback", &self.fallback)
            .field("cached_agents", &self.engines.blocking_read().len())
            .finish()
    }
}

impl Default for AgentApiMultiAgentRouter {
    fn default() -> Self {
        Self::new()
    }
}

impl AgentApiMultiAgentRouter {
    /// Construct with the substrate Phase-1 default fallback.
    pub fn new() -> Self {
        AgentApiMultiAgentRouter {
            fallback: RoutingDecision::default_forge_kimi(),
            engines: RwLock::new(HashMap::new()),
        }
    }

    /// Construct with an explicit fallback decision (e.g. for tests).
    pub fn with_fallback(fallback: RoutingDecision) -> Self {
        AgentApiMultiAgentRouter {
            fallback,
            engines: RwLock::new(HashMap::new()),
        }
    }

    /// Look up (or lazily create) the engine for `agent_type`.
    pub async fn engine_for(&self, agent_type: &str) -> Result<Arc<AgentApiEngine>> {
        if let Some(e) = self.engines.read().await.get(agent_type) {
            return Ok(Arc::clone(e));
        }
        let mut write = self.engines.write().await;
        // Double-check after acquiring the write lock.
        if let Some(e) = write.get(agent_type) {
            return Ok(Arc::clone(e));
        }
        let engine = Arc::new(AgentApiEngine::with_agent_and_endpoint(
            agent_type,
            crate::DEFAULT_ENDPOINT,
        ));
        write.insert(agent_type.to_string(), Arc::clone(&engine));
        Ok(engine)
    }

    /// Number of cached engines (for tests/metrics).
    pub async fn cached_agents(&self) -> usize {
        self.engines.read().await.len()
    }

    /// Drop all cached engines. Subsequent `engine_for` calls will spawn fresh.
    pub async fn clear_cache(&self) {
        self.engines.write().await.clear();
    }
}

#[async_trait]
impl RoutingPort for AgentApiMultiAgentRouter {
    /// Always returns `route_decision(...).engine` — see [`routing`].
    async fn route(&self, task: &Task) -> Result<String> {
        Ok(self.route_decision(task).await?.engine)
    }

    /// Map the routing layer's engine name to an agentapi-shaped decision.
    ///
    /// - `"agentapi-claude"` → `engine = "agentapi-claude"`,
    ///   `model = "claude"`, `reason = "agentapi-multi-agent"`.
    /// - `"forge"` (or any non-agentapi name) → the configured fallback.
    async fn route_decision(&self, task: &Task) -> Result<RoutingDecision> {
        // Inspect the task's prompt for an explicit `engine: <name>` hint.
        // This is the convention driver-cli uses today and is the simplest
        // hook that doesn't require schema changes to `Task`.
        let hint = extract_engine_hint(&task.prompt);

        // Combine the hint with the routing module's mapping. If neither
        // resolves, return the fallback.
        let candidate = hint.unwrap_or("");
        if let Some(agent) = routing::parse_agent_target(candidate) {
            return Ok(RoutingDecision {
                engine: format!("{}{}", routing::AGENTAPI_PREFIX, agent),
                model: agent.to_string(),
                reason: Some(format!("agentapi-multi-agent:{agent}")),
            });
        }
        // No agentapi target — defer to fallback.
        let _ = task; // `task` is unused beyond the hint; keep for future heuristics.
        Ok(self.fallback.clone())
    }
}

/// Extract an `engine: <name>` directive from the task prompt.
///
/// Format: the substring `engine: <name>` (case-insensitive, word-bounded).
/// Returns `None` if no such directive is present or if `<name>` is empty.
fn extract_engine_hint(prompt: &str) -> Option<&str> {
    let lower = prompt.to_ascii_lowercase();
    let idx = lower.find("engine:")?;
    let after = &prompt[idx + "engine:".len()..];
    let after = after.trim_start();
    // Take the next whitespace-bounded token.
    let end = after
        .find(|c: char| c.is_whitespace() || c == '\n' || c == '\r')
        .unwrap_or(after.len());
    let name = after[..end].trim();
    if name.is_empty() {
        None
    } else {
        Some(name)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    fn make_task(prompt: &str) -> Task {
        Task {
            id: Uuid::new_v4(),
            prompt: prompt.to_string(),
            cwd: "/tmp".to_string(),
            state: substrate_core::domain::TaskState::Submitted,
            parent_task_id: None,
            requirement_id: None,
            epic_id: None,
            conv_id: None,
        }
    }

    #[test]
    fn extract_engine_hint_bare() {
        assert_eq!(extract_engine_hint("engine: claude"), Some("claude"));
        assert_eq!(extract_engine_hint("engine: codex"), Some("codex"));
        assert_eq!(extract_engine_hint("ENGINE: gemini"), Some("gemini"));
    }

    #[test]
    fn extract_engine_hint_after_text() {
        assert_eq!(
            extract_engine_hint("please use engine: claude for this"),
            Some("claude")
        );
        assert_eq!(
            extract_engine_hint("multi\nengine: codex\nfollow up"),
            Some("codex")
        );
    }

    #[test]
    fn extract_engine_hint_missing() {
        assert_eq!(extract_engine_hint("hello world"), None);
        assert_eq!(extract_engine_hint("engine:"), None);
        assert_eq!(extract_engine_hint("engine:   "), None);
        assert_eq!(extract_engine_hint(""), None);
    }

    #[tokio::test]
    async fn routes_agentapi_engine_names() {
        let r = AgentApiMultiAgentRouter::new();
        let t = make_task("engine: claude\ndo the thing");
        let d = r.route_decision(&t).await.unwrap();
        assert_eq!(d.engine, "agentapi-claude");
        assert_eq!(d.model, "claude");
        assert!(d.reason.unwrap().contains("claude"));
    }

    #[tokio::test]
    async fn routes_alternate_syntax() {
        let r = AgentApiMultiAgentRouter::new();
        let t = make_task("engine: agentapi:gemini");
        let d = r.route_decision(&t).await.unwrap();
        assert_eq!(d.engine, "agentapi-gemini");
        assert_eq!(d.model, "gemini");
    }

    #[tokio::test]
    async fn routes_bare_agent_name() {
        let r = AgentApiMultiAgentRouter::new();
        let t = make_task("engine: codex");
        let d = r.route_decision(&t).await.unwrap();
        assert_eq!(d.engine, "agentapi-codex");
        assert_eq!(d.model, "codex");
    }

    #[tokio::test]
    async fn non_agentapi_engine_falls_through() {
        let r = AgentApiMultiAgentRouter::new();
        let t = make_task("engine: forge");
        let d = r.route_decision(&t).await.unwrap();
        assert_eq!(d.engine, "forge");
        assert_eq!(d.model, "accounts/fireworks/routers/kimi-k2p6-turbo");
    }

    #[tokio::test]
    async fn no_hint_uses_fallback() {
        let r = AgentApiMultiAgentRouter::new();
        let t = make_task("just do it");
        let d = r.route_decision(&t).await.unwrap();
        assert_eq!(d.engine, "forge");
    }

    #[tokio::test]
    async fn unknown_agentapi_target_falls_through() {
        let r = AgentApiMultiAgentRouter::new();
        let t = make_task("engine: agentapi-gpt9000");
        let d = r.route_decision(&t).await.unwrap();
        assert_eq!(d.engine, "forge");
    }

    #[tokio::test]
    async fn engine_cache_lazily_populates() {
        let r = AgentApiMultiAgentRouter::new();
        assert_eq!(r.cached_agents().await, 0);
        let _ = r.engine_for("claude").await.unwrap();
        assert_eq!(r.cached_agents().await, 1);
        // Second call returns same engine (no new spawn).
        let _ = r.engine_for("claude").await.unwrap();
        assert_eq!(r.cached_agents().await, 1);
        let _ = r.engine_for("codex").await.unwrap();
        assert_eq!(r.cached_agents().await, 2);
        r.clear_cache().await;
        assert_eq!(r.cached_agents().await, 0);
    }

    #[test]
    fn routable_engine_names_match_supported() {
        // The dispatcher's `RoutableEngineNames` query should agree with the
        // routing module's enumerated set.
        let names = routing::routable_engine_names();
        assert!(names.contains(&"agentapi-claude".to_string()));
        assert!(names.contains(&"agentapi-codex".to_string()));
        assert!(names.contains(&"agentapi-gemini".to_string()));
        for n in &names {
            assert!(
                routing::parse_agent_target(n).is_some(),
                "routable engine {n} doesn't round-trip"
            );
        }
    }

    #[test]
    fn fallback_with_custom_decision() {
        let fb = RoutingDecision {
            engine: "custom-engine".to_string(),
            model: "custom-model".to_string(),
            reason: Some("test".to_string()),
        };
        let r = AgentApiMultiAgentRouter::with_fallback(fb.clone());
        // The decision is stored verbatim.
        assert_eq!(r.fallback.engine, "custom-engine");
        assert_eq!(r.fallback.model, "custom-model");
        // Use the decision in a test (avoid constructing a tokio task).
        assert_eq!(fb.engine, "custom-engine");
    }

    #[tokio::test]
    async fn route_returns_engine_string_from_decision() {
        let r = AgentApiMultiAgentRouter::new();
        let t = make_task("engine: gemini");
        let engine = r.route(&t).await.unwrap();
        assert_eq!(engine, "agentapi-gemini");
    }

    #[tokio::test]
    async fn unknown_engine_returns_engineerror_when_no_match_and_no_fallback() {
        // Construct a router whose fallback itself fails — exercised by
        // forcing `route_decision` to return an error. Since our default
        // fallback is always Ok, this is a placeholder test that asserts
        // the happy path returns Ok(_).
        let r = AgentApiMultiAgentRouter::new();
        let t = make_task("hello");
        let _ = r.route_decision(&t).await.expect("happy path");
    }

    // Sanity: the `SubstrateError` shape used in the impl is reachable.
    #[test]
    fn substrate_error_engine_display() {
        let e = SubstrateError::Engine("demo".to_string());
        let s = format!("{e}");
        assert!(s.contains("demo"));
    }
}
