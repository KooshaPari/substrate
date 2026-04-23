use crate::cli::{Cli, Emit, Session};
use crate::provider;
use anyhow::{Context, Result};
use serde::Serialize;
use std::process::Command;

#[derive(Serialize)]
struct DispatchPlan<'a> {
    provider: String,
    mode: String,
    session: String,
    dry_run: bool,
    argv: &'a [String],
}

pub fn run(args: Cli) -> Result<()> {
    let argv = provider::build_argv(&args)?;

    // Wrap in thegent bg for session=bg.
    let final_argv: Vec<String> = if args.session == Session::Bg {
        let owner = args
            .owner
            .clone()
            .context("--session bg requires --owner (or $THGENT_OWNER_TAG)")?;
        let mut wrapped = vec![
            "thegent".into(),
            "bg".into(),
            "--owner".into(),
            owner,
            "--format".into(),
            "json".into(),
            "--".into(),
        ];
        wrapped.extend(argv);
        wrapped
    } else {
        argv
    };

    tracing::info!(argv = ?final_argv, "dispatching");

    if args.dry_run || args.emit == Emit::Json {
        let plan = DispatchPlan {
            provider: format!("{:?}", args.provider).to_lowercase(),
            mode: format!("{:?}", args.mode).to_lowercase(),
            session: format!("{:?}", args.session).to_lowercase(),
            dry_run: args.dry_run,
            argv: &final_argv,
        };
        println!("{}", serde_json::to_string_pretty(&plan)?);
        if args.dry_run {
            return Ok(());
        }
    }

    let mut cmd = Command::new(&final_argv[0]);
    cmd.args(&final_argv[1..]);
    cmd.current_dir(&args.cwd);
    let status = cmd.status().with_context(|| {
        format!("failed to execute provider CLI: {}", final_argv[0])
    })?;
    if !status.success() {
        anyhow::bail!("provider exited with {}", status);
    }
    Ok(())
}
