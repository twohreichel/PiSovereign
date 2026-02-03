//! Application configuration

use serde::{Deserialize, Serialize};

use ai_core::InferenceConfig;

/// Main application configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    /// Server configuration
    #[serde(default)]
    pub server: ServerConfig,

    /// Inference configuration
    #[serde(default)]
    pub inference: InferenceConfig,

    /// Security configuration
    #[serde(default)]
    pub security: SecurityConfig,
}

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
}

fn default_host() -> String {
    "0.0.0.0".to_string()
}

fn default_port() -> u16 {
    3000
}

fn default_true() -> bool {
    true
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            host: default_host(),
            port: default_port(),
            cors_enabled: true,
        }
    }
}

/// Security configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityConfig {
    /// Whitelisted phone numbers for WhatsApp
    #[serde(default)]
    pub whitelisted_phones: Vec<String>,

    /// API key for HTTP API (optional)
    #[serde(default)]
    pub api_key: Option<String>,

    /// Enable rate limiting
    #[serde(default = "default_true")]
    pub rate_limit_enabled: bool,

    /// Requests per minute per IP
    #[serde(default = "default_rate_limit")]
    pub rate_limit_rpm: u32,
}

fn default_rate_limit() -> u32 {
    60
}

impl Default for SecurityConfig {
    fn default() -> Self {
        Self {
            whitelisted_phones: Vec::new(),
            api_key: None,
            rate_limit_enabled: true,
            rate_limit_rpm: default_rate_limit(),
        }
    }
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            server: ServerConfig::default(),
            inference: InferenceConfig::default(),
            security: SecurityConfig::default(),
        }
    }
}

impl AppConfig {
    /// Load configuration from environment and optional file
    pub fn load() -> Result<Self, config::ConfigError> {
        let builder = config::Config::builder()
            // Start with defaults
            .set_default("server.host", "0.0.0.0")?
            .set_default("server.port", 3000)?
            .set_default("inference.base_url", "http://localhost:11434")?
            .set_default("inference.default_model", "qwen2.5-1.5b-instruct")?
            // Load from file if exists
            .add_source(config::File::with_name("config").required(false))
            // Override with environment variables (e.g., PISOVEREIGN_SERVER_PORT)
            .add_source(
                config::Environment::with_prefix("PISOVEREIGN")
                    .separator("_")
                    .try_parsing(true),
            );

        let config = builder.build()?;
        config.try_deserialize()
    }
}
