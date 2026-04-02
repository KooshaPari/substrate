//! Configuration management for sharecli

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// Registered projects (name -> path)
    pub projects: HashMap<String, String>,

    /// Runtime settings
    pub runtime: RuntimeConfig,

    /// Default settings
    pub defaults: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeConfig {
    /// Path to node executable
    pub node_path: Option<String>,
    /// Path to bun executable  
    pub bun_path: Option<String>,
    /// Maximum memory per process (MB)
    pub max_memory_mb: Option<u64>,
    /// Maximum number of processes
    pub max_processes: Option<usize>,
}

impl Default for RuntimeConfig {
    fn default() -> Self {
        Self {
            node_path: None,
            bun_path: None,
            max_memory_mb: Some(4096),
            max_processes: Some(100),
        }
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            projects: default_projects(),
            runtime: RuntimeConfig::default(),
            defaults: HashMap::new(),
        }
    }
}

fn default_projects() -> HashMap<String, String> {
    let mut projects = HashMap::new();
    projects.insert(
        "helios-cli".to_string(),
        "~/CodeProjects/Phenotype/repos/helios-cli".to_string(),
    );
    projects.insert(
        "portage".to_string(),
        "~/CodeProjects/Phenotype/repos/portage".to_string(),
    );
    projects.insert(
        "agentapi".to_string(),
        "~/CodeProjects/Phenotype/repos/agentapi-plusplus".to_string(),
    );
    projects.insert(
        "cliproxy".to_string(),
        "~/CodeProjects/Phenotype/repos/cliproxyapi-plusplus".to_string(),
    );
    projects.insert(
        "colab".to_string(),
        "~/CodeProjects/Phenotype/repos/colab".to_string(),
    );
    projects
}

impl Config {
    /// Load configuration from ~/.config/sharecli/config.toml
    pub fn load() -> Result<Self> {
        let config_path = Self::config_path()?;

        if config_path.exists() {
            let contents = std::fs::read_to_string(&config_path)?;
            let config: Config = toml::from_str(&contents)?;
            Ok(config)
        } else {
            Ok(Config::default())
        }
    }

    /// Initialize default configuration
    pub fn init() -> Result<()> {
        let config_path = Self::config_path()?;

        if let Some(parent) = config_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let config = Config::default();
        let contents = toml::to_string_pretty(&config)?;
        std::fs::write(&config_path, contents)?;

        Ok(())
    }

    /// Save configuration
    pub fn save(&self) -> Result<()> {
        let config_path = Self::config_path()?;

        if let Some(parent) = config_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let contents = toml::to_string_pretty(self)?;
        std::fs::write(&config_path, contents)?;

        Ok(())
    }

    /// Get config file path
    fn config_path() -> Result<PathBuf> {
        let base =
            dirs::config_dir().ok_or_else(|| anyhow::anyhow!("Could not find config directory"))?;
        Ok(base.join("sharecli").join("config.toml"))
    }
}

// CLI command enums (defined here to avoid circular dependencies)

#[derive(clap::Subcommand, Debug)]
pub enum ConfigCmd {
    /// Initialize default configuration
    Init,
    /// Validate configuration
    Validate,
    /// Show current configuration
    Show,
    /// Get a configuration value
    Get { key: String },
    /// Set a configuration value
    Set { key: String, value: String },
}

#[derive(clap::Subcommand, Debug)]
pub enum ProjectCmd {
    /// Add a project to the registry
    Add { name: String, path: String },
    /// Remove a project from the registry
    Remove { name: String },
    /// List all registered projects
    List,
    /// Show project details
    Show { name: String },
    /// Discover projects in a directory
    /// Discover projects in a directory
    Discover { path: Option<String> },
    /// Generate process-compose.yml from registered projects
    Generate { output: Option<String> },
}
