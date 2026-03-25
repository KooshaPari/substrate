//! `config` command - Configuration management

use crate::config::Config;
use anyhow::Result;
use clap::Parser;

#[derive(Parser, Debug)]
pub enum ConfigCmd {
    /// Initialize default configuration
    Init,

    /// Validate configuration
    Validate,

    /// Show current configuration
    Show,

    /// Get a configuration value
    Get {
        /// Configuration key (e.g., runtime.max_memory_mb)
        key: String,
    },

    /// Set a configuration value
    Set {
        /// Configuration key
        key: String,
        /// Value to set
        value: String,
    },
}

impl ConfigCmd {
    pub fn run(&self) -> Result<()> {
        match self {
            ConfigCmd::Init => {
                Config::init()?;
                println!("Configuration initialized.");
            }
            ConfigCmd::Validate => {
                let config = Config::load()?;
                println!("Configuration is valid.");
                println!("  Projects: {}", config.projects.len());
                println!("  Runtime: {:?}", config.runtime);
                println!("  Defaults: {:?}", config.defaults.keys().collect::<Vec<_>>());
            }
            ConfigCmd::Show => {
                let config = Config::load()?;
                let serialized = toml::to_string_pretty(&config)?;
                println!("{}", serialized);
            }
            ConfigCmd::Get { key } => {
                let config = Config::load()?;
                // Simple key parsing (supports "section.key" format)
                let parts: Vec<&str> = key.split('.').collect();
                match parts.as_slice() {
                    ["projects"] => {
                        for (name, path) in &config.projects {
                            println!("{} = {}", name, path);
                        }
                    }
                    ["runtime", subkey] => {
                        match *subkey {
                            "node_path" => println!("{}", config.runtime.node_path.as_deref().unwrap_or("not set")),
                            "bun_path" => println!("{}", config.runtime.bun_path.as_deref().unwrap_or("not set")),
                            "max_memory_mb" => println!("{:?}", config.runtime.max_memory_mb),
                            "max_processes" => println!("{:?}", config.runtime.max_processes),
                            _ => anyhow::bail!("Unknown runtime key: {}", subkey),
                        }
                    }
                    _ => anyhow::bail!("Unsupported key format. Use 'projects' or 'runtime.<subkey>'"),
                }
            }
            ConfigCmd::Set { key, value } => {
                let mut config = Config::load()?;
                let parts: Vec<&str> = key.split('.').collect();
                match parts.as_slice() {
                    ["runtime", subkey] => {
                        match *subkey {
                            "node_path" => config.runtime.node_path = Some(value.clone()),
                            "bun_path" => config.runtime.bun_path = Some(value.clone()),
                            "max_memory_mb" => config.runtime.max_memory_mb = value.parse().ok(),
                            "max_processes" => config.runtime.max_processes = value.parse().ok(),
                            _ => anyhow::bail!("Unknown runtime key: {}", subkey),
                        }
                    }
                    _ => anyhow::bail!("Unsupported key. Use 'runtime.<subkey>'"),
                }
                config.save()?;
                println!("Set {} = {}", key, value);
            }
        }
        Ok(())
    }
}
