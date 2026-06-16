use crate::cli::{ArgvCli, Emit, Session};
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

/// Build argv for `args`, optionally print a plan, and execute unless dry-run.
pub fn run(args: ArgvCli) -> Result<()> {
    let argv = provider::build_argv(&args)?;

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

    if args.dry_run || args.emit == Emit::Json {
        let provider = format!("{:?}", args.provider).to_lowercase();
        let mode = format!("{:?}", args.mode).to_lowercase();
        let session = format!("{:?}", args.session).to_lowercase();

        if args.emit == Emit::Json {
            let plan = DispatchPlan {
                provider,
                mode,
                session,
                dry_run: args.dry_run,
                argv: &final_argv,
            };
            println!("{}", serde_json::to_string_pretty(&plan)?);
        } else {
            print_dry_run_panel(&provider, &mode, &session, &final_argv)?;
        }

        if args.dry_run {
            return Ok(());
        }
    }

    let mut cmd = Command::new(&final_argv[0]);
    cmd.args(&final_argv[1..]);
    cmd.current_dir(&args.cwd);
    let status = cmd
        .status()
        .with_context(|| format!("failed to execute provider CLI: {}", final_argv[0]))?;
    if !status.success() {
        anyhow::bail!("provider exited with {}", status);
    }
    Ok(())
}

fn print_dry_run_panel(provider: &str, mode: &str, session: &str, argv: &[String]) -> Result<()> {
    use std::io::Write;

    let mut lines: Vec<String> = vec![
        format!("provider : {provider}"),
        format!("mode     : {mode}"),
        format!("session  : {session}"),
        String::new(),
        "argv:".to_string(),
    ];
    for arg in argv {
        for line in arg.lines() {
            lines.push(format!("  {line}"));
        }
    }

    let max_width = lines
        .iter()
        .map(|l| l.len())
        .max()
        .unwrap_or(0)
        .max("substrate argv - dry run".len());

    let mut out = std::io::stdout().lock();

    writeln!(out, "+-{}-+", "-".repeat(max_width))?;
    writeln!(
        out,
        "| substrate argv - dry run{} |",
        " ".repeat(max_width - "substrate argv - dry run".len())
    )?;
    writeln!(out, "+-{}-+", "-".repeat(max_width))?;
    for line in &lines {
        writeln!(out, "| {}{} |", line, " ".repeat(max_width - line.len()))?;
    }
    writeln!(out, "+-{}-+", "-".repeat(max_width))?;

    out.flush()?;
    Ok(())
}
