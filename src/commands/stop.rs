//! `stop` command - Stop managed processes

use crate::runtime::ProcessPool;
use anyhow::Result;
use clap::Parser;

#[derive(Parser, Debug)]
pub struct Stop {
    /// Process ID to stop
    #[arg(short, long)]
    pid: Option<u32>,

    /// Project to stop all processes for
    #[arg(short, long)]
    project: Option<String>,

    /// Harness type to stop
    #[arg(short, long)]
    harness: Option<String>,

    /// Stop all managed processes
    #[arg(short, long)]
    all: bool,

    /// Force kill (SIGKILL)
    #[arg(short, long)]
    force: bool,
}

impl Stop {
    pub async fn run(&self) -> Result<()> {
        let pool = ProcessPool::new();

        if self.all {
            println!("Stopping all managed processes...");
            let processes = pool.list().await;

            for proc in processes {
                if self.force {
                    pool.kill(proc.pid).await?;
                    println!("Force killed process {}", proc.pid);
                } else {
                    // Send SIGTERM first
                    pool.kill(proc.pid).await?;
                    println!("Stopped process {}", proc.pid);
                }
            }

            pool.kill_all().await?;
            println!("All processes stopped.");
            return Ok(());
        }

        if let Some(pid) = self.pid {
            println!("Stopping process {}...", pid);
            pool.kill(pid).await?;
            println!("Process {} stopped.", pid);
            return Ok(());
        }

        if let Some(ref project) = self.project {
            println!("Stopping processes for project '{}'...", project);
            // Find and kill all processes for this project
            let procs = pool.find(crate::runtime::ProcessFilter::ByProject(project.clone())).await;
            for proc in procs {
                pool.kill(proc.pid).await?;
                println!("Stopped process {} ({})", proc.pid, proc.name);
            }
            return Ok(());
        }

        if let Some(ref harness) = self.harness {
            println!("Stopping {} harness processes...", harness);
            let procs = pool.find(crate::runtime::ProcessFilter::ByHarness(harness.clone())).await;
            for proc in procs {
                pool.kill(proc.pid).await?;
                println!("Stopped process {} ({})", proc.pid, proc.name);
            }
            return Ok(());
        }

        anyhow::bail!("Specify --pid, --project, --harness, or --all");
    }
}
