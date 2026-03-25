//! CLI commands for sharecli

pub mod config;

use anyhow::Result;
use crate::config::{Config, ConfigCmd, ProjectCmd};
use crate::runtime::{ProcessFilter, ProcessPool, ProcessInfo};
use std::path::PathBuf;

/// List processes
pub async fn ps(project: Option<&str>, harness: Option<&str>, all: bool) -> Result<()> {
    let pool = ProcessPool::new();
    let filter = if let Some(p) = project {
        ProcessFilter::ByProject(p.to_string())
    } else if let Some(h) = harness {
        ProcessFilter::ByHarness(h.to_string())
    } else if all {
        ProcessFilter::All
    } else {
        ProcessFilter::All
    };

    let processes: Vec<ProcessInfo> = pool.find(filter).await;

    println!("{:<8} {:<20} {:<12} {:<15} {}", "PID", "NAME", "MEM(MB)", "PROJECT", "HARNESS");
    println!("{}", "-".repeat(70));

    for proc in &processes {
        let project = proc.project.as_deref().unwrap_or("-");
        let harness = proc.harness.as_deref().unwrap_or("-");
        println!(
            "{:<8} {:<20} {:<12.1} {:<15} {}",
            proc.pid, proc.name, proc.memory_mb as f64, project, harness
        );
    }

    let total_mem: u64 = processes.iter().map(|p| p.memory_mb).sum();
    println!("\nTotal: {} processes, {} MB memory", processes.len(), total_mem);

    Ok(())
}

/// Start a harness process
pub async fn start(project: &str, harness: &str, cwd: Option<&str>, _args: &[String]) -> Result<()> {
    let cfg = Config::load()?;
    
    let project_path = if let Some(c) = cwd {
        PathBuf::from(expand_path(c))
    } else if let Some(path) = cfg.projects.get(project) {
        PathBuf::from(expand_path(path))
    } else {
        anyhow::bail!("Unknown project: {}. Add with 'sharecli project add <name> <path>'", project);
    };

    if !project_path.exists() {
        anyhow::bail!("Project path does not exist: {:?}", project_path);
    }

    let pool = ProcessPool::new();
    println!("Starting {} harness for project '{}'...", harness, project);
    
    let info = pool.spawn(harness, &[], Some(project_path.clone()), Some(project.to_string()), Some(harness.to_string())).await?;

    println!("Started process {} ({})", info.pid, info.name);
    println!("Working directory: {:?}", project_path);

    Ok(())
}

/// Stop processes
pub async fn stop(pid: Option<u32>, project: Option<&str>, harness: Option<&str>, all: bool, force: bool) -> Result<()> {
    let pool = ProcessPool::new();

    if all {
        println!("Stopping all managed processes...");
        pool.kill_all().await?;
        println!("All processes stopped.");
        return Ok(());
    }

    if let Some(p) = pid {
        println!("Stopping process {}...", p);
        pool.kill(p).await?;
        println!("Process {} stopped.", p);
        return Ok(());
    }

    let filter = if let Some(proj) = project {
        ProcessFilter::ByProject(proj.to_string())
    } else if let Some(h) = harness {
        ProcessFilter::ByHarness(h.to_string())
    } else {
        anyhow::bail!("Specify --pid, --project, --harness, or --all");
    };

    let processes = pool.find(filter).await;
    for proc in processes {
        pool.kill(proc.pid).await?;
        println!("Stopped {} ({})", proc.pid, proc.name);
    }

    Ok(())
}

/// Check process status
pub async fn status(verbose: bool) -> Result<()> {
    let pool = ProcessPool::new();
    let processes: Vec<ProcessInfo> = pool.list().await;

    let mut by_harness: std::collections::HashMap<&str, (usize, u64)> = std::collections::HashMap::new();

    for proc in &processes {
        let h = proc.harness.as_deref().unwrap_or("unknown");
        let entry = by_harness.entry(h).or_insert((0, 0));
        entry.0 += 1;
        entry.1 += proc.memory_mb;
    }

    println!("=== Process Status ===\n");
    println!("Total: {} processes\n", processes.len());

    println!("{:<15} {:<10} {:<15}", "HARNESS", "COUNT", "MEMORY(MB)");
    println!("{}", "-".repeat(40));

    for (h, (count, mem)) in by_harness.iter() {
        println!("{:<15} {:<10} {:<15}", h, count, mem);
    }

    if verbose {
        println!("\n=== Detailed Process List ===\n");
        for proc in &processes {
            println!("PID: {}, Name: {}, Memory: {} MB", proc.pid, proc.name, proc.memory_mb);
        }
    }

    Ok(())
}

/// Configuration management
pub fn config(cfg_cmd: &ConfigCmd) -> Result<()> {
    match cfg_cmd {
        ConfigCmd::Init => {
            Config::init()?;
            println!("Configuration initialized.");
        }
        ConfigCmd::Validate => {
            let cfg = Config::load()?;
            println!("Configuration is valid.");
            println!("  Projects: {}", cfg.projects.len());
        }
        ConfigCmd::Show => {
            let cfg = Config::load()?;
            let serialized = toml::to_string_pretty(&cfg)?;
            println!("{}", serialized);
        }
        ConfigCmd::Get { key: _ } => {
            let cfg = Config::load()?;
            println!("Projects:");
            for (name, path) in &cfg.projects {
                println!("  {} = {}", name, path);
            }
        }
        ConfigCmd::Set { .. } => {
            println!("Set not implemented yet.");
        }
    }
    Ok(())
}

/// Project management
pub fn project(proj_cmd: &ProjectCmd) -> Result<()> {
    match proj_cmd {
        ProjectCmd::Add { name, path } => {
            let mut cfg = Config::load()?;
            cfg.projects.insert(name.clone(), expand_path(path));
            cfg.save()?;
            println!("Added project '{}'", name);
        }
        ProjectCmd::Remove { name } => {
            let mut cfg = Config::load()?;
            if cfg.projects.remove(name).is_some() {
                cfg.save()?;
                println!("Removed project '{}'", name);
            }
        }
        ProjectCmd::List => {
            let cfg = Config::load()?;
            if cfg.projects.is_empty() {
                println!("No projects registered. Run 'sharecli project discover'.");
            } else {
                println!("Registered Projects:");
                for (name, path) in &cfg.projects {
                    println!("  {} -> {}", name, path);
                }
            }
        }
        ProjectCmd::Show { name } => {
            let cfg = Config::load()?;
            if let Some(path) = cfg.projects.get(name) {
                println!("Project: {}", name);
                println!("Path:    {}", path);
                println!("Exists:  {}", std::path::Path::new(path).exists());
            }
        }
        ProjectCmd::Discover { path } => {
            let base = PathBuf::from(expand_path(path.as_deref().unwrap_or("~/CodeProjects/Phenotype/repos")));
            println!("Scanning {:?} for projects...", base);

            let mut found = Vec::new();
            if let Ok(entries) = std::fs::read_dir(&base) {
                for entry in entries.flatten() {
                    let p = entry.path();
                    if p.is_dir() && p.join(".git").exists() {
                        let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("unknown").to_string();
                        found.push((name, p.to_string_lossy().to_string()));
                    }
                }
            }

            println!("\nFound {} projects:", found.len());
            for (name, path) in &found {
                println!("  {} -> {}", name, path);
            }
        }
    }
    Ok(())
}

fn expand_path(path: &str) -> String {
    if path.starts_with("~/") {
        if let Ok(home) = std::env::var("HOME") {
            return path.replacen("~/", &format!("{}/", home), 1);
        }
    }
    path.to_string()
}
