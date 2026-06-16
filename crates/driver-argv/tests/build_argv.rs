//! Unit tests for multi-provider argv construction.

use clap::Parser;
use driver_argv::cli::ArgvCli;
use driver_argv::provider::build_argv;

fn parse(args: &[&str]) -> ArgvCli {
    let mut full = vec!["argv"];
    full.extend(args);
    ArgvCli::parse_from(full)
}

#[test]
fn forge_basic() {
    let cli = parse(&["--provider", "forge", "--prompt", "hello"]);
    let argv = build_argv(&cli).unwrap();
    assert_eq!(argv[0], "forge");
    assert!(argv.iter().any(|a| a == "hello"));
}

#[test]
fn codex_with_reasoning() {
    let cli = parse(&[
        "--provider",
        "codex",
        "--prompt",
        "refactor this",
        "--reasoning",
        "high",
        "--mode",
        "plan",
    ]);
    let argv = build_argv(&cli).unwrap();
    assert_eq!(argv[0], "codex-agent");
    assert!(argv.iter().any(|a| a == "high"));
    assert!(argv.iter().any(|a| a == "read-only"));
}

#[test]
fn copilot_rejects_model() {
    let cli = parse(&[
        "--provider",
        "copilot",
        "--prompt",
        "x",
        "--model",
        "something",
    ]);
    let err = build_argv(&cli).unwrap_err();
    assert!(err.to_string().contains("Haiku-locked"));
}

#[test]
fn copilot_autopilot_mode() {
    let cli = parse(&[
        "--provider",
        "copilot",
        "--prompt",
        "x",
        "--mode",
        "autopilot",
    ]);
    let argv = build_argv(&cli).unwrap();
    assert!(argv.iter().any(|a| a == "autopilot"));
}

#[test]
fn missing_prompt_fails() {
    let cli = parse(&["--provider", "forge"]);
    let err = build_argv(&cli).unwrap_err();
    assert!(err.to_string().contains("--prompt"));
}

#[test]
fn minimax_routes_through_cheap_llm() {
    let cli = parse(&["--provider", "minimax", "--prompt", "hi"]);
    let argv = build_argv(&cli).unwrap();
    assert_eq!(argv[0], "cheap-llm");
}

#[test]
fn forge_with_model() {
    let cli = parse(&[
        "--provider",
        "forge",
        "--prompt",
        "hello",
        "--model",
        "claude-opus-4",
    ]);
    let argv = build_argv(&cli).unwrap();
    assert!(argv.contains(&"--model".into()));
    assert!(argv.contains(&"claude-opus-4".into()));
}

#[test]
fn forge_with_cwd() {
    let cli = parse(&[
        "--provider",
        "forge",
        "--prompt",
        "hello",
        "--cwd",
        "/tmp/test",
    ]);
    let argv = build_argv(&cli).unwrap();
    assert!(argv.contains(&"-C".into()));
    assert!(argv.contains(&"/tmp/test".into()));
}

#[test]
fn forge_sandbox_flag() {
    let cli = parse(&["--provider", "forge", "--prompt", "x", "--sandbox"]);
    let argv = build_argv(&cli).unwrap();
    assert!(argv.contains(&"--sandbox".into()));
}

#[test]
fn forge_restricted_flag() {
    let cli = parse(&["--provider", "forge", "--prompt", "x", "--restricted"]);
    let argv = build_argv(&cli).unwrap();
    assert!(argv.contains(&"--restricted".into()));
}

#[test]
fn forge_extra_flags() {
    let cli = parse(&[
        "--provider",
        "forge",
        "--prompt",
        "x",
        "--",
        "--verbose",
        "--max-tokens=4096",
    ]);
    let argv = build_argv(&cli).unwrap();
    assert!(argv.ends_with(&["--verbose".to_owned(), "--max-tokens=4096".to_owned()]));
}

#[test]
fn codex_read_only_modes() {
    for mode in ["read-only", "research", "plan", "quick-edit"] {
        let cli = parse(&["--provider", "codex", "--prompt", "x", "--mode", mode]);
        let argv = build_argv(&cli).unwrap();
        assert!(
            argv.contains(&"--mode".into()) && argv.contains(&"read-only".into()),
            "mode {mode} should map to read-only"
        );
    }
}

#[test]
fn codex_write_modes() {
    for mode in ["agent", "write", "autopilot"] {
        let cli = parse(&["--provider", "codex", "--prompt", "x", "--mode", mode]);
        let argv = build_argv(&cli).unwrap();
        assert!(
            argv.contains(&"--mode".into()) && argv.contains(&"workspace-write".into()),
            "mode {mode} should map to workspace-write"
        );
    }
}

#[test]
fn codex_background_mode() {
    let cli = parse(&[
        "--provider",
        "codex",
        "--prompt",
        "x",
        "--mode",
        "background",
    ]);
    let argv = build_argv(&cli).unwrap();
    assert!(argv.contains(&"workspace-write".into()));
}

#[test]
fn codex_reasoning_levels() {
    for level in ["low", "medium", "high"] {
        let cli = parse(&["--provider", "codex", "--prompt", "x", "--reasoning", level]);
        let argv = build_argv(&cli).unwrap();
        assert!(
            argv.contains(&"--reasoning".into()),
            "reasoning {level} missing flag"
        );
        assert!(
            argv.contains(&level.to_string()),
            "reasoning {level} missing value"
        );
    }
}

