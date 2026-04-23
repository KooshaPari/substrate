use clap::{Parser, ValueEnum};
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(
    name = "thegent-dispatch",
    about = "Unified dispatcher for external coding-agent CLIs",
    version
)]
pub struct Cli {
    /// Target provider CLI.
    #[arg(long, value_enum)]
    pub provider: Provider,

    /// The prompt to send to the agent. Required unless `--mode interactive`.
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

#[derive(Copy, Clone, Debug, ValueEnum)]
pub enum Provider {
    Forge,
    Codex,
    Gemini,
    Copilot,
    Cursor,
    Droid,
    Minimax,
    Claude,
}

#[derive(Copy, Clone, Debug, ValueEnum)]
pub enum Mode {
    Agent,
    QuickEdit,
    Research,
    Plan,
    Background,
    ReadOnly,
    Write,
    Autopilot,
}

#[derive(Copy, Clone, Debug, ValueEnum)]
pub enum Reasoning {
    Low,
    Medium,
    High,
}

#[derive(Copy, Clone, Debug, ValueEnum, PartialEq)]
pub enum Session {
    Oneshot,
    Bg,
    Interactive,
}

#[derive(Copy, Clone, Debug, ValueEnum, PartialEq)]
pub enum Emit {
    Text,
    Json,
}

#[derive(Copy, Clone, Debug, ValueEnum)]
pub enum Routing {
    PreferDirect,
    PreferProxy,
    Failover,
    RoundRobin,
    Cheapest,
    CostQuality,
    Pareto,
    Roi,
}
