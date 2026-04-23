use clap::Parser;
use thegent_dispatch::cli::Cli;
use thegent_dispatch::provider::build_argv;

fn parse(args: &[&str]) -> Cli {
    let mut full = vec!["thegent-dispatch"];
    full.extend(args);
    Cli::parse_from(full)
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
        "--provider", "codex",
        "--prompt", "refactor this",
        "--reasoning", "high",
        "--mode", "plan",
    ]);
    let argv = build_argv(&cli).unwrap();
    assert_eq!(argv[0], "codex-agent");
    assert!(argv.iter().any(|a| a == "high"));
    assert!(argv.iter().any(|a| a == "read-only"));
}

#[test]
fn copilot_rejects_model() {
    let cli = parse(&[
        "--provider", "copilot",
        "--prompt", "x",
        "--model", "something",
    ]);
    let err = build_argv(&cli).unwrap_err();
    assert!(err.to_string().contains("Haiku-locked"));
}

#[test]
fn copilot_autopilot_mode() {
    let cli = parse(&["--provider", "copilot", "--prompt", "x", "--mode", "autopilot"]);
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
