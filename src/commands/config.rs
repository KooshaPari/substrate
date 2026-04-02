//! `config` command - Configuration management

use clap::Parser;

#[derive(Parser, Debug)]
#[command(name = "cfg")]
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
