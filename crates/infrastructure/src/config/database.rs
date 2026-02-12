//! Database (SQLite) configuration.

use serde::{Deserialize, Serialize};

use super::default_true;

/// SQLite database configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatabaseConfig {
    /// Path to the SQLite database file
    #[serde(default = "default_db_path")]
    pub path: String,

    /// Maximum number of concurrent database connections
    #[serde(default = "default_max_connections")]
    pub max_connections: u32,

    /// Whether to run pending migrations on startup (default: true)
    #[serde(default = "default_true")]
    pub run_migrations: bool,
}

fn default_db_path() -> String {
    "pisovereign.db".to_string()
}

const fn default_max_connections() -> u32 {
    5
}

impl Default for DatabaseConfig {
    fn default() -> Self {
        Self {
            path: default_db_path(),
            max_connections: default_max_connections(),
            run_migrations: true,
        }
    }
}
