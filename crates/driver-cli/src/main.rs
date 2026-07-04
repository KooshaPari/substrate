//! # substrate (driver-cli)
//!
//! The composition root: parses CLI args, wires the concrete adapters into
//! [`DispatchService`], and prints the [`StructuredResult`] as JSON.
#![forbid(unsafe_code)]

mod cloud_dispatch;
mod plan;
mod serve;

use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{anyhow, Context};
use clap::{Parser, Subcommand};
use driver_argv::ArgvCli;
use engine_codex::CodexEngine;
use engine_forge::ForgeEngine;
use engine_spec::TaskSpec;
use plan::{engine_catalog, enrich_plan_argv, print_plan};
use store_file::FileStore;
use substrate_app::tiered_dispatch::dispatch_with_reroute_async;
use substrate_app::DispatchService;
use substrate_app::{DispatchPlanner, PlanRequest, SessionMode};
use substrate_core::domain::Task;
use substrate_core::ports::DispatchApi;
use substrate_core::Tier;
use transport_file::FileTransport;

#[derive(Parser)]
#[command(
    name = "substrate",
    version,
    about = "Dispatch agent tasks over the substrate hexagonal spine.",
    long_about = "Substrate routes prompts to coding engines (forge, codex, claude, agentapi) \
                  through a deterministic planner. Use `plan` or `dispatch --dry-run` to inspect \
                  the chosen engine, session mode, and argv without spawning a process.",
    subcommand_required = true,
    arg_required_else_help = true,
    after_help = "EXAMPLES:\n  \
                  substrate plan --engine forge --cwd . \"fix the bug\"\n  \
                  substrate dispatch --fake --cwd . \"echo hi\"\n  \
                  substrate dispatch --dry-run --cwd . \"echo hi\"\n  \
                  substrate argv --provider forge --prompt \"hello\" --dry-run\n\n\
                  ENV (engine binaries):\n  \
                  FORGE_BIN, CODEX_BIN, CLAUDE_BIN, AGENTAPI_ENDPOINT"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Dispatch a task to an engine and print the structured result as JSON.
    Dispatch(DispatchArgs),
    /// Print the dispatch plan (engine, session mode, argv) without executing.
    Plan(DispatchArgs),
    /// Build provider-native argv for external agent CLIs (thegent-dispatch surface).
    Argv {
        #[command(flatten)]
        args: ArgvCli,
    },
    /// Submit a remote cloud-agent task and harvest the PR result as JSON.
    CloudDispatch(CloudDispatchArgs),
    /// Start the substrate HTTP server with a lock-based single-instance guard.
    Serve(serve::ServeArgs),
}

/// Flags shared by `dispatch` and `plan`.
#[derive(Parser)]
#[command(next_help_heading = "TASK")]
struct DispatchArgs {
    /// The prompt to dispatch.
    prompt: String,

    #[command(flatten)]
    options: DispatchOptions,
}

#[derive(Parser)]
#[command(next_help_heading = "OPTIONS")]
struct DispatchOptions {
    /// Engine to use (`forge`, `codex`, `claude`, `agentapi`); planner selects when omitted.
    #[arg(long, value_name = "ENGINE")]
    engine: Option<String>,
    /// Session mode: `background`, `foreground`, or `in_process`.
    #[arg(long, value_name = "MODE")]
    mode: Option<String>,
    /// Use the bundled network-free fake forge (sets `FORGE_BIN` and `in_process` mode).
    #[arg(long)]
    fake: bool,
    /// Print the plan without spawning (same output as the `plan` subcommand).
    #[arg(long)]
    dry_run: bool,
    /// Run codex through tiered dispatch (`heavy`, `main`, or `worker`) with reroute-up retries.
    #[arg(long, value_name = "TIER")]
    tier: Option<String>,
    /// Working directory the engine runs in.
    #[arg(long, value_name = "DIR")]
    cwd: String,
    /// Conversation id to resume (requires an engine with `supports_resume`).
    #[arg(long, value_name = "CONV_ID")]
    resume: Option<String>,
    /// Named agent/persona (`subagent` requires `supports_subagents`).
    #[arg(long, value_name = "NAME")]
    agent: Option<String>,
}

#[derive(Parser)]
#[command(next_help_heading = "CLOUD DISPATCH")]
struct CloudDispatchArgs {
    /// Cloud platform: `cursor` (Cursor Cloud Agents), `codex` (Codex Cloud CLI), or `kilo` (gateway + local git).
    #[arg(long, value_enum, value_name = "PLATFORM")]
    platform: cloud_dispatch::CloudPlatform,
    /// Repository URL (for example `https://github.com/org/repo`).
    #[arg(long, value_name = "REPO")]
    repo: String,
    /// Base branch or ref to start from.
    #[arg(long, value_name = "BRANCH")]
    branch: String,
    /// Task prompt for the remote agent.
    #[arg(long, value_name = "PROMPT")]
    task: String,
}

impl DispatchArgs {
    fn prompt_text(&self) -> anyhow::Result<String> {
        let path = PathBuf::from(&self.prompt);
        if path.exists() && path.is_file() {
            std::fs::read_to_string(&path)
                .with_context(|| format!("read prompt file {}", path.display()))
        } else {
            Ok(self.prompt.clone())
        }
    }

