//! HTTP server configuration.

use serde::{Deserialize, Serialize};

use super::default_true;

/// HTTP server configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    /// Host to bind to
    #[serde(default = "default_host")]
    pub host: String,

    /// Port to bind to
    #[serde(default = "default_port")]
    pub port: u16,

    /// Enable CORS
    #[serde(default = "default_true")]
    pub cors_enabled: bool,

    /// Allowed CORS origins (empty = allow all in dev, specific origins in production)
    #[serde(default)]
    pub allowed_origins: Vec<String>,

    /// Graceful shutdown timeout in seconds
    #[serde(default)]
    pub shutdown_timeout_secs: Option<u64>,

    /// Log format: "json" for structured JSON logs, "text" for human-readable
    #[serde(default = "default_log_format")]
    pub log_format: String,

    /// Maximum body size for audio uploads in bytes (default: 10MB)
    #[serde(default = "default_max_body_audio")]
    pub max_body_size_audio_bytes: usize,

    /// Maximum body size for JSON requests in bytes (default: 1MB)
    #[serde(default = "default_max_body_json")]
    pub max_body_size_json_bytes: usize,
}

fn default_host() -> String {
    "127.0.0.1".to_string()
}

const fn default_max_body_audio() -> usize {
    10 * 1024 * 1024 // 10MB
}

const fn default_max_body_json() -> usize {
    1024 * 1024 // 1MB
}

const fn default_port() -> u16 {
    3000
}

fn default_log_format() -> String {
    "text".to_string()
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            host: default_host(),
            port: default_port(),
            cors_enabled: true,
            allowed_origins: Vec::new(),
            shutdown_timeout_secs: Some(30),
            log_format: default_log_format(),
            max_body_size_audio_bytes: default_max_body_audio(),
            max_body_size_json_bytes: default_max_body_json(),
        }
    }
}
