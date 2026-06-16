//! # substrate (driver-cli)
//!
//! The composition root: parses CLI args, wires the concrete adapters into
//! [`DispatchService`], and prints the [`StructuredResult`] as JSON.
#![forbid(unsafe_code)]

mod plan;

use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{anyhow, Context};
use clap::{Parser, Subcommand};
use engine_forge::ForgeEngine;
use engine_spec::TaskSpec;
use plan::{engine_catalog, enrich_plan_argv, print_plan};
use store_file::FileStore;
use substrate_app::DispatchService;
use substrate_app::{DispatchPlanner, PlanRequest, SessionMode};
use substrate_core::domain::Task;
use substrate_core::ports::DispatchApi;
use transport_file::FileTransport;

#[derive(Parser)]
#[command(
    name = "substrate",
    about = "Dispatch agent tasks over the substrate spine."
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Dispatch a single task to an engine and print the structured result.
    Dispatch(DispatchArgs),
    /// Print the dispatch plan without executing (dry-run).
    Plan(DispatchArgs),
}

/// Shared flags for dispatch and plan.
#[derive(Parser)]
struct DispatchArgs {
    /// Engine to use; when omitted the planner selects by capabilities + routing.
    #[arg(long)]
    engine: Option<String>,
    /// Session mode: background, foreground, in_process.
    #[arg(long)]
    mode: Option<String>,
    /// Use the bundled network-free fake forge.
    #[arg(long)]
    fake: bool,
    /// Print the plan without spawning (alias for the `plan` subcommand).
    #[arg(long)]
    dry_run: bool,
    /// Working directory the engine runs in.
    #[arg(long)]
    cwd: String,
    /// Conversation id to resume (requires an engine with supports_resume).
    #[arg(long)]
    resume: Option<String>,
    /// Named agent/persona (use `subagent` to require supports_subagents).
    #[arg(long)]
    agent: Option<String>,
    /// The prompt to dispatch.
    prompt: String,
}

impl DispatchArgs {
    fn task_spec(&self) -> TaskSpec {
        let mut spec = TaskSpec::new(&self.prompt, &self.cwd);
        if let Some(agent) = &self.agent {
            spec = spec.with_agent(agent.clone());
        }
        if let Some(resume) = &self.resume {
            spec.resume = Some(resume.clone());
        }
        spec
    }

    fn session_mode(&self) -> anyhow::Result<Option<SessionMode>> {
        match &self.mode {
            None => Ok(None),
            Some(s) => SessionMode::parse_cli(s)
                .map(Some)
                .ok_or_else(|| {
                    anyhow!("invalid --mode {s}: use background, foreground, or in_process")
                }),
        }
    }

    fn plan(&self) -> anyhow::Result<substrate_app::DispatchPlan> {
        let spec = self.task_spec();
        let engines = engine_catalog();
        let mut plan = DispatchPlanner::plan(&PlanRequest {
            spec: &spec,
            engines: &engines,
            explicit_engine: self.engine.as_deref(),
            session_mode: self.session_mode()?,
            routing_engine: self.engine.as_deref().or(Some("forge")),
        })
        .map_err(|e| anyhow!("plan failed: {e}"))?;
        enrich_plan_argv(&mut plan);
        if self.fake {
            plan.session_mode = SessionMode::InProcess;
        }
        Ok(plan)
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

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Command::Dispatch(args) => {
            if args.fake {
                let path = fake_forge_path()?;
                std::env::set_var("FORGE_BIN", path);
            }
            let plan = args.plan()?;
            if args.dry_run {
                print_plan(&plan)?;
                return Ok(());
            }
            execute_plan(&plan, &args.cwd).await
        }
        Command::Plan(args) => {
            let plan = args.plan()?;
            print_plan(&plan)
        }
    }
}
