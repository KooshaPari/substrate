//! Configuration management for sharecli

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// Project name to path mappings
    pub projects: HashMap<String, String>,

    /// Runtime configuration
    pub runtime: RuntimeConfig,

    /// Default harness settings
    pub defaults: HashMap<String, HarnessConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeConfig {
    /// Path to node executable
    pub node_path: Option<String>,
    /// Path to bun executable
    pub bun_path: Option<String>,
    /// Maximum memory in MB
    pub max_memory_mb: Option<u64>,
    /// Maximum processes
    pub max_processes: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HarnessConfig {
    /// Enable this harness type
    pub enabled: bool,
    /// Maximum instances
    pub max_instances: u32,
    /// Memory limit per instance in MB
    pub memory_limit_mb: Option<u64>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            projects: default_projects(),
            runtime: RuntimeConfig::default(),
            defaults: default_harnesses(),
        }
    }
}

impl Default for RuntimeConfig {
    fn default() -> Self {
        Self {
            node_path: Some("/opt/homebrew/bin/node".to_string()),
            bun_path: Some("/opt/homebrew/bin/bun".to_string()),
            max_memory_mb: Some(4096),
            max_processes: Some(50),
        }
    }
}

impl Default for HarnessConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            max_instances: 5,
            memory_limit_mb: Some(512),
        }
    }
}

fn default_projects() -> HashMap<String, String> {
    let mut projects = HashMap::new();
    projects.insert(
        "portage".to_string(),
        "~/CodeProjects/Phenotype/repos/portage".to_string(),
    );
    projects.insert(
        "helios-cli".to_string(),
        "~/CodeProjects/Phenotype/repos/helios-cli".to_string(),
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

fn default_harnesses() -> HashMap<String, HarnessConfig> {
    let mut defaults = HashMap::new();
    defaults.insert(
        "claude".to_string(),
        HarnessConfig {
            enabled: true,
            max_instances: 11,
            memory_limit_mb: Some(512),
        },
    );
    defaults.insert(
        "forge".to_string(),
        HarnessConfig {
            enabled: true,
            max_instances: 20,
            memory_limit_mb: Some(256),
        },
    );
    defaults.insert(
        "node".to_string(),
        HarnessConfig {
            enabled: true,
            max_instances: 30,
            memory_limit_mb: Some(256),
        },
    );
    defaults
}

impl Config {
    /// Load configuration from standard locations
    pub fn load() -> Result<Self> {
        let config_path = Self::config_path()?;

        if config_path.exists() {
            let content = std::fs::read_to_string(&config_path)
                .context("Failed to read config file")?;
            toml::from_str(&content).context("Failed to parse config")
        } else {
            // Return default config if no file exists
            Ok(Config::default())
        }
    }

    /// Save configuration to standard location
    pub fn save(&self) -> Result<()> {
        let config_path = Self::config_path()?;
        if let Some(parent) = config_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let content = toml::to_string_pretty(self)
            .context("Failed to serialize config")?;
        std::fs::write(&config_path, content)
            .context("Failed to write config file")?;
        Ok(())
    }

    /// Get the standard configuration path
    fn config_path() -> Result<PathBuf> {
        let config_dir = dirs::config_dir()
            .context("Could not find config directory")?;
        Ok(config_dir.join("sharecli").join("sharecli.toml"))
    }

    /// Initialize with default configuration
    pub fn init() -> Result<()> {
        let config = Config::default();
        config.save()?;
        println!("Initialized sharecli configuration at {:?}", Self::config_path()?);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = Config::default();
        assert!(!config.projects.is_empty());
        assert!(config.projects.contains_key("portage"));
        assert!(config.defaults.contains_key("claude"));
    }

    #[test]
    fn test_config_serialization() {
        let config = Config::default();
        let serialized = toml::to_string(&config).unwrap();
        let deserialized: Config = toml::from_str(&serialized).unwrap();
        assert_eq!(config.projects.len(), deserialized.projects.len());
    }
}
