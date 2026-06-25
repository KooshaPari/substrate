//! Local git workflow driven by gateway LLM output.

use std::path::{Path, PathBuf};

use serde::Deserialize;
use substrate_core::cloud_dispatch_port::CloudResult;
use substrate_core::error::{Result, SubstrateError};
use tokio::process::Command;

use crate::gateway::KiloGatewayConfig;

const SYSTEM_PROMPT: &str = "You are a coding agent. Respond with a single JSON object only (no markdown fences) containing: commit_message, pr_title, pr_body, diff_summary, and files (array of {path, content} objects with repo-relative paths). Keep changes minimal and focused on the user task.";

/// Structured payload extracted from the gateway response.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct LlmDispatchPayload {
    /// Git commit message.
    pub commit_message: String,
    /// Pull request title for `gh pr create`.
    pub pr_title: String,
    /// Pull request body.
    pub pr_body: String,
    /// Short human summary of the diff.
    pub diff_summary: String,
    /// Files to write before commit.
    #[serde(default)]
    pub files: Vec<LlmFileChange>,
}

/// A single file change from the model.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct LlmFileChange {
    /// Repo-relative path.
    pub path: String,
    /// Full file contents.
    pub content: String,
}

/// Parse JSON (optionally wrapped in markdown fences) from LLM output.
pub fn parse_llm_payload(raw: &str) -> Result<LlmDispatchPayload> {
    let trimmed = raw.trim();
    let json_text = if trimmed.starts_with("```") {
        trimmed
            .trim_start_matches("```json")
            .trim_start_matches("```")
            .trim_end_matches("```")
            .trim()
    } else {
        trimmed
    };
    serde_json::from_str(json_text)
        .map_err(|e| SubstrateError::CloudDispatch(format!("kilo llm payload parse: {e}")))
}

/// Run the full model-backed dispatch pipeline.
pub async fn run_dispatch(
    config: &KiloGatewayConfig,
    repo: &str,
    branch: &str,
    prompt: &str,
) -> Result<CloudResult> {
    let user = format!(
        "Repository: {repo}\nBase branch: {branch}\nTask: {prompt}\n\
         Produce JSON with file edits implementing the task."
    );
    let llm_text = config.complete(SYSTEM_PROMPT, &user).await?;
    let payload = parse_llm_payload(&llm_text)?;

    let work_dir = tempfile_dir()?.join(format!("kilo-dispatch-{}", uuid::Uuid::new_v4()));
    tokio::fs::create_dir_all(&work_dir)
        .await
        .map_err(|e| SubstrateError::Io(e.to_string()))?;

    run_git(&work_dir, &["clone", "--depth", "1", repo, "repo"]).await?;
    let repo_dir = work_dir.join("repo");
    let work_branch = format!("kilo/cloud-dispatch-{}", uuid::Uuid::new_v4());

    run_git(&repo_dir, &["checkout", "-b", &work_branch]).await?;
    run_git(&repo_dir, &["fetch", "origin", branch]).await?;
    let _ = run_git(&repo_dir, &["merge", &format!("origin/{branch}")]).await;

    for file in &payload.files {
        write_repo_file(&repo_dir, &file.path, &file.content).await?;
    }

    // Always record the dispatch artifact for traceability.
    let artifact = repo_dir.join(".kilo-dispatch.md");
    let artifact_body = format!(
        "# Kilo model-backed dispatch\n\n{}\n\n## diff_summary\n\n{}",
        prompt, payload.diff_summary
    );
    tokio::fs::write(&artifact, artifact_body)
        .await
        .map_err(|e| SubstrateError::Io(e.to_string()))?;

    run_git(&repo_dir, &["add", "-A"]).await?;
    run_git(&repo_dir, &["commit", "-m", &payload.commit_message]).await?;
    run_git(&repo_dir, &["push", "-u", "origin", &work_branch]).await?;

    let pr_url = create_pr(&repo_dir, &payload.pr_title, &payload.pr_body, &work_branch).await;

    Ok(CloudResult {
        pr_url,
        branch: work_branch,
        diff_summary: payload.diff_summary,
    })
}

async fn write_repo_file(repo_dir: &Path, rel: &str, content: &str) -> Result<()> {
    let path = repo_dir.join(rel);
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|e| SubstrateError::Io(e.to_string()))?;
    }
    tokio::fs::write(&path, content)
        .await
        .map_err(|e| SubstrateError::Io(e.to_string()))
}

async fn run_git(cwd: &Path, args: &[&str]) -> Result<()> {
    let output = Command::new("git")
        .args(args)
        .current_dir(cwd)
        .output()
        .await
        .map_err(|e| SubstrateError::CloudDispatch(format!("git spawn: {e}")))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(SubstrateError::CloudDispatch(format!(
            "git {} failed: {stderr}",
            args.join(" ")
        )));
    }
    Ok(())
}

async fn create_pr(repo_dir: &Path, title: &str, body: &str, branch: &str) -> Option<String> {
    let output = Command::new("gh")
        .args([
            "pr", "create", "--title", title, "--body", body, "--head", branch,
        ])
        .current_dir(repo_dir)
        .output()
        .await
        .ok()?;

    if !output.status.success() {
        return None;
    }
    let url = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if url.is_empty() {
        None
    } else {
        Some(url)
    }
}

fn tempfile_dir() -> Result<PathBuf> {
    std::env::temp_dir()
        .canonicalize()
        .map_err(|e| SubstrateError::Io(e.to_string()))
}
