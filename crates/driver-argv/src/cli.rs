use clap::{Parser, ValueEnum};
use std::path::PathBuf;

/// CLI flags for `substrate argv` (multi-provider argv construction).
#[derive(Parser, Debug)]
#[command(about = "Build provider-native argv without executing")]
pub struct ArgvCli {
    /// Target provider CLI.
    #[arg(long, value_enum)]
    pub provider: Provider,

    /// The prompt to send to the agent. Required unless `--session interactive`.
    #[arg(long, short)]
    pub prompt: Option<String>,

    /// Working directory.
    #[arg(long, short = 'C', default_value = ".")]
    pub cwd: PathBuf,

    /// Model to use. Hard-rejected for `copilot` (Haiku-locked).
    #[arg(long)]
    pub model: Option<String>,

    /// Task mode; mapped per-provider.
    #[arg(long, value_enum, default_value_t = Mode::Agent)]
    pub mode: Mode,

    /// Codex-only: reasoning depth.
    #[arg(long, value_enum)]
    pub reasoning: Option<Reasoning>,

    /// Session kind.
    #[arg(long, value_enum, default_value_t = Session::Oneshot)]
    pub session: Session,

    /// Owner tag (required for session=bg).
    #[arg(long, env = "THGENT_OWNER_TAG")]
    pub owner: Option<String>,

    /// Routing policy (thegent orchestration only; ignored by raw CLIs).
    #[arg(long, value_enum)]
    pub routing: Option<Routing>,

    /// Timeout in seconds.
    #[arg(long, default_value_t = 600)]
    pub timeout_s: u64,

    /// Sandbox mode.
    #[arg(long)]
    pub sandbox: bool,

    /// Restricted mode.
    #[arg(long)]
    pub restricted: bool,

    /// Dry-run: print the argv that would be executed.
    #[arg(long)]
    pub dry_run: bool,

    /// Emit output as JSON (for machine consumption).
    #[arg(long, value_enum, default_value_t = Emit::Text)]
    pub emit: Emit,

    /// Extra raw flags passed through to the provider CLI.
    #[arg(last = true)]
    pub extra_flags: Vec<String>,
}

/// Supported external agent CLIs.
#[derive(Copy, Clone, Debug, ValueEnum)]
pub enum Provider {
    /// Forge CLI.
    Forge,
    /// Codex CLI (via codex-agent wrapper).
    Codex,
    /// Gemini CLI.
    Gemini,
    /// GitHub Copilot CLI (via copilot-agent wrapper).
    Copilot,
    /// Cursor IDE (no headless CLI; emits echo instruction).
    Cursor,
    /// Droid launcher script.
    Droid,
    /// Minimax via cheap-llm router.
    Minimax,
    /// Claude CLI.
    Claude,
}

/// Task mode mapped per-provider.
#[derive(Copy, Clone, Debug, ValueEnum)]
pub enum Mode {
    /// Full agent mode.
    Agent,
    /// Quick edit mode.
    QuickEdit,
    /// Research mode.
    Research,
    /// Plan mode.
    Plan,
    /// Background mode.
    Background,
    /// Read-only mode.
    ReadOnly,
    /// Write mode.
    Write,
    /// Autopilot mode.
    Autopilot,
}

/// Codex reasoning depth.
#[derive(Copy, Clone, Debug, ValueEnum)]
pub enum Reasoning {
    /// Low reasoning.
    Low,
    /// Medium reasoning.
    Medium,
    /// High reasoning.
    High,
}

/// Session kind.
#[derive(Copy, Clone, Debug, ValueEnum, PartialEq, Eq)]
pub enum Session {
    /// One-shot run.
    Oneshot,
    /// Background session (wraps with `thegent bg` when installed).
    Bg,
    /// Interactive session (prompt optional).
    Interactive,
}

/// Output format for argv planning.
#[derive(Copy, Clone, Debug, ValueEnum, PartialEq, Eq)]
pub enum Emit {
    /// Human-readable text panel.
    Text,
    /// Machine-readable JSON.
    Json,
}

/// Routing policy (orchestration only).
#[derive(Copy, Clone, Debug, ValueEnum)]
pub enum Routing {
    /// Prefer direct CLI invocation.
    PreferDirect,
    /// Prefer proxy route.
    PreferProxy,
    /// Failover routing.
    Failover,
    /// Round-robin routing.
    RoundRobin,
    /// Cheapest route.
    Cheapest,
    /// Cost/quality tradeoff.
    CostQuality,
    /// Pareto-optimal routing.
    Pareto,
    /// ROI-based routing.
    Roi,
}
