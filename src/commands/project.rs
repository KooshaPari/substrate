//! `project` command - Project management

use crate::config::Config;
use anyhow::Result;
use clap::Parser;
use std::path::PathBuf;

#[derive(Parser, Debug)]
pub enum ProjectCmd {
    /// Add a project to the registry
    Add {
        /// Project name
        name: String,
        /// Project path
        path: String,
    },

    /// Remove a project from the registry
    Remove {
        /// Project name
        name: String,
    },

    /// List all registered projects
    List,

    /// Show project details
    Show {
        /// Project name
        name: String,
    },

    /// Discover projects in a directory
    Discover {
        /// Directory to scan
        #[arg(default_value = "~/CodeProjects/Phenotype/repos")]
        path: String,
    },
}

impl ProjectCmd {
    pub fn run(&self) -> Result<()> {
        match self {
            ProjectCmd::Add { name, path } => {
                let expanded = expand_path(path);
                let expanded = shellexp::expand(path)?;
                let path = PathBuf::from(&expanded);

                if !path.exists() {
                    anyhow::bail!("Path does not exist: {:?}", path);
                }

                let mut config = Config::load()?;
                config.projects.insert(name.clone(), path.to_string_lossy().to_string());
                config.save()?;

                println!("Added project '{}' -> {:?}", name, path);
            }
            ProjectCmd::Remove { name } => {
                let mut config = Config::load()?;
                if config.projects.remove(name).is_some() {
                    config.save()?;
                    println!("Removed project '{}'", name);
                } else {
                    println!("Project '{}' not found", name);
                }
            }
            ProjectCmd::List => {
                let config = Config::load()?;
                if config.projects.is_empty() {
                    println!("No projects registered.");
                    println!("Run 'sharecli project discover' to find projects.");
                    return Ok(());
                }

                println!("Registered Projects:");
                println!("{:<25} {}", "NAME", "PATH");
                println!("{}", "-".repeat(80));

                for (name, path) in &config.projects {
                    let expanded = expand_path(path);
                    let exists = std::path::Path::new(&expanded).exists();
                    let status = if exists { "" } else { " [MISSING]" };
                    println!("{:<25} {}{}", name, path, status);
                }
            }
            ProjectCmd::Show { name } => {
                let config = Config::load()?;
                if let Some(path) = config.projects.get(name) {
                    let expanded = expand_path(path);
                    let path = PathBuf::from(&expanded);

                    println!("Project: {}", name);
                    println!("Path:    {:?}", path);
                    println!("Exists:  {}", path.exists());
                    println!("Git:     {}", path.join(".git").exists());
                } else {
                    anyhow::bail!("Project '{}' not found", name);
                }
            }
            ProjectCmd::Discover { path } => {
                let expanded = expand_path(path);
                let base = PathBuf::from(&expanded);

                if !base.exists() {
                    anyhow::bail!("Directory does not exist: {:?}", base);
                }

                println!("Scanning {:?} for projects...", base);

                let mut found = Vec::new();
                if let Ok(entries) = std::fs::read_dir(&base) {
                    for entry in entries.flatten() {
                        let path = entry.path();
                        if path.is_dir() && path.join(".git").exists() {
                            let name = path.file_name()
                                .and_then(|n| n.to_str())
                                .unwrap_or("unknown")
                                .to_string();
                            found.push((name, path.to_string_lossy().to_string()));
                        }
                    }
                }

                println!("\nFound {} projects:", found.len());
                for (name, path) in &found {
                    println!("  {} -> {}", name, path);
                }

                if !found.is_empty() {
                    println!("\nTo add projects, run:");
                    println!("  sharecli project add <name> <path>");
                }
            }
        }
        Ok(())
    }
}

fn expand_path(path: &str) -> String {
    if path.starts_with("~/") {
        if let Ok(home) = std::env::var("HOME") {
            return path.replacen("~/", &format!("{}/", home), 1);
        }
    }
    path.to_string()
}

mod shellexp {
    pub fn expand(path: &str) -> anyhow::Result<String> {
        let expanded = if path.starts_with("~/") {
            let home = std::env::var("HOME")
                .context("Could not find HOME directory")?;
            path.replacen("~/", &format!("{}/", home), 1)
        } else {
            path.to_string()
        };
        Ok(expanded)
    }
}
