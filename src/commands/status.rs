//! `status` command - Check process health

use crate::runtime::ProcessPool;
use anyhow::Result;
use clap::Parser;
use sysinfo::System;

#[derive(Parser, Debug)]
pub struct Status {
    /// Project name to check
    #[arg(short, long)]
    project: Option<String>,

    /// Detailed output
    #[arg(short, long)]
    verbose: bool,
}

impl Status {
    pub async fn run(&self) -> Result<()> {
        let pool = ProcessPool::new();

        // Get system info
        let mut sys = System::new_all();
        sys.refresh_all();

        // Memory info
        let total_mem = sys.total_memory() / 1024 / 1024;
        let used_mem = sys.used_memory() / 1024 / 1024;
        let available_mem = total_mem - used_mem;

        println!("=== System Status ===");
        println!("Total Memory: {} MB", total_mem);
        println!("Used Memory:  {} MB ({:.1}%)", used_mem, (used_mem as f64 / total_mem as f64) * 100.0);
        println!("Available:    {} MB", available_mem);

        // Process count
        let processes = pool.list().await;
        let managed_count = processes.len();

        println!("\n=== Managed Processes ===");
        println!("Active processes: {}", managed_count);

        // By project
        if !processes.is_empty() {
            let mut by_project: std::collections::HashMap<&str, usize> = std::collections::HashMap::new();
            let mut by_harness: std::collections::HashMap<&str, usize> = std::collections::HashMap::new();
            let mut total_memory = 0u64;

            for proc in &processes {
                if let Some(ref proj) = proc.project {
                    *by_project.entry(proj.as_str()).or_insert(0) += 1;
                }
                if let Some(ref harness) = proc.harness {
                    *by_harness.entry(harness.as_str()).or_insert(0) += 1;
                }
                total_memory += proc.memory_mb;
            }

            println!("Total managed memory: {} MB", total_memory);

            if self.verbose {
                println!("\nBy Project:");
                for (proj, count) in &by_project {
                    println!("  {}: {} processes", proj, count);
                }

                println!("\nBy Harness:");
                for (harness, count) in &by_harness {
                    println!("  {}: {} processes", harness, count);
                }
            }
        }

        // Health check
        println!("\n=== Health ===");
        let healthy = if managed_count > 0 {
            "OK"
        } else {
            "No managed processes"
        };
        println!("Status: {}", healthy);

        Ok(())
    }
}
