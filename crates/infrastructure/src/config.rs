//! Application configuration

use ai_core::InferenceConfig;
use serde::{Deserialize, Serialize};

/// Main application configuration
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
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

const fn default_port() -> u16 {
    3000
}

const fn default_true() -> bool {
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

const fn default_rate_limit() -> u32 {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_config_default() {
        let config = AppConfig::default();
        assert_eq!(config.server.port, 3000);
        assert_eq!(config.server.host, "0.0.0.0");
        assert!(config.server.cors_enabled);
    }

    #[test]
    fn server_config_default() {
        let config = ServerConfig::default();
        assert_eq!(config.host, "0.0.0.0");
        assert_eq!(config.port, 3000);
        assert!(config.cors_enabled);
    }

    #[test]
    fn security_config_default() {
        let config = SecurityConfig::default();
        assert!(config.whitelisted_phones.is_empty());
        assert!(config.api_key.is_none());
        assert!(config.rate_limit_enabled);
        assert_eq!(config.rate_limit_rpm, 60);
    }

    #[test]
    fn app_config_serialization() {
        let config = AppConfig::default();
        let json = serde_json::to_string(&config).unwrap();
        assert!(json.contains("server"));
        assert!(json.contains("inference"));
        assert!(json.contains("security"));
    }

    #[test]
    fn app_config_deserialization() {
        let json = r#"{"server":{"port":8080}}"#;
        let config: AppConfig = serde_json::from_str(json).unwrap();
        assert_eq!(config.server.port, 8080);
        assert_eq!(config.server.host, "0.0.0.0");
    }

    #[test]
    fn server_config_serialization() {
        let config = ServerConfig::default();
        let json = serde_json::to_string(&config).unwrap();
        assert!(json.contains("host"));
        assert!(json.contains("port"));
        assert!(json.contains("cors_enabled"));
    }

    #[test]
    fn security_config_with_phones() {
        let json = r#"{"whitelisted_phones":["+491234567890"]}"#;
        let config: SecurityConfig = serde_json::from_str(json).unwrap();
        assert_eq!(config.whitelisted_phones.len(), 1);
        assert_eq!(config.whitelisted_phones[0], "+491234567890");
    }

    #[test]
    fn security_config_with_api_key() {
        let json = r#"{"api_key":"secret123"}"#;
        let config: SecurityConfig = serde_json::from_str(json).unwrap();
        assert_eq!(config.api_key, Some("secret123".to_string()));
    }

    #[test]
    fn config_has_debug_impl() {
        let config = AppConfig::default();
        let debug = format!("{config:?}");
        assert!(debug.contains("AppConfig"));
        assert!(debug.contains("server"));
    }

    #[test]
    fn config_clone() {
        let config = AppConfig::default();
        let cloned = config.clone();
        assert_eq!(config.server.port, cloned.server.port);
    }

    #[test]
    fn server_config_clone() {
        let config = ServerConfig::default();
        let cloned = config.clone();
        assert_eq!(config.host, cloned.host);
    }

    #[test]
    fn security_config_clone() {
        let config = SecurityConfig::default();
        let cloned = config.clone();
        assert_eq!(config.rate_limit_enabled, cloned.rate_limit_enabled);
    }

    #[test]
    fn security_config_serialization() {
        let config = SecurityConfig::default();
        let json = serde_json::to_string(&config).unwrap();
        assert!(json.contains("whitelisted_phones"));
        assert!(json.contains("rate_limit_enabled"));
    }

    #[test]
    fn server_config_debug() {
        let config = ServerConfig::default();
        let debug = format!("{config:?}");
        assert!(debug.contains("ServerConfig"));
    }

    #[test]
    fn security_config_debug() {
        let config = SecurityConfig::default();
        let debug = format!("{config:?}");
        assert!(debug.contains("SecurityConfig"));
    }

    #[test]
    fn app_config_with_custom_port() {
        let json = r#"{"server":{"port":4000,"host":"127.0.0.1"}}"#;
        let config: AppConfig = serde_json::from_str(json).unwrap();
        assert_eq!(config.server.port, 4000);
        assert_eq!(config.server.host, "127.0.0.1");
    }

    #[test]
    fn security_config_rate_limit_disabled() {
        let json = r#"{"rate_limit_enabled":false,"rate_limit_rpm":120}"#;
        let config: SecurityConfig = serde_json::from_str(json).unwrap();
        assert!(!config.rate_limit_enabled);
        assert_eq!(config.rate_limit_rpm, 120);
    }
}
