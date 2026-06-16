use crate::cli::{ArgvCli, Mode, Provider};
use anyhow::{bail, Result};

/// Resolve provider-native argv for a given request. Does not execute.
pub fn build_argv(args: &ArgvCli) -> Result<Vec<String>> {
    match args.provider {
        Provider::Copilot if args.model.is_some() => {
            bail!("copilot is Haiku-locked; --model is not permitted");
        }
        _ => {}
    }

    if args.session != crate::cli::Session::Interactive && args.prompt.is_none() {
        bail!("--prompt is required unless --session interactive");
    }

    let prompt = args.prompt.clone().unwrap_or_default();

    let mut argv: Vec<String> = match args.provider {
        Provider::Forge => forge_argv(args, &prompt),
        Provider::Codex => codex_argv(args, &prompt),
        Provider::Gemini => gemini_argv(args, &prompt),
        Provider::Copilot => copilot_argv(args, &prompt),
        Provider::Cursor => cursor_argv(args, &prompt),
        Provider::Droid => droid_argv(args, &prompt),
        Provider::Minimax => minimax_argv(args, &prompt),
        Provider::Claude => claude_argv(args, &prompt),
    };

    argv.extend(args.extra_flags.iter().cloned());
    Ok(argv)
}

fn forge_argv(args: &ArgvCli, prompt: &str) -> Vec<String> {
    let mut a = vec!["forge".into(), "-p".into(), prompt.into()];
    a.push("-C".into());
    a.push(args.cwd.display().to_string());
    if let Some(model) = &args.model {
        a.push("--model".into());
        a.push(model.clone());
    }
    if args.sandbox {
        a.push("--sandbox".into());
    }
    if args.restricted {
        a.push("--restricted".into());
    }
    a
}

fn codex_argv(args: &ArgvCli, prompt: &str) -> Vec<String> {
    let mode = match args.mode {
        Mode::Agent | Mode::Write | Mode::Autopilot => "workspace-write",
        Mode::ReadOnly | Mode::Research | Mode::Plan | Mode::QuickEdit => "read-only",
        Mode::Background => "workspace-write",
    };
    let mut a = vec![
        "codex-agent".into(),
        "--mode".into(),
        mode.into(),
        "--cd".into(),
        args.cwd.display().to_string(),
        "--prompt".into(),
        prompt.into(),
    ];
    if let Some(model) = &args.model {
        a.push("--model".into());
        a.push(model.clone());
    }
    if let Some(r) = &args.reasoning {
        a.push("--reasoning".into());
        a.push(format!("{r:?}").to_lowercase());
    }
    a
}

fn gemini_argv(_args: &ArgvCli, prompt: &str) -> Vec<String> {
    vec!["gemini".into(), "chat".into(), prompt.into()]
}

fn copilot_argv(args: &ArgvCli, prompt: &str) -> Vec<String> {
    let mode = match args.mode {
        Mode::Agent | Mode::Autopilot | Mode::Write => "autopilot",
        _ => "programmatic",
    };
    vec![
        "copilot-agent".into(),
        "--mode".into(),
        mode.into(),
        "--cd".into(),
        args.cwd.display().to_string(),
        "--prompt".into(),
        prompt.into(),
    ]
}

fn cursor_argv(_args: &ArgvCli, prompt: &str) -> Vec<String> {
    vec![
        "echo".into(),
        format!("[cursor] {prompt} (invoke via Cursor IDE)"),
    ]
}

fn droid_argv(_args: &ArgvCli, prompt: &str) -> Vec<String> {
    vec!["run_droid.sh".into(), prompt.into()]
}

fn minimax_argv(args: &ArgvCli, prompt: &str) -> Vec<String> {
    let mut a = vec![
        "cheap-llm".into(),
        prompt.into(),
        "--provider".into(),
        "minimax".into(),
    ];
    if let Some(model) = &args.model {
        a.push("--model".into());
        a.push(model.clone());
    }
    a
}

fn claude_argv(_args: &ArgvCli, prompt: &str) -> Vec<String> {
    vec!["claude".into(), "chat".into(), prompt.into()]
}
