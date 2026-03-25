//! `start` command - Start a harness process

use crate::config::Config;
use crate::runtime::ProcessPool;
use anyhow::Result;
use clap::Parser;
use std::path::PathBuf;

#[derive(Parser, Debug)]
pub struct Start {
    /// Project name
    #[arg(required = true)]
    project: String,

    /// Harness type (claude, forge, node, bun)
    #[arg(short, long, default_value = "claude")]
    harness: String,

    /// Working directory
    #[arg(short, long)]
    cwd: Option<PathBuf>,

    /// Arguments to pass to the harness
    #[arg(trailing_var_arg = true)]
    args: Vec<String>,
}

impl Start {
    pub async fn run(&self) -> Result<()> {
        let config = Config::load()?;
        let pool = ProcessPool::new();

        // Resolve project path
        let project_path = if let Some(ref cwd) = self.cwd {
            cwd.clone()
        } else if let Some(path) = config.projects.get(&self.project) {
            let expanded = shellexp::expand(path)?;
            PathBuf::from(expanded)
        } else {
            anyhow::bail!("Unknown project: {}. Add it with 'sharecli project add <name> <path>'", self.project);
        };

        if !project_path.exists() {
            anyhow::bail!("Project path does not exist: {:?}", project_path);
        }

        // Determine command based on harness
        let (cmd, args) = match self.harness.as_str() {
            "claude" => {
                ("claude", vec!["--resume".to_string()].into_iter().chain(self.args.iter().cloned()).collect::<Vec<_>>())
            }
            "forge" => {
                ("forge", self.args.clone())
            }
            "node" => {
                ("node", self.args.clone())
            }
            "bun" => {
                let bun_path = config.runtime.bun_path.as_ref()
                    .map(|p| p.as_str())
                    .unwrap_or("bun");
                (bun_path, self.args.clone())
            }
            _ => anyhow::bail!("Unknown harness type: {}. Use claude, forge, node, or bun", self.harness),
        };

        println!("Starting {} harness for project '{}'...", self.harness, self.project);
        let info = pool.spawn(cmd, &args, Some(project_path.clone()), Some(self.project.clone()), Some(self.harness.clone())).await?;

        println!("Started process {} ({})", info.pid, info.name);
        println!("Working directory: {:?}", project_path);

        Ok(())
    }
}

// Simple shell expansion for ~/
fn expand_path(path: &str) -> String {
    if path.starts_with("~/") {
        if let Ok(home) = std::env::var("HOME") {
            return path.replacen("~/", &format!("{}/", home), 1);
        }
    }
    path.to_string()
}

// Module for shell expansion
mod shellexp {
    pub fn expand(path: &str) -> anyhow::Result<String> {
        let expanded = if path.starts_with("~/") {
            let home = std::env::var("HOME")
                .context("Could not find HOME directory")?;
            path.replacen("~/", &format!("{}/", home), 1)
        } else {
            path.to_string()
        };

        // Basic tilde expansion
        let result = shellexp::tilde_expand::tilde_expand(&expanded)
            .map_err(|e| anyhow::anyhow!("Failed to expand path: {}", e))?;

        Ok(result)
    }
}
