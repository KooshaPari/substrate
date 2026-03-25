//! sharecli - Shared CLI process manager

use anyhow::Result;
use clap::{Parser, Subcommand};

mod commands;
mod config;
mod monitoring;
mod projects;
mod runtime;

#[derive(Parser, Debug)]
#[command(
    name = "sharecli",
    about = "Shared CLI process manager for multi-project agent orchestration",
    version = "0.1.0",
    author = "Phenotype"
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    /// Enable verbose output
    #[arg(short, long)]
    verbose: bool,

    /// Quiet mode
    #[arg(short, long)]
    quiet: bool,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// List managed processes
    Ps(commands::Ps),

    /// Start a harness process
    Start(commands::Start),

    /// Stop managed processes
    Stop(commands::Stop),

    /// Check process health
    Status(commands::Status),

    /// Configuration management
    Config(commands::ConfigCmd),

    /// Project management
    Project(commands::ProjectCmd),

    /// Optimize resource usage
    Optimize {
        /// Apply optimizations automatically
        #[arg(short, long)]
        apply: bool,
    },

    /// Prune idle processes
    Prune {
        /// Idle time threshold in seconds
        #[arg(short, long, default_value = "300")]
        idle_seconds: u64,

        /// Actually kill processes (dry run by default)
        #[arg(short, long)]
        force: bool,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    // Initialize logging
    if !cli.quiet {
        tracing_subscriber::fmt()
            .with_max_level(if cli.verbose {
                tracing::Level::DEBUG
            } else {
                tracing::Level::INFO
            })
            .init();
    }

    match &cli.command {
        Commands::Ps(cmd) => cmd.run().await?,
        Commands::Start(cmd) => cmd.run().await?,
        Commands::Stop(cmd) => cmd.run().await?,
        Commands::Status(cmd) => cmd.run().await?,
        Commands::Config(cmd) => cmd.run()?,
        Commands::Project(cmd) => cmd.run()?,
        Commands::Optimize { apply } => optimize(*apply).await?,
        Commands::Prune { idle_seconds, force } => prune(*idle_seconds, *force).await?,
    }

    Ok(())
}

async fn optimize(apply: bool) -> Result<()> {
    println!("Analyzing resource usage...");

    // Get current stats
    let pool = runtime::ProcessPool::new();
    let processes = pool.list().await;

    let mut by_harness: std::collections::HashMap<&str, (usize, u64)> = std::collections::HashMap::new();

    for proc in &processes {
        if let Some(ref harness) = proc.harness {
            let entry = by_harness.entry(harness.as_str()).or_insert((0, 0));
            entry.0 += 1;
            entry.1 += proc.memory_mb;
        }
    }

    println!("\nCurrent resource usage:");
    println!("{:<15} {:<10} {:<15}", "HARNESS", "COUNT", "MEMORY(MB)");
    println!("{}", "-".repeat(40));

    for (harness, (count, mem)) in &by_harness {
        println!("{:<15} {:<10} {:<15}", harness, count, mem);
    }

    let total_mem: u64 = by_harness.values().map(|(_, m)| m).sum();
    let total_count: usize = by_harness.values().map(|(c, _)| c).sum();

    println!("\n{:<15} {:<10} {:<15}", "TOTAL", total_count, total_mem);

    // Suggest optimizations
    println!("\n=== Optimization Suggestions ===");

    if total_count > 30 {
        println!("- Consider reducing max instances per harness");
    }

    if total_mem > 4096 {
        println!("- Memory usage is high ({} MB). Consider pruning idle processes.", total_mem);
    }

    // Check for duplicate node/bun instances
    let node_count = by_harness.get("node").map(|(c, _)| *c).unwrap_or(0);
    if node_count > 10 {
        println!("- Multiple node processes ({}). Consider sharing a single instance.".format(node_count));
    }

    if apply {
        println!("\nApplying optimizations...");
        // TODO: Implement actual optimization
        println!("Done.");
    }

    Ok(())
}

async fn prune(idle_seconds: u64, force: bool) -> Result<()> {
    println!("Pruning idle processes (threshold: {}s)...", idle_seconds);

    let pool = runtime::ProcessPool::new();
    let mut sys = sysinfo::System::new_all();
    sys.refresh_all();

    let processes = pool.list().await;
    let mut pruned = 0;
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();

    for proc in processes {
        // Check if process is idle (low CPU for threshold time)
        // For now, just kill based on threshold
        if proc.start_time > 0 && (now - proc.start_time) > idle_seconds {
            if force {
                pool.kill(proc.pid).await?;
                println!("Pruned process {} ({})", proc.pid, proc.name);
            } else {
                println!("Would prune: {} ({})", proc.pid, proc.name);
            }
            pruned += 1;
        }
    }

    if force {
        println!("\nPruned {} processes.", pruned);
    } else {
        println!("\nWould prune {} processes (use --force to apply).", pruned);
    }

    Ok(())
}
