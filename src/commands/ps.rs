//! `ps` command - List managed processes

use crate::runtime::{ProcessFilter, ProcessPool};
use clap::Parser;

#[derive(Parser, Debug)]
pub struct Ps {
    /// Filter by project name
    #[arg(short, long)]
    project: Option<String>,

    /// Filter by harness type (claude, forge, node)
    #[arg(short, long)]
    harness: Option<String>,

    /// Show all processes (including unmanaged)
    #[arg(short, long)]
    all: bool,
}

impl Ps {
    pub async fn run(&self) -> anyhow::Result<()> {
        let pool = ProcessPool::new();

        let filter = if let Some(ref project) = self.project {
            ProcessFilter::ByProject(project.clone())
        } else if let Some(ref harness) = self.harness {
            ProcessFilter::ByHarness(harness.clone())
        } else {
            ProcessFilter::All
        };

        let processes = pool.find(filter).await;

        if processes.is_empty() {
            println!("No managed processes found.");
            return Ok(());
        }

        // Print header
        println!("{:<10} {:<20} {:<12} {:<15} {:<15}", "PID", "NAME", "MEM(MB)", "PROJECT", "HARNESS");
        println!("{}", "-".repeat(75));

        for proc in processes {
            println!(
                "{:<10} {:<20} {:<12.1} {:<15} {:<15}",
                proc.pid,
                truncate(&proc.name, 18),
                proc.memory_mb,
                proc.project.as_deref().unwrap_or("-"),
                proc.harness.as_deref().unwrap_or("-")
            );
        }

        // Summary
        let total_mem: u64 = processes.iter().map(|p| p.memory_mb).sum();
        println!("\nTotal: {} processes, {} MB memory", processes.len(), total_mem);

        Ok(())
    }
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() > max {
        format!("{}...", &s[..max - 3])
    } else {
        s.to_string()
    }
}
