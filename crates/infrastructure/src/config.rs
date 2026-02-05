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

    /// WhatsApp configuration
    #[serde(default)]
    pub whatsapp: WhatsAppConfig,

    /// Database configuration
    #[serde(default)]
    pub database: DatabaseConfig,

    /// Cache configuration
    #[serde(default)]
    pub cache: CacheConfig,

    /// Telemetry configuration (optional)
    #[serde(default)]
    pub telemetry: Option<TelemetryAppConfig>,

    /// Degraded mode configuration (optional)
    #[serde(default)]
    pub degraded_mode: Option<DegradedModeAppConfig>,
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

    /// Allowed CORS origins (empty = allow all in dev, specific origins in production)
    #[serde(default)]
    pub allowed_origins: Vec<String>,

    /// Graceful shutdown timeout in seconds
    #[serde(default)]
    pub shutdown_timeout_secs: Option<u64>,
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
            allowed_origins: Vec::new(),
            shutdown_timeout_secs: Some(30),
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

    /// Rate limiter cleanup interval in seconds (default: 300 = 5 minutes)
    #[serde(default = "default_cleanup_interval")]
    pub rate_limit_cleanup_interval_secs: u64,

    /// Rate limiter entry max age in seconds before cleanup (default: 600 = 10 minutes)
    #[serde(default = "default_cleanup_max_age")]
    pub rate_limit_cleanup_max_age_secs: u64,

    /// Validate TLS certificates for outbound connections
    #[serde(default = "default_true")]
    pub tls_verify_certs: bool,

    /// Connection timeout in seconds for external services
    #[serde(default = "default_connection_timeout")]
    pub connection_timeout_secs: u64,

    /// Minimum TLS version (1.2 or 1.3)
    #[serde(default = "default_min_tls_version")]
    pub min_tls_version: String,
}

const fn default_rate_limit() -> u32 {
    60
}

const fn default_cleanup_interval() -> u64 {
    300 // 5 minutes
}

const fn default_cleanup_max_age() -> u64 {
    600 // 10 minutes
}

const fn default_connection_timeout() -> u64 {
    30
}

fn default_min_tls_version() -> String {
    "1.2".to_string()
}

impl Default for SecurityConfig {
    fn default() -> Self {
        Self {
            whitelisted_phones: Vec::new(),
            api_key: None,
            rate_limit_enabled: true,
            rate_limit_rpm: default_rate_limit(),
            rate_limit_cleanup_interval_secs: default_cleanup_interval(),
            rate_limit_cleanup_max_age_secs: default_cleanup_max_age(),
            tls_verify_certs: true,
            connection_timeout_secs: default_connection_timeout(),
            min_tls_version: default_min_tls_version(),
        }
    }
}

/// Cache configuration with TTL settings per cache type
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheConfig {
    /// Whether caching is enabled
    #[serde(default = "default_true")]
    pub enabled: bool,

    /// Short TTL in seconds (for frequently changing data, default: 5 minutes)
    #[serde(default = "default_cache_ttl_short")]
    pub ttl_short_secs: u64,

    /// Medium TTL in seconds (for moderately stable data, default: 1 hour)
    #[serde(default = "default_cache_ttl_medium")]
    pub ttl_medium_secs: u64,

    /// Long TTL in seconds (for stable data, default: 24 hours)
    #[serde(default = "default_cache_ttl_long")]
    pub ttl_long_secs: u64,

    /// TTL for dynamic LLM responses in seconds (high temperature, default: 1 hour)
    #[serde(default = "default_cache_ttl_medium")]
    pub ttl_llm_dynamic_secs: u64,

    /// TTL for stable LLM responses in seconds (low temperature, default: 24 hours)
    #[serde(default = "default_cache_ttl_long")]
    pub ttl_llm_stable_secs: u64,

    /// Maximum number of entries in L1 (in-memory) cache
    #[serde(default = "default_l1_max_entries")]
    pub l1_max_entries: u64,
}

const fn default_cache_ttl_short() -> u64 {
    5 * 60 // 5 minutes
}

