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
        let provider = format!("{:?}", args.provider).to_lowercase();
        let mode = format!("{:?}", args.mode).to_lowercase();
        let session = format!("{:?}", args.session).to_lowercase();

        if args.emit == Emit::Json {
            // Machine-readable path: keep the JSON contract pristine.
            let plan = DispatchPlan {
                provider,
                mode,
                session,
                dry_run: args.dry_run,
                argv: &final_argv,
            };
            println!("{}", serde_json::to_string_pretty(&plan)?);
        } else {
            // Human path (dry-run, text emit): render the planned argv as a
            // rich panel via the Phenotype-org rck-core toolkit. Degrades to
            // plain ASCII on non-kitty terminals, when piped, or in CI.
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

/// Render the planned dispatch as a rich panel via rck-core. Capability
/// detection means this prints a kitty/rounded panel on capable terminals and
/// degrades to a plain-ASCII box (or simpler) when piped, in CI, or on
/// terminals without graphics support — so the output stays pipe-safe.
fn print_dry_run_panel(provider: &str, mode: &str, session: &str, argv: &[String]) -> Result<()> {
    use std::io::Write;

    let caps = rck_core::detect();

    let mut lines: Vec<String> = vec![
        format!("provider : {provider}"),
        format!("mode     : {mode}"),
        format!("session  : {session}"),
        String::new(),
        "argv:".to_string(),
    ];
    // Split each argument on embedded newlines so a multi-line argument (e.g. a
    // prompt containing '\n') is wrapped line-by-line inside the panel instead
    // of breaking the box border.
    for arg in argv {
        for line in arg.lines() {
            lines.push(format!("  {line}"));
        }
    }
    let line_refs: Vec<&str> = lines.iter().map(String::as_str).collect();

    // Rounded (unicode) border on a real terminal; plain ASCII border when
    // piped or in CI so downstream consumers never see unicode box-drawing.
    let border = if caps.is_tty {
        rck_core::BorderStyle::Rounded
    } else {
        rck_core::BorderStyle::Ascii
    };

    let mut out = std::io::stdout().lock();
    rck_core::emit_panel(
        &mut out,
        &caps,
        "thegent-dispatch - dry run",
        &line_refs,
        border,
    )?;
    out.flush()?;
    Ok(())
}