#[test]
fn codex_model_forwarded() {
    let cli = parse(&[
        "--provider",
        "codex",
        "--prompt",
        "x",
        "--model",
        "claude-sonnet-4",
    ]);
    let argv = build_argv(&cli).unwrap();
    assert!(argv.contains(&"--model".into()));
    assert!(argv.contains(&"claude-sonnet-4".into()));
}

#[test]
fn codex_cwd_forwarded() {
    let cli = parse(&[
        "--provider",
        "codex",
        "--prompt",
        "x",
        "--cwd",
        "/project/src",
    ]);
    let argv = build_argv(&cli).unwrap();
    assert!(argv.contains(&"--cd".into()));
    assert!(argv.contains(&"/project/src".into()));
}

#[test]
fn gemini_argv() {
    let cli = parse(&["--provider", "gemini", "--prompt", "explain this"]);
    let argv = build_argv(&cli).unwrap();
    assert_eq!(argv, &["gemini", "chat", "explain this"]);
}

#[test]
fn copilot_programmatic_modes() {
    for mode in ["quick-edit", "research", "plan"] {
        let cli = parse(&["--provider", "copilot", "--prompt", "x", "--mode", mode]);
        let argv = build_argv(&cli).unwrap();
        assert!(
            argv.contains(&"programmatic".into()),
            "mode {mode} should use programmatic"
        );
    }
}

#[test]
fn copilot_with_cwd() {
    let cli = parse(&["--provider", "copilot", "--prompt", "x", "--cwd", "/app"]);
    let argv = build_argv(&cli).unwrap();
    assert!(argv.contains(&"--cd".into()));
    assert!(argv.contains(&"/app".into()));
}

#[test]
fn cursor_echo_style() {
    let cli = parse(&["--provider", "cursor", "--prompt", "review this"]);
    let argv = build_argv(&cli).unwrap();
    assert_eq!(argv[0], "echo");
    assert!(argv[1].starts_with("[cursor]"));
    assert!(argv[1].contains("review this"));
}

#[test]
fn droid_argv() {
    let cli = parse(&["--provider", "droid", "--prompt", "run task"]);
    let argv = build_argv(&cli).unwrap();
    assert_eq!(argv, &["run_droid.sh", "run task"]);
}

#[test]
fn minimax_with_model() {
    let cli = parse(&[
        "--provider",
        "minimax",
        "--prompt",
        "hi",
        "--model",
        "abab6.5s",
    ]);
    let argv = build_argv(&cli).unwrap();
    assert!(argv.contains(&"--model".into()));
    assert!(argv.contains(&"abab6.5s".into()));
}

#[test]
fn claude_argv() {
    let cli = parse(&["--provider", "claude", "--prompt", "analyze this"]);
    let argv = build_argv(&cli).unwrap();
    assert_eq!(argv, &["claude", "chat", "analyze this"]);
}

#[test]
fn copilot_model_rejection_all_variants() {
    for model in ["claude-sonnet-4", "gpt-5", "gemini-2.5"] {
        let cli = parse(&["--provider", "copilot", "--prompt", "x", "--model", model]);
        let err = build_argv(&cli).unwrap_err();
        assert!(
            err.to_string().contains("Haiku-locked") || err.to_string().contains("model"),
            "model {model} should be rejected for copilot"
        );
    }
}

#[test]
fn prompt_optional_for_interactive_session() {
    let cli = parse(&["--provider", "forge", "--session", "interactive"]);
    let argv = build_argv(&cli);
    assert!(
        argv.is_ok(),
        "interactive session should not require prompt"
    );
}

#[test]
fn bg_session_with_prompt_and_owner() {
    let cli = parse(&[
        "--provider",
        "forge",
        "--session",
        "bg",
        "--owner",
        "test",
        "--prompt",
        "background task",
    ]);
    let argv = build_argv(&cli);
    assert!(argv.is_ok(), "bg session with prompt+owner should succeed");
}

fn assert_extra_flags_after_provider_args(provider: &str) {
    let cli = parse(&[
        "--provider",
        provider,
        "--prompt",
        "x",
        "--",
        "--verbose",
        "--timeout=300",
    ]);
    let argv = build_argv(&cli).unwrap();
    assert!(
        argv.ends_with(&["--verbose".to_owned(), "--timeout=300".to_owned()]),
        "{provider}: extra flags should be appended last"
    );
}

#[test]
fn extra_flags_forge() {
    assert_extra_flags_after_provider_args("forge");
}

#[test]
fn extra_flags_codex() {
    assert_extra_flags_after_provider_args("codex");
}

#[test]
fn extra_flags_gemini() {
    assert_extra_flags_after_provider_args("gemini");
}

#[test]
fn extra_flags_copilot() {
    assert_extra_flags_after_provider_args("copilot");
}

#[test]
fn extra_flags_cursor() {
    assert_extra_flags_after_provider_args("cursor");
}

#[test]
fn extra_flags_droid() {
    assert_extra_flags_after_provider_args("droid");
}

#[test]
fn extra_flags_minimax() {
    let cli = parse(&[
        "--provider",
        "minimax",
        "--prompt",
        "x",
        "--",
        "--verbose",
        "--timeout=300",
    ]);
    let argv = build_argv(&cli).unwrap();
    assert!(argv.ends_with(&["--verbose".to_owned(), "--timeout=300".to_owned()]));
}

#[test]
fn extra_flags_claude() {
    assert_extra_flags_after_provider_args("claude");
}