const fn default_cache_ttl_medium() -> u64 {
    60 * 60 // 1 hour
}

const fn default_cache_ttl_long() -> u64 {
    24 * 60 * 60 // 24 hours
}

const fn default_l1_max_entries() -> u64 {
    10_000
}

impl Default for CacheConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            ttl_short_secs: default_cache_ttl_short(),
            ttl_medium_secs: default_cache_ttl_medium(),
            ttl_long_secs: default_cache_ttl_long(),
            ttl_llm_dynamic_secs: default_cache_ttl_medium(),
            ttl_llm_stable_secs: default_cache_ttl_long(),
            l1_max_entries: default_l1_max_entries(),
        }
    }
}

impl CacheConfig {
    /// Get the short TTL as a Duration
    #[must_use]
    pub const fn ttl_short(&self) -> std::time::Duration {
        std::time::Duration::from_secs(self.ttl_short_secs)
    }

    /// Get the medium TTL as a Duration
    #[must_use]
    pub const fn ttl_medium(&self) -> std::time::Duration {
        std::time::Duration::from_secs(self.ttl_medium_secs)
    }

    /// Get the long TTL as a Duration
    #[must_use]
    pub const fn ttl_long(&self) -> std::time::Duration {
        std::time::Duration::from_secs(self.ttl_long_secs)
    }

    /// Get the LLM dynamic TTL as a Duration
    #[must_use]
    pub const fn ttl_llm_dynamic(&self) -> std::time::Duration {
        std::time::Duration::from_secs(self.ttl_llm_dynamic_secs)
    }

    /// Get the LLM stable TTL as a Duration
    #[must_use]
    pub const fn ttl_llm_stable(&self) -> std::time::Duration {
        std::time::Duration::from_secs(self.ttl_llm_stable_secs)
    }
}

/// WhatsApp integration configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WhatsAppConfig {
    /// Meta Graph API access token
    #[serde(default)]
    pub access_token: Option<String>,

    /// Phone number ID from WhatsApp Business
    #[serde(default)]
    pub phone_number_id: Option<String>,

    /// App secret for webhook signature verification
    #[serde(default)]
    pub app_secret: Option<String>,

    /// Verify token for webhook setup
    #[serde(default)]
    pub verify_token: Option<String>,

    /// Whether signature verification is required (default: true)
    #[serde(default = "default_true")]
    pub signature_required: bool,

    /// API version (default: v18.0)
    #[serde(default = "default_api_version")]
    pub api_version: String,
}

fn default_api_version() -> String {
    "v18.0".to_string()
}

impl Default for WhatsAppConfig {
    fn default() -> Self {
        Self {
            access_token: None,
            phone_number_id: None,
            app_secret: None,
            verify_token: None,
            signature_required: true,
            api_version: default_api_version(),
        }
    }
}

