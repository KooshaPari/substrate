//! Routing seam: map a `RoutingDecision.engine` value to a configured
//! [`AgentApiEngine`].
//!
//! `RoutingPort::route_decision` returns an `engine` string. When the engine
//! name carries an `agentapi-<agent>` prefix, the dispatcher hands it to the
//! agentapi engine, which forwards to the agentapi-plusplus child process for
//! that agent type. The mapping is intentionally tiny and pure: no IO, no
//! allocation of ports, no env-var reads. All process management stays in
//! [`crate::AgentApiEngine::new`].
//!
//! Examples:
//!
//! | RoutingDecision.engine | `parse_agent_target` | resolved engine arg |
//! |---|---|---|
//! | `"forge"`              | `None`               | (not us)            |
//! | `"claude"`             | `Some("claude")`     | `claude`            |
//! | `"agentapi-claude"`    | `Some("claude")`     | `claude`            |
//! | `"agentapi:codex"`     | `Some("codex")`      | `codex`             |
//!
//! Anything not in [`crate::SUPPORTED_AGENTS`] returns `None`, signalling to
//! the dispatcher that the agentapi engine does not own this target.
#![forbid(unsafe_code)]

use crate::SUPPORTED_AGENTS;

/// Engine-name prefixes that route a task to the agentapi engine.
pub const AGENTAPI_PREFIX: &str = "agentapi-";
/// Engine-name separator variant: `"agentapi:<agent>"`.
pub const AGENTAPI_COLON_PREFIX: &str = "agentapi:";

/// Parse a routing engine name into the agent type the agentapi server should
/// run. Returns `Some(agent)` if the engine name maps to one of the agents
/// in [`SUPPORTED_AGENTS`].
pub fn parse_agent_target(engine: &str) -> Option<&'static str> {
    // Bare agent name: "claude" → "claude"
    if let Some(agent) = SUPPORTED_AGENTS.iter().find(|a| **a == engine) {
        return Some(*agent);
    }
    // Prefixed forms: "agentapi-claude", "agentapi:claude".
    let stripped = engine
        .strip_prefix(AGENTAPI_PREFIX)
        .or_else(|| engine.strip_prefix(AGENTAPI_COLON_PREFIX))?;
    SUPPORTED_AGENTS.iter().copied().find(|a| *a == stripped)
}

/// All agentapi-routable engine names (`agentapi-<agent>` for every supported
/// agent). Useful for the dispatcher to know the full set of targets this
/// engine can claim.
pub fn routable_engine_names() -> Vec<String> {
    SUPPORTED_AGENTS
        .iter()
        .map(|a| format!("{AGENTAPI_PREFIX}{a}"))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_bare_agent_name() {
        assert_eq!(parse_agent_target("claude"), Some("claude"));
        assert_eq!(parse_agent_target("codex"), Some("codex"));
        assert_eq!(parse_agent_target("gemini"), Some("gemini"));
    }

    #[test]
    fn parse_hyphen_prefix() {
        assert_eq!(parse_agent_target("agentapi-claude"), Some("claude"));
        assert_eq!(parse_agent_target("agentapi-codex"), Some("codex"));
    }

    #[test]
    fn parse_colon_prefix() {
        assert_eq!(parse_agent_target("agentapi:claude"), Some("claude"));
        assert_eq!(parse_agent_target("agentapi:gemini"), Some("gemini"));
    }

    #[test]
    fn unknown_agent_returns_none() {
        assert_eq!(parse_agent_target("gpt-9000"), None);
        assert_eq!(parse_agent_target("forge"), None);
        assert_eq!(parse_agent_target("agentapi-unknown"), None);
        assert_eq!(parse_agent_target(""), None);
    }

    #[test]
    fn routable_engine_names_match_supported_set() {
        let names = routable_engine_names();
        assert_eq!(names.len(), SUPPORTED_AGENTS.len());
        assert!(names.iter().any(|n| n == "agentapi-claude"));
        assert!(names.iter().any(|n| n == "agentapi-codex"));
        assert!(names.iter().any(|n| n == "agentapi-gemini"));
    }

    #[test]
    fn every_supported_agent_round_trips() {
        for agent in SUPPORTED_AGENTS {
            let prefixed = format!("{AGENTAPI_PREFIX}{agent}");
            assert_eq!(parse_agent_target(&prefixed), Some(*agent));
            let coloned = format!("{AGENTAPI_COLON_PREFIX}{agent}");
            assert_eq!(parse_agent_target(&coloned), Some(*agent));
            // Bare name also resolves.
            assert_eq!(parse_agent_target(agent), Some(*agent));
        }
    }
}
