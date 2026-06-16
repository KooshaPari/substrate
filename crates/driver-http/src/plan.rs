//! Engine catalog, argv enrichment, and plan helpers for the HTTP driver.

use engine_agentapi::AgentApiArgv;
use engine_claude::ClaudeArgv;
use engine_codex::CodexArgv;
use engine_forge::ForgeArgv;
use engine_spec::{ArgvBuilder, TaskSpec};
use substrate_app::{DispatchPlan, EngineCandidate};
use substrate_core::domain::EngineCapabilities;

/// Static catalog of engines the planner may choose among.
pub fn engine_catalog() -> Vec<EngineCandidate> {
    vec![
        EngineCandidate {
            name: "agentapi".into(),
            capabilities: EngineCapabilities {
                supports_resume: false,
                supports_subagents: false,
                supports_mcp_import: false,
            },
        },
        EngineCandidate {
            name: "claude".into(),
            capabilities: EngineCapabilities {
                supports_resume: true,
                supports_subagents: true,
                supports_mcp_import: false,
            },
        },
        EngineCandidate {
            name: "codex".into(),
            capabilities: EngineCapabilities {
                supports_resume: false,
                supports_subagents: true,
                supports_mcp_import: false,
            },
        },
        EngineCandidate {
            name: "forge".into(),
            capabilities: EngineCapabilities {
                supports_resume: true,
                supports_subagents: true,
                supports_mcp_import: false,
            },
        },
    ]
}

/// Fill `plan.argv` from the chosen engine's argv builder + env-resolved binary.
pub fn enrich_plan_argv(plan: &mut DispatchPlan) {
    plan.argv = build_argv(&plan.engine, &plan.spec);
}

/// Build `[program, arg0, …]` for `engine` + `spec`.
pub fn build_argv(engine: &str, spec: &TaskSpec) -> Vec<String> {
    match engine {
        "forge" => {
            let bin = std::env::var("FORGE_BIN").unwrap_or_else(|_| "forge".into());
            let mut argv = ForgeArgv::default().build_start(spec);
            argv.insert(0, bin);
            argv
        }
        "codex" => {
            let bin = std::env::var("CODEX_BIN").unwrap_or_else(|_| "codex".into());
            let mut argv = CodexArgv::default().build_start(spec);
            argv.insert(0, bin);
            argv
        }
        "claude" => {
            let bin = std::env::var("CLAUDE_BIN").unwrap_or_else(|_| "claude".into());
            let mut argv = ClaudeArgv::default().build_start(spec);
            argv.insert(0, bin);
            argv
        }
        "agentapi" => {
            let endpoint = std::env::var("AGENTAPI_ENDPOINT")
                .unwrap_or_else(|_| "http://localhost:3284".into());
            let mut argv = AgentApiArgv::new(endpoint).build_start(spec);
            argv.insert(0, "agentapi".into());
            argv
        }
        other => vec![other.into()],
    }
}