/// Database configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatabaseConfig {
    /// Path to the SQLite database file
    #[serde(default = "default_db_path")]
    pub path: String,

    /// Maximum number of connections in the pool
    #[serde(default = "default_max_connections")]
    pub max_connections: u32,

    /// Whether to run migrations on startup
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
        #[allow(clippy::redundant_clone)]
        let cloned = config.clone();
        assert_eq!(config.server.port, cloned.server.port);
    }

    #[test]
    fn server_config_clone() {
        let config = ServerConfig::default();
        #[allow(clippy::redundant_clone)]
        let cloned = config.clone();
        assert_eq!(config.host, cloned.host);
    }

    #[test]
    fn security_config_clone() {
        let config = SecurityConfig::default();
        #[allow(clippy::redundant_clone)]
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

    #[test]
    fn telemetry_config_deserialize() {
        let json = r#"{"enabled":true,"otlp_endpoint":"http://tempo:4317"}"#;
        let config: TelemetryAppConfig = serde_json::from_str(json).unwrap();
        assert!(config.enabled);
        assert_eq!(config.otlp_endpoint, "http://tempo:4317");
    }

    #[test]
    fn degraded_mode_config_deserialize() {
        let json = r#"{"enabled":true,"failure_threshold":5}"#;
        let config: DegradedModeAppConfig = serde_json::from_str(json).unwrap();
        assert!(config.enabled);
        assert_eq!(config.failure_threshold, 5);
    }

    #[test]
    fn cache_config_default() {
        let config = CacheConfig::default();
        assert!(config.enabled);
        assert_eq!(config.ttl_short_secs, 5 * 60);
        assert_eq!(config.ttl_medium_secs, 60 * 60);
        assert_eq!(config.ttl_long_secs, 24 * 60 * 60);
        assert_eq!(config.ttl_llm_dynamic_secs, 60 * 60);
        assert_eq!(config.ttl_llm_stable_secs, 24 * 60 * 60);
        assert_eq!(config.l1_max_entries, 10_000);
    }

    #[test]
    fn cache_config_ttl_durations() {
        let config = CacheConfig::default();
        assert_eq!(config.ttl_short().as_secs(), 5 * 60);
        assert_eq!(config.ttl_medium().as_secs(), 60 * 60);
        assert_eq!(config.ttl_long().as_secs(), 24 * 60 * 60);
        assert_eq!(config.ttl_llm_dynamic().as_secs(), 60 * 60);
        assert_eq!(config.ttl_llm_stable().as_secs(), 24 * 60 * 60);
    }

    #[test]
    fn cache_config_deserialize() {
        let json = r#"{"enabled":false,"ttl_short_secs":60,"l1_max_entries":5000}"#;
        let config: CacheConfig = serde_json::from_str(json).unwrap();
        assert!(!config.enabled);
        assert_eq!(config.ttl_short_secs, 60);
        assert_eq!(config.l1_max_entries, 5000);
        // Defaults should still apply for unspecified fields
        assert_eq!(config.ttl_medium_secs, 60 * 60);
    }

    #[test]
    fn cache_config_serialization() {
        let config = CacheConfig::default();
        let json = serde_json::to_string(&config).unwrap();
        assert!(json.contains("enabled"));
        assert!(json.contains("ttl_short_secs"));
        assert!(json.contains("l1_max_entries"));
    }
}

/// Telemetry configuration for OpenTelemetry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TelemetryAppConfig {
    /// Enable telemetry
    #[serde(default)]
    pub enabled: bool,

    /// OTLP endpoint URL
    #[serde(default = "default_otlp_endpoint")]
    pub otlp_endpoint: String,

    /// Sample ratio (0.0 to 1.0)
    #[serde(default)]
    pub sample_ratio: Option<f64>,
}

fn default_otlp_endpoint() -> String {
    "http://localhost:4317".to_string()
}

impl Default for TelemetryAppConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            otlp_endpoint: default_otlp_endpoint(),
            sample_ratio: Some(1.0),
        }
    }
}

/// Degraded mode configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DegradedModeAppConfig {
    /// Enable degraded mode fallback
    #[serde(default = "default_true")]
    pub enabled: bool,

    /// Message to return when service is unavailable
    #[serde(default = "default_unavailable_message")]
    pub unavailable_message: String,

    /// Cooldown before retrying primary backend (seconds)
    #[serde(default = "default_retry_cooldown")]
    pub retry_cooldown_secs: u64,

    /// Number of failures before entering degraded mode
    #[serde(default = "default_failure_threshold")]
    pub failure_threshold: u32,

    /// Number of successes to exit degraded mode
    #[serde(default = "default_success_threshold")]
    pub success_threshold: u32,
}

fn default_unavailable_message() -> String {
    "I'm currently experiencing technical difficulties. Please try again in a moment.".to_string()
}

const fn default_retry_cooldown() -> u64 {
    30
}

const fn default_failure_threshold() -> u32 {
    3
}

const fn default_success_threshold() -> u32 {
    2
}

impl Default for DegradedModeAppConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            unavailable_message: default_unavailable_message(),
            retry_cooldown_secs: default_retry_cooldown(),
            failure_threshold: default_failure_threshold(),
            success_threshold: default_success_threshold(),
        }
    }
}