    fn task_spec(&self) -> anyhow::Result<TaskSpec> {
        let prompt = self.prompt_text()?;
        let mut spec = TaskSpec::new(&prompt, &self.options.cwd);
        if let Some(agent) = &self.options.agent {
            spec = spec.with_agent(agent.clone());
        }
        if let Some(resume) = &self.options.resume {
            spec.resume = Some(resume.clone());
        }
        Ok(spec)
    }

    fn session_mode(&self) -> anyhow::Result<Option<SessionMode>> {
        match &self.options.mode {
            None => Ok(None),
            Some(s) => SessionMode::parse_cli(s).map(Some).ok_or_else(|| {
                anyhow!("invalid --mode {s}: use background, foreground, or in_process")
            }),
        }
    }

    fn plan(&self) -> anyhow::Result<substrate_app::DispatchPlan> {
        let spec = self.task_spec()?;
        let engines = engine_catalog();
        let mut plan = DispatchPlanner::plan(&PlanRequest {
            spec: &spec,
            engines: &engines,
            explicit_engine: self.options.engine.as_deref(),
            session_mode: self.session_mode()?,
            routing_engine: self.options.engine.as_deref().or(Some("forge")),
        })
        .map_err(|e| anyhow!("plan failed: {e}"))?;
        enrich_plan_argv(&mut plan);
        if let Some(tier) = self.tier()? {
            plan.engine = "codex".to_string();
            plan.argv = {
                let bin = std::env::var("CODEX_BIN").unwrap_or_else(|_| "codex".into());
                let mut argv = CodexEngine::new().with_tier(tier).argv_for(&plan.spec);
                argv.insert(0, bin);
                argv
            };
        }
        if self.options.fake {
            plan.session_mode = SessionMode::InProcess;
        }
        Ok(plan)
    }

    fn tier(&self) -> anyhow::Result<Option<Tier>> {
        self.options
            .tier
            .as_deref()
            .map(str::parse::<Tier>)
            .transpose()
            .map_err(anyhow::Error::msg)
    }
}

/// Locate the bundled `fake-forge` binary next to the running executable.
fn fake_forge_path() -> anyhow::Result<PathBuf> {
    let exe = std::env::current_exe().context("resolve current exe")?;
    let dir = exe
        .parent()
        .ok_or_else(|| anyhow!("exe has no parent dir"))?;
    let name = if cfg!(windows) {
        "fake-forge.exe"
    } else {
        "fake-forge"
    };
    let candidate = dir.join(name);
    if candidate.exists() {
        Ok(candidate)
    } else {
        Err(anyhow!(
            "fake-forge not found at {} (build the workspace first)",
            candidate.display()
        ))
    }
}

async fn execute_plan(plan: &substrate_app::DispatchPlan, cwd: &str) -> anyhow::Result<()> {
    if plan.engine != "forge" {
        return Err(anyhow!(
            "execution wiring supports forge only in this build; plan selected {}",
            plan.engine
        ));
    }

    let state = PathBuf::from(cwd).join(".substrate");
    let store = Arc::new(FileStore::new(state.join("store"))?);
    let transport = Arc::new(FileTransport::new(state.join("mailbox"))?);
    let forge = Arc::new(ForgeEngine::new());

    let svc = DispatchService::new(forge, store, transport);
    let task = Task::new(plan.spec.prompt.clone(), plan.spec.cwd.clone());
    let result = svc
        .dispatch(task)
        .await
        .map_err(|e| anyhow!("dispatch failed: {e}"))?;

    println!("{}", serde_json::to_string_pretty(&result)?);
    Ok(())
}

async fn execute_tiered_dispatch(args: &DispatchArgs, start_tier: Tier) -> anyhow::Result<()> {
    let prompt = args.prompt_text()?;
    let cwd = args.options.cwd.clone();
    let outcome = dispatch_with_reroute_async(start_tier, |tier| {
        let prompt = prompt.clone();
        let cwd = cwd.clone();
        async move {
            let spec = TaskSpec::new(&prompt, &cwd);
            CodexEngine::new().with_tier(tier).run_exec(&spec).await
        }
    })
    .await
    .map_err(|e| anyhow!("tiered dispatch failed: {e}"))?;

    let payload = serde_json::json!({
        "success": true,
        "engine": "codex",
        "succeeded_tier": outcome.succeeded_tier.to_string(),
        "output": outcome.output,
    });
    println!("{}", serde_json::to_string_pretty(&payload)?);
    Ok(())
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Command::Dispatch(args) => {
            if args.options.fake {
                let path = fake_forge_path()?;
                std::env::set_var("FORGE_BIN", path);
            }
            let tier = args.tier()?;
            let plan = args.plan()?;
            if args.options.dry_run {
                print_plan(&plan)?;
                return Ok(());
            }
            if let Some(tier) = tier {
                return execute_tiered_dispatch(&args, tier).await;
            }
            execute_plan(&plan, &args.options.cwd).await
        }
        Command::Plan(args) => {
            let plan = args.plan()?;
            print_plan(&plan)
        }
        Command::Argv { args } => driver_argv::dispatch::run(args),
        Command::CloudDispatch(args) => {
            cloud_dispatch::run(args.platform, &args.repo, &args.branch, &args.task).await
        }
        Command::Serve(args) => serve::run(args).await,
    }
}
