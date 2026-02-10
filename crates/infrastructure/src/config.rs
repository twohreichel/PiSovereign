//! Application configuration

use ai_core::InferenceConfig;
use ai_speech::SpeechConfig;
use domain::MessengerSource;
use secrecy::{ExposeSecret, SecretString};
use serde::{Deserialize, Serialize};
use std::fmt;
use std::net::IpAddr;

/// Application environment (development or production)
///
/// Controls security validation strictness and default behaviors.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Environment {
    /// Development environment - relaxed security warnings
    #[default]
    Development,
    /// Production environment - strict security validation
    Production,
}

impl fmt::Display for Environment {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Development => write!(f, "development"),
            Self::Production => write!(f, "production"),
        }
    }
}

impl std::str::FromStr for Environment {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "development" | "dev" => Ok(Self::Development),
            "production" | "prod" => Ok(Self::Production),
            _ => Err(format!(
                "Invalid environment: {s}. Use 'development' or 'production'"
            )),
        }
    }
}

/// Messenger platform selection
///
/// Determines which messaging platform is active for receiving and sending messages.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MessengerSelection {
    /// Use WhatsApp Business API (default)
    #[default]
    WhatsApp,
    /// Use Signal via signal-cli daemon
    Signal,
    /// Disable messenger integration
    None,
}

impl MessengerSelection {
    /// Check if a messenger is enabled
    #[must_use]
    pub const fn is_enabled(&self) -> bool {
        !matches!(self, Self::None)
    }

    /// Convert to `MessengerSource` if enabled
    #[must_use]
    pub const fn to_source(&self) -> Option<MessengerSource> {
        match self {
            Self::WhatsApp => Some(MessengerSource::WhatsApp),
            Self::Signal => Some(MessengerSource::Signal),
            Self::None => None,
        }
    }
}

impl fmt::Display for MessengerSelection {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::WhatsApp => write!(f, "whatsapp"),
            Self::Signal => write!(f, "signal"),
            Self::None => write!(f, "none"),
        }
    }
}

/// Main application configuration
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AppConfig {
    /// Application environment (development or production)
    ///
    /// Controls security validation strictness. In production, critical
    /// security warnings will prevent startup unless PISOVEREIGN_ALLOW_INSECURE_CONFIG=true.
    #[serde(default)]
    pub environment: Option<Environment>,

    /// Active messenger platform (whatsapp or signal)
    #[serde(default)]
    pub messenger: MessengerSelection,

    /// Server configuration
    #[serde(default)]
    pub server: ServerConfig,

    /// Inference configuration
    #[serde(default)]
    pub inference: InferenceConfig,

    /// Security configuration
    #[serde(default)]
    pub security: SecurityConfig,

    /// Prompt security configuration (AI input validation)
    #[serde(default)]
    pub prompt_security: PromptSecurityConfig,

    /// WhatsApp configuration
    #[serde(default)]
    pub whatsapp: WhatsAppConfig,

    /// Signal configuration
    #[serde(default)]
    pub signal: SignalConfig,

    /// Database configuration
    #[serde(default)]
    pub database: DatabaseConfig,

    /// Cache configuration
    #[serde(default)]
    pub cache: CacheConfig,

    /// Weather configuration (optional)
    #[serde(default)]
    pub weather: Option<WeatherConfig>,

    /// Web search configuration (optional)
    #[serde(default)]
    pub websearch: Option<WebSearchAppConfig>,

    /// CalDAV calendar configuration (optional)
    #[serde(default)]
    pub caldav: Option<CalDavAppConfig>,

    /// Proton Mail configuration (optional)
    #[serde(default)]
    pub proton: Option<ProtonAppConfig>,

    /// Retry configuration for external service calls
    #[serde(default)]
    pub retry: Option<RetryAppConfig>,

    /// Health check configuration (optional)
    #[serde(default)]
    pub health: Option<HealthAppConfig>,

    /// Telemetry configuration (optional)
    #[serde(default)]
    pub telemetry: Option<TelemetryAppConfig>,

    /// Degraded mode configuration (optional)
    #[serde(default)]
    pub degraded_mode: Option<DegradedModeAppConfig>,

    /// Speech processing configuration (optional, for voice messages)
    #[serde(default)]
    pub speech: Option<SpeechConfig>,

    /// Memory/knowledge storage configuration (optional)
    #[serde(default)]
    pub memory: Option<MemoryAppConfig>,
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

const fn default_true() -> bool {
    true
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

/// Configuration for a hashed API key with associated user ID
///
/// API keys must be pre-hashed using Argon2id format (PHC string).
/// Use `pisovereign-cli migrate-keys` to convert plaintext keys.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiKeyEntry {
    /// Argon2id hash of the API key in PHC format
    /// Example: "$argon2id$v=19$m=19456,t=2,p=1$..."
    pub hash: String,

    /// User ID associated with this API key
    pub user_id: String,
}

/// Security configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityConfig {
    /// Whitelisted phone numbers for WhatsApp
    #[serde(default)]
    pub whitelisted_phones: Vec<String>,

    /// Hashed API keys for authentication (recommended)
    ///
    /// Each entry contains an Argon2id hash and associated user ID.
    /// Use `pisovereign-cli migrate-keys` to generate hashes.
    ///
    /// Example in config.toml:
    /// ```toml
    /// [[security.api_keys]]
    /// hash = "$argon2id$v=19$m=19456,t=2,p=1$..."
    /// user_id = "550e8400-e29b-41d4-a716-446655440000"
    /// ```
    #[serde(default)]
    pub api_keys: Vec<ApiKeyEntry>,

    /// Trusted proxy IP addresses for X-Forwarded-For header validation
    ///
    /// Only IPs in this list are trusted to set X-Forwarded-For headers.
    /// If empty, the direct connection IP is always used (secure default).
    #[serde(default)]
    pub trusted_proxies: Vec<IpAddr>,

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
            api_keys: Vec::new(),
            trusted_proxies: Vec::new(),
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

impl SecurityConfig {
    /// Validate that all API keys are properly hashed (not plaintext)
    ///
    /// Returns the number of invalid (plaintext) keys found.
    /// In release builds, this should cause startup to fail.
    #[must_use]
    pub fn count_plaintext_keys(&self) -> usize {
        self.api_keys
            .iter()
            .filter(|entry| !entry.hash.starts_with("$argon2"))
            .count()
    }

    /// Check if the configuration has any API keys configured
    #[must_use]
    pub fn has_api_keys(&self) -> bool {
        !self.api_keys.is_empty()
    }
}

/// Prompt security configuration for AI input validation
///
/// Controls detection and blocking of prompt injection attacks.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromptSecurityConfig {
    /// Whether prompt security analysis is enabled
    #[serde(default = "default_true")]
    pub enabled: bool,

    /// Sensitivity level: "low", "medium", or "high"
    #[serde(default = "default_sensitivity")]
    pub sensitivity: String,

    /// Whether to block requests when threats are detected
    #[serde(default = "default_true")]
    pub block_on_detection: bool,

    /// Maximum violations before automatically blocking an IP
    #[serde(default = "default_max_violations")]
    pub max_violations_before_block: u32,

    /// Time window for counting violations (in seconds)
    #[serde(default = "default_violation_window")]
    pub violation_window_secs: u64,

    /// How long to block an IP after exceeding threshold (in seconds)
    #[serde(default = "default_block_duration")]
    pub block_duration_secs: u64,

    /// Whether to auto-block on critical threats (without waiting for threshold)
    #[serde(default = "default_true")]
    pub auto_block_on_critical: bool,

    /// Custom patterns to detect (in addition to built-in patterns)
    #[serde(default)]
    pub custom_patterns: Vec<String>,
}

fn default_sensitivity() -> String {
    "medium".to_string()
}

const fn default_max_violations() -> u32 {
    3
}

const fn default_violation_window() -> u64 {
    3600 // 1 hour
}

const fn default_block_duration() -> u64 {
    86400 // 24 hours
}

impl Default for PromptSecurityConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            sensitivity: default_sensitivity(),
            block_on_detection: true,
            max_violations_before_block: default_max_violations(),
            violation_window_secs: default_violation_window(),
            block_duration_secs: default_block_duration(),
            auto_block_on_critical: true,
            custom_patterns: Vec::new(),
        }
    }
}

impl PromptSecurityConfig {
    /// Parse the sensitivity string into the application layer enum
    #[must_use]
    pub fn sensitivity_level(&self) -> application::services::SecuritySensitivity {
        match self.sensitivity.to_lowercase().as_str() {
            "low" => application::services::SecuritySensitivity::Low,
            "high" => application::services::SecuritySensitivity::High,
            _ => application::services::SecuritySensitivity::Medium,
        }
    }

    /// Convert to the application layer config
    #[must_use]
    pub fn to_prompt_security_config(&self) -> application::services::PromptSecurityConfig {
        application::services::PromptSecurityConfig {
            enabled: self.enabled,
            sensitivity: self.sensitivity_level(),
            block_on_detection: self.block_on_detection,
            min_confidence: self.sensitivity_level().confidence_threshold(),
        }
    }

    /// Convert to the suspicious activity config
    #[must_use]
    pub const fn to_suspicious_activity_config(
        &self,
    ) -> application::ports::SuspiciousActivityConfig {
        application::ports::SuspiciousActivityConfig {
            max_violations_before_block: self.max_violations_before_block,
            violation_window_secs: self.violation_window_secs,
            block_duration_secs: self.block_duration_secs,
            auto_block_on_critical: self.auto_block_on_critical,
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
#[derive(Clone, Serialize, Deserialize)]
pub struct WhatsAppConfig {
    /// Meta Graph API access token (sensitive - uses SecretString)
    #[serde(default, skip_serializing)]
    pub access_token: Option<SecretString>,

    /// Phone number ID from WhatsApp Business
    #[serde(default)]
    pub phone_number_id: Option<String>,

    /// App secret for webhook signature verification (sensitive - uses SecretString)
    #[serde(default, skip_serializing)]
    pub app_secret: Option<SecretString>,

    /// Verify token for webhook setup
    #[serde(default)]
    pub verify_token: Option<String>,

    /// Whether signature verification is required (default: true)
    #[serde(default = "default_true")]
    pub signature_required: bool,

    /// API version (default: v18.0)
    #[serde(default = "default_api_version")]
    pub api_version: String,

    /// Phone numbers allowed to send messages (empty = allow all)
    #[serde(default)]
    pub whitelist: Vec<String>,
}

impl std::fmt::Debug for WhatsAppConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WhatsAppConfig")
            .field(
                "access_token",
                &self.access_token.as_ref().map(|_| "[REDACTED]"),
            )
            .field("phone_number_id", &self.phone_number_id)
            .field(
                "app_secret",
                &self.app_secret.as_ref().map(|_| "[REDACTED]"),
            )
            .field("verify_token", &self.verify_token)
            .field("signature_required", &self.signature_required)
            .field("api_version", &self.api_version)
            .field("whitelist", &format!("[{} entries]", self.whitelist.len()))
            .finish()
    }
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
            whitelist: Vec::new(),
        }
    }
}

impl WhatsAppConfig {
    /// Get the access token as a string reference (for API calls)
    #[must_use]
    pub fn access_token_str(&self) -> Option<&str> {
        self.access_token.as_ref().map(ExposeSecret::expose_secret)
    }

    /// Get the app secret as a string reference (for signature verification)
    #[must_use]
    pub fn app_secret_str(&self) -> Option<&str> {
        self.app_secret.as_ref().map(ExposeSecret::expose_secret)
    }
}

/// Signal integration configuration
#[derive(Clone, Serialize, Deserialize)]
pub struct SignalConfig {
    /// Phone number registered with Signal (E.164 format, e.g., "+1234567890")
    #[serde(default)]
    pub phone_number: String,

    /// Path to the signal-cli JSON-RPC socket
    #[serde(default = "default_signal_socket_path")]
    pub socket_path: String,

    /// Path to signal-cli data directory (optional)
    #[serde(default)]
    pub data_path: Option<String>,

    /// Connection timeout in milliseconds
    #[serde(default = "default_signal_timeout")]
    pub timeout_ms: u64,

    /// Phone numbers allowed to send messages (empty = allow all)
    #[serde(default)]
    pub whitelist: Vec<String>,
}

fn default_signal_socket_path() -> String {
    "/var/run/signal-cli/socket".to_string()
}

const fn default_signal_timeout() -> u64 {
    30_000
}

impl std::fmt::Debug for SignalConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SignalConfig")
            .field("phone_number", &self.phone_number)
            .field("socket_path", &self.socket_path)
            .field("data_path", &self.data_path)
            .field("timeout_ms", &self.timeout_ms)
            .field("whitelist", &format!("[{} entries]", self.whitelist.len()))
            .finish()
    }
}

impl Default for SignalConfig {
    fn default() -> Self {
        Self {
            phone_number: String::new(),
            socket_path: default_signal_socket_path(),
            data_path: None,
            timeout_ms: default_signal_timeout(),
            whitelist: Vec::new(),
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

    // Environment tests
    #[test]
    fn environment_default_is_development() {
        let env = Environment::default();
        assert_eq!(env, Environment::Development);
    }

    #[test]
    fn environment_display() {
        assert_eq!(format!("{}", Environment::Development), "development");
        assert_eq!(format!("{}", Environment::Production), "production");
    }

    #[test]
    fn environment_from_str() {
        assert_eq!(
            "development".parse::<Environment>().unwrap(),
            Environment::Development
        );
        assert_eq!(
            "production".parse::<Environment>().unwrap(),
            Environment::Production
        );
        assert_eq!(
            "dev".parse::<Environment>().unwrap(),
            Environment::Development
        );
        assert_eq!(
            "prod".parse::<Environment>().unwrap(),
            Environment::Production
        );
    }

    #[test]
    fn environment_from_str_case_insensitive() {
        assert_eq!(
            "DEVELOPMENT".parse::<Environment>().unwrap(),
            Environment::Development
        );
        assert_eq!(
            "PRODUCTION".parse::<Environment>().unwrap(),
            Environment::Production
        );
    }

    // Messenger selection tests
    #[test]
    fn messenger_selection_default_is_whatsapp() {
        let selection = MessengerSelection::default();
        assert_eq!(selection, MessengerSelection::WhatsApp);
    }

    #[test]
    fn messenger_selection_display() {
        assert_eq!(format!("{}", MessengerSelection::WhatsApp), "whatsapp");
        assert_eq!(format!("{}", MessengerSelection::Signal), "signal");
        assert_eq!(format!("{}", MessengerSelection::None), "none");
    }

    #[test]
    fn messenger_selection_is_enabled() {
        assert!(MessengerSelection::WhatsApp.is_enabled());
        assert!(MessengerSelection::Signal.is_enabled());
        assert!(!MessengerSelection::None.is_enabled());
    }

    #[test]
    fn messenger_selection_to_source() {
        assert_eq!(
            MessengerSelection::WhatsApp.to_source(),
            Some(MessengerSource::WhatsApp)
        );
        assert_eq!(
            MessengerSelection::Signal.to_source(),
            Some(MessengerSource::Signal)
        );
        assert_eq!(MessengerSelection::None.to_source(), None);
    }

    #[test]
    fn messenger_selection_serialize() {
        assert_eq!(
            serde_json::to_string(&MessengerSelection::WhatsApp).unwrap(),
            "\"whatsapp\""
        );
        assert_eq!(
            serde_json::to_string(&MessengerSelection::Signal).unwrap(),
            "\"signal\""
        );
        assert_eq!(
            serde_json::to_string(&MessengerSelection::None).unwrap(),
            "\"none\""
        );
    }

    #[test]
    fn messenger_selection_deserialize() {
        assert_eq!(
            serde_json::from_str::<MessengerSelection>("\"whatsapp\"").unwrap(),
            MessengerSelection::WhatsApp
        );
        assert_eq!(
            serde_json::from_str::<MessengerSelection>("\"signal\"").unwrap(),
            MessengerSelection::Signal
        );
        assert_eq!(
            serde_json::from_str::<MessengerSelection>("\"none\"").unwrap(),
            MessengerSelection::None
        );
    }

    // Signal config tests
    #[test]
    fn signal_config_default() {
        let config = SignalConfig::default();
        assert!(config.phone_number.is_empty());
        assert_eq!(config.socket_path, "/var/run/signal-cli/socket");
        assert!(config.data_path.is_none());
        assert_eq!(config.timeout_ms, 30_000);
        assert!(config.whitelist.is_empty());
    }

    #[test]
    fn environment_from_str_invalid() {
        let result = "invalid".parse::<Environment>();
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Invalid environment"));
    }

    #[test]
    fn environment_serialize() {
        let env = Environment::Production;
        let json = serde_json::to_string(&env).unwrap();
        assert_eq!(json, "\"production\"");
    }

    #[test]
    fn environment_deserialize() {
        let env: Environment = serde_json::from_str("\"production\"").unwrap();
        assert_eq!(env, Environment::Production);

        let env: Environment = serde_json::from_str("\"development\"").unwrap();
        assert_eq!(env, Environment::Development);
    }

    #[test]
    fn app_config_with_environment() {
        let json = r#"{"environment":"production"}"#;
        let config: AppConfig = serde_json::from_str(json).unwrap();
        assert_eq!(config.environment, Some(Environment::Production));
    }

    #[test]
    fn app_config_default() {
        let config = AppConfig::default();
        assert_eq!(config.server.port, 3000);
        assert_eq!(config.server.host, "127.0.0.1");
        assert!(config.server.cors_enabled);
    }

    #[test]
    fn server_config_default() {
        let config = ServerConfig::default();
        assert_eq!(config.host, "127.0.0.1");
        assert_eq!(config.port, 3000);
        assert!(config.cors_enabled);
    }

    #[test]
    fn security_config_default() {
        let config = SecurityConfig::default();
        assert!(config.whitelisted_phones.is_empty());
        assert!(config.api_keys.is_empty());
        assert!(config.trusted_proxies.is_empty());
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
        assert_eq!(config.server.host, "127.0.0.1");
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
    fn security_config_with_api_keys() {
        let json = r#"{"api_keys":[{"hash":"$argon2id$v=19$m=19456,t=2,p=1$test","user_id":"550e8400-e29b-41d4-a716-446655440000"}]}"#;
        let config: SecurityConfig = serde_json::from_str(json).unwrap();
        assert_eq!(config.api_keys.len(), 1);
        assert!(config.api_keys[0].hash.starts_with("$argon2"));
        assert_eq!(
            config.api_keys[0].user_id,
            "550e8400-e29b-41d4-a716-446655440000"
        );
    }

    #[test]
    fn security_config_count_plaintext_keys() {
        let mut config = SecurityConfig::default();
        config.api_keys.push(ApiKeyEntry {
            hash: "$argon2id$v=19$m=19456,t=2,p=1$valid".to_string(),
            user_id: "user1".to_string(),
        });
        config.api_keys.push(ApiKeyEntry {
            hash: "plaintext-key".to_string(),
            user_id: "user2".to_string(),
        });
        assert_eq!(config.count_plaintext_keys(), 1);
    }

    #[test]
    fn security_config_trusted_proxies() {
        let json = r#"{"trusted_proxies":["10.0.0.1","192.168.1.1"]}"#;
        let config: SecurityConfig = serde_json::from_str(json).unwrap();
        assert_eq!(config.trusted_proxies.len(), 2);
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

    #[test]
    fn security_config_api_keys_default_empty() {
        let config = SecurityConfig::default();
        assert!(config.api_keys.is_empty());
    }

    #[test]
    fn security_config_api_keys_deserialize_multiple() {
        let json = r#"{"api_keys":[{"hash":"$argon2id$v=19$m=19456,t=2,p=1$abc","user_id":"550e8400-e29b-41d4-a716-446655440000"},{"hash":"$argon2id$v=19$m=19456,t=2,p=1$xyz","user_id":"6ba7b810-9dad-11d1-80b4-00c04fd430c8"}]}"#;
        let config: SecurityConfig = serde_json::from_str(json).unwrap();
        assert_eq!(config.api_keys.len(), 2);
        assert_eq!(
            config.api_keys[0].user_id,
            "550e8400-e29b-41d4-a716-446655440000"
        );
        assert_eq!(
            config.api_keys[1].user_id,
            "6ba7b810-9dad-11d1-80b4-00c04fd430c8"
        );
    }

    #[test]
    fn security_config_api_keys_serialize() {
        let mut config = SecurityConfig::default();
        config.api_keys.push(ApiKeyEntry {
            hash: "$argon2id$v=19$m=19456,t=2,p=1$test".to_string(),
            user_id: "550e8400-e29b-41d4-a716-446655440000".to_string(),
        });
        let json = serde_json::to_string(&config).unwrap();
        assert!(json.contains("api_keys"));
        assert!(json.contains("$argon2id"));
    }

    #[test]
    fn security_config_has_api_keys() {
        let mut config = SecurityConfig::default();
        assert!(!config.has_api_keys());

        config.api_keys.push(ApiKeyEntry {
            hash: "$argon2id$test".to_string(),
            user_id: "user".to_string(),
        });
        assert!(config.has_api_keys());
    }

    #[test]
    fn websearch_config_default() {
        let config = WebSearchAppConfig::default();
        assert!(config.api_key.is_none());
        assert_eq!(config.max_results, 5);
        assert_eq!(config.timeout_secs, 30);
        assert!(config.fallback_enabled);
        assert_eq!(config.safe_search, "moderate");
        assert!(config.country.is_none());
        assert!(config.language.is_none());
        assert!(config.rate_limit_rpm.is_none());
        assert_eq!(config.cache_ttl_minutes, 30);
    }

    #[test]
    fn websearch_config_deserialize() {
        let json = r#"{"api_key":"BSA-123456","max_results":10,"language":"de","country":"DE"}"#;
        let config: WebSearchAppConfig = serde_json::from_str(json).unwrap();
        assert_eq!(config.api_key, Some("BSA-123456".to_string()));
        assert_eq!(config.max_results, 10);
        assert_eq!(config.language, Some("de".to_string()));
        assert_eq!(config.country, Some("DE".to_string()));
    }

    #[test]
    fn websearch_config_to_integration_config() {
        let config = WebSearchAppConfig {
            api_key: Some("test-key".to_string()),
            max_results: 8,
            timeout_secs: 60,
            fallback_enabled: false,
            safe_search: "strict".to_string(),
            country: Some("US".to_string()),
            language: Some("en".to_string()),
            rate_limit_rpm: Some(30),
            cache_ttl_minutes: 15,
        };

        let integration_config = config.to_websearch_config();
        assert_eq!(
            integration_config.brave_api_key,
            Some("test-key".to_string())
        );
        assert_eq!(integration_config.max_results, 8);
        assert_eq!(integration_config.timeout_secs, 60);
        assert!(!integration_config.fallback_enabled);
        assert_eq!(integration_config.safe_search, "strict");
        assert_eq!(integration_config.result_country, "US");
        assert_eq!(integration_config.result_language, "en");
        assert_eq!(integration_config.cache_ttl_minutes, 15);
    }

    #[test]
    fn app_config_with_websearch() {
        let json = r#"{"websearch":{"api_key":"BSA-test","max_results":3}}"#;
        let config: AppConfig = serde_json::from_str(json).unwrap();
        assert!(config.websearch.is_some());
        let ws = config.websearch.unwrap();
        assert_eq!(ws.api_key, Some("BSA-test".to_string()));
        assert_eq!(ws.max_results, 3);
    }

    // Additional tests for improved coverage

    #[test]
    fn telemetry_config_default() {
        let config = TelemetryAppConfig::default();
        assert!(!config.enabled);
        assert_eq!(config.otlp_endpoint, "http://localhost:4317");
        assert_eq!(config.sample_ratio, Some(1.0));
    }

    #[test]
    fn retry_config_default() {
        let config = RetryAppConfig::default();
        assert_eq!(config.initial_delay_ms, 100);
        assert_eq!(config.max_delay_ms, 10_000);
        assert!((config.multiplier - 2.0).abs() < f64::EPSILON);
        assert_eq!(config.max_retries, 3);
    }

    #[test]
    fn retry_config_to_retry_config() {
        let config = RetryAppConfig {
            initial_delay_ms: 200,
            max_delay_ms: 5000,
            multiplier: 1.5,
            max_retries: 5,
        };
        let retry_config = config.to_retry_config();
        assert_eq!(retry_config.initial_delay_ms, 200);
        assert_eq!(retry_config.max_delay_ms, 5000);
        assert!((retry_config.multiplier - 1.5).abs() < f64::EPSILON);
        assert_eq!(retry_config.max_retries, 5);
    }

    #[test]
    fn memory_config_default() {
        let config = MemoryAppConfig::default();
        assert!(config.enabled);
        assert!(config.enable_rag);
        assert!(config.enable_learning);
        assert_eq!(config.rag_limit, 5);
        assert!((config.rag_threshold - 0.5).abs() < 0.001);
        assert!((config.merge_threshold - 0.85).abs() < 0.001);
        assert!((config.min_importance - 0.1).abs() < 0.001);
        assert!((config.decay_factor - 0.95).abs() < 0.001);
        assert!(config.enable_encryption);
    }

    #[test]
    fn memory_config_to_memory_service_config() {
        let config = MemoryAppConfig {
            rag_limit: 10,
            rag_threshold: 0.6,
            merge_threshold: 0.9,
            min_importance: 0.2,
            decay_factor: 0.9,
            enable_encryption: false,
            ..Default::default()
        };
        let service_config = config.to_memory_service_config();
        assert_eq!(service_config.rag_limit, 10);
        assert!((service_config.rag_threshold - 0.6).abs() < 0.001);
        assert!((service_config.merge_threshold - 0.9).abs() < 0.001);
        assert!(!service_config.enable_encryption);
    }

    #[test]
    fn memory_config_to_enhanced_chat_config() {
        let config = MemoryAppConfig {
            enable_rag: true,
            enable_learning: false,
            ..Default::default()
        };
        let chat_config = config.to_enhanced_chat_config(Some("Test prompt".to_string()));
        assert!(chat_config.enable_rag);
        assert!(!chat_config.enable_learning);
        assert_eq!(chat_config.system_prompt, Some("Test prompt".to_string()));
        assert_eq!(chat_config.min_learning_length, 20);
        assert!((chat_config.default_importance - 0.5).abs() < 0.001);
    }

    #[test]
    fn memory_config_to_embedding_config() {
        let config = MemoryAppConfig {
            embedding: EmbeddingAppConfig {
                model: "test-model".to_string(),
                dimension: 768,
                timeout_ms: 60000,
            },
            ..Default::default()
        };
        let embedding_config = config.to_embedding_config("http://localhost:11434");
        assert_eq!(embedding_config.base_url, "http://localhost:11434");
        assert_eq!(embedding_config.model, "test-model");
        assert_eq!(embedding_config.dimensions, 768);
        assert_eq!(embedding_config.timeout_ms, 60000);
    }

    #[test]
    fn embedding_config_default() {
        let config = EmbeddingAppConfig::default();
        assert_eq!(config.model, "nomic-embed-text");
        assert_eq!(config.dimension, 384);
        assert_eq!(config.timeout_ms, 30000);
    }

    #[test]
    fn geo_location_config_to_geo_location_valid() {
        let config = GeoLocationConfig {
            latitude: 52.52,
            longitude: 13.405,
        };
        let geo = config.to_geo_location();
        assert!(geo.is_some());
        let g = geo.unwrap();
        assert!((g.latitude() - 52.52).abs() < 0.001);
        assert!((g.longitude() - 13.405).abs() < 0.001);
    }

    #[test]
    fn geo_location_config_to_geo_location_invalid() {
        let config = GeoLocationConfig {
            latitude: 200.0, // Invalid
            longitude: 13.405,
        };
        let geo = config.to_geo_location();
        assert!(geo.is_none());
    }

    #[test]
    fn weather_config_default() {
        let config = WeatherConfig::default();
        assert_eq!(config.base_url, "https://api.open-meteo.com/v1");
        assert_eq!(config.timeout_secs, 30);
        assert_eq!(config.forecast_days, 7);
        assert_eq!(config.cache_ttl_minutes, 30);
        assert!(config.default_location.is_none());
    }

    #[test]
    fn health_config_default() {
        let config = HealthAppConfig::default();
        assert_eq!(config.global_timeout_secs, 5);
        assert!(config.inference_timeout_secs.is_none());
    }

    #[test]
    fn health_config_to_health_config() {
        let config = HealthAppConfig {
            global_timeout_secs: 30,
            inference_timeout_secs: Some(5),
            email_timeout_secs: Some(10),
            calendar_timeout_secs: Some(8),
            weather_timeout_secs: Some(15),
        };
        let health_config = config.to_health_config();
        assert_eq!(health_config.global_timeout_secs, 30);
        assert_eq!(health_config.service_timeouts.len(), 4);
        assert_eq!(health_config.service_timeouts.get("inference"), Some(&5));
        assert_eq!(health_config.service_timeouts.get("email"), Some(&10));
    }

    #[test]
    fn degraded_mode_config_default() {
        let config = DegradedModeAppConfig::default();
        assert!(config.enabled);
        assert_eq!(config.failure_threshold, 3);
        assert_eq!(config.retry_cooldown_secs, 30);
        assert_eq!(config.success_threshold, 2);
    }

    // Additional tests for default configurations

    #[test]
    fn whatsapp_config_default_test() {
        let config = WhatsAppConfig::default();
        assert!(config.access_token.is_none());
        assert!(config.phone_number_id.is_none());
        assert!(config.app_secret.is_none());
        assert!(config.verify_token.is_none());
        assert!(config.signature_required);
    }

    #[test]
    fn prompt_security_config_default() {
        let config = PromptSecurityConfig::default();
        assert!(config.enabled);
        assert_eq!(config.sensitivity, "medium");
        assert!(config.block_on_detection);
        assert_eq!(config.max_violations_before_block, 3);
    }

    #[test]
    fn prompt_security_config_sensitivity_level() {
        let mut config = PromptSecurityConfig::default();

        config.sensitivity = "low".to_string();
        assert_eq!(
            config.sensitivity_level(),
            application::services::SecuritySensitivity::Low
        );

        config.sensitivity = "medium".to_string();
        assert_eq!(
            config.sensitivity_level(),
            application::services::SecuritySensitivity::Medium
        );

        config.sensitivity = "high".to_string();
        assert_eq!(
            config.sensitivity_level(),
            application::services::SecuritySensitivity::High
        );

        // Unknown defaults to medium
        config.sensitivity = "unknown".to_string();
        assert_eq!(
            config.sensitivity_level(),
            application::services::SecuritySensitivity::Medium
        );
    }

    #[test]
    fn prompt_security_config_to_prompt_security_config() {
        let config = PromptSecurityConfig {
            enabled: true,
            sensitivity: "high".to_string(),
            block_on_detection: false,
            ..Default::default()
        };
        let converted = config.to_prompt_security_config();
        assert!(converted.enabled);
        assert!(!converted.block_on_detection);
        assert_eq!(
            converted.sensitivity,
            application::services::SecuritySensitivity::High
        );
    }

    #[test]
    fn prompt_security_config_to_suspicious_activity_config() {
        let config = PromptSecurityConfig {
            max_violations_before_block: 5,
            violation_window_secs: 7200,
            block_duration_secs: 43200,
            auto_block_on_critical: false,
            ..Default::default()
        };
        let converted = config.to_suspicious_activity_config();
        assert_eq!(converted.max_violations_before_block, 5);
        assert_eq!(converted.violation_window_secs, 7200);
        assert_eq!(converted.block_duration_secs, 43200);
        assert!(!converted.auto_block_on_critical);
    }

    #[test]
    fn security_config_additional_fields() {
        let config = SecurityConfig::default();
        assert_eq!(config.rate_limit_cleanup_interval_secs, 300);
        assert_eq!(config.rate_limit_cleanup_max_age_secs, 600);
        assert!(config.tls_verify_certs);
        assert_eq!(config.connection_timeout_secs, 30);
        assert_eq!(config.min_tls_version, "1.2");
    }

    #[test]
    fn database_config_default() {
        let config = DatabaseConfig::default();
        assert_eq!(config.path, "pisovereign.db");
        assert_eq!(config.max_connections, 5);
        assert!(config.run_migrations);
    }

    #[test]
    fn database_config_serialization() {
        let config = DatabaseConfig {
            path: "custom.db".to_string(),
            max_connections: 10,
            run_migrations: false,
        };
        let json = serde_json::to_string(&config).unwrap();
        let parsed: DatabaseConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.path, "custom.db");
        assert_eq!(parsed.max_connections, 10);
        assert!(!parsed.run_migrations);
    }

    #[test]
    fn signal_config_serialization() {
        let config = SignalConfig {
            phone_number: "+1234567890".to_string(),
            socket_path: "/custom/socket".to_string(),
            data_path: Some("/data".to_string()),
            timeout_ms: 60000,
            whitelist: vec!["+11111111111".to_string()],
        };
        let json = serde_json::to_string(&config).unwrap();
        let parsed: SignalConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.phone_number, "+1234567890");
        assert_eq!(parsed.socket_path, "/custom/socket");
        assert_eq!(parsed.data_path, Some("/data".to_string()));
        assert_eq!(parsed.timeout_ms, 60000);
        assert_eq!(parsed.whitelist.len(), 1);
    }

    #[test]
    fn signal_config_debug() {
        let config = SignalConfig::default();
        let debug = format!("{config:?}");
        assert!(debug.contains("SignalConfig"));
        assert!(debug.contains("phone_number"));
        assert!(debug.contains("socket_path"));
    }
}

/// Weather service configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WeatherConfig {
    /// Open-Meteo API base URL
    #[serde(default = "default_weather_base_url")]
    pub base_url: String,

    /// Connection timeout in seconds
    #[serde(default = "default_weather_timeout")]
    pub timeout_secs: u64,

    /// Number of forecast days (1-16)
    #[serde(default = "default_forecast_days")]
    pub forecast_days: u8,

    /// Cache TTL in minutes
    #[serde(default = "default_cache_ttl_minutes")]
    pub cache_ttl_minutes: u32,

    /// Default location for weather when user profile has no location
    ///
    /// Configured as inline table: `{ latitude = 52.52, longitude = 13.405 }`
    #[serde(default)]
    pub default_location: Option<GeoLocationConfig>,
}

/// Geographic location configuration (latitude/longitude pair)
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct GeoLocationConfig {
    /// Latitude (-90.0 to 90.0)
    pub latitude: f64,
    /// Longitude (-180.0 to 180.0)
    pub longitude: f64,
}

impl GeoLocationConfig {
    /// Convert to domain GeoLocation value object
    ///
    /// Returns `None` if coordinates are invalid.
    #[must_use]
    pub fn to_geo_location(&self) -> Option<domain::GeoLocation> {
        domain::GeoLocation::new(self.latitude, self.longitude).ok()
    }
}

fn default_weather_base_url() -> String {
    "https://api.open-meteo.com/v1".to_string()
}

const fn default_weather_timeout() -> u64 {
    30
}

const fn default_forecast_days() -> u8 {
    7
}

const fn default_cache_ttl_minutes() -> u32 {
    30
}

impl Default for WeatherConfig {
    fn default() -> Self {
        Self {
            base_url: default_weather_base_url(),
            timeout_secs: default_weather_timeout(),
            forecast_days: default_forecast_days(),
            cache_ttl_minutes: default_cache_ttl_minutes(),
            default_location: None,
        }
    }
}

/// Web search service configuration
///
/// Configures web search integration using Brave Search (primary) and DuckDuckGo (fallback).
/// Get your Brave API key at: <https://brave.com/search/api/>
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebSearchAppConfig {
    /// Brave Search API key (required for Brave, optional if using DuckDuckGo only)
    ///
    /// Obtain from <https://brave.com/search/api/>
    #[serde(default)]
    pub api_key: Option<String>,

    /// Maximum number of search results to return (1-10)
    #[serde(default = "default_websearch_max_results")]
    pub max_results: u32,

    /// Connection timeout in seconds
    #[serde(default = "default_websearch_timeout")]
    pub timeout_secs: u64,

    /// Enable DuckDuckGo fallback when Brave fails or returns no results
    #[serde(default = "default_true")]
    pub fallback_enabled: bool,

    /// Safe search level: "off", "moderate", or "strict"
    #[serde(default = "default_safe_search")]
    pub safe_search: String,

    /// Country code for search results (e.g., "DE", "US", "GB")
    #[serde(default)]
    pub country: Option<String>,

    /// Language code for search results (e.g., "de", "en", "fr")
    #[serde(default)]
    pub language: Option<String>,

    /// Rate limit: maximum requests per minute (0 = unlimited)
    #[serde(default)]
    pub rate_limit_rpm: Option<u32>,

    /// Cache TTL in minutes for search results
    #[serde(default = "default_websearch_cache_ttl")]
    pub cache_ttl_minutes: u32,
}

const fn default_websearch_max_results() -> u32 {
    5
}

const fn default_websearch_timeout() -> u64 {
    30
}

fn default_safe_search() -> String {
    "moderate".to_string()
}

const fn default_websearch_cache_ttl() -> u32 {
    30
}

impl Default for WebSearchAppConfig {
    fn default() -> Self {
        Self {
            api_key: None,
            max_results: default_websearch_max_results(),
            timeout_secs: default_websearch_timeout(),
            fallback_enabled: true,
            safe_search: default_safe_search(),
            country: None,
            language: None,
            rate_limit_rpm: None,
            cache_ttl_minutes: default_websearch_cache_ttl(),
        }
    }
}

impl WebSearchAppConfig {
    /// Convert to integration_websearch config
    #[must_use]
    pub fn to_websearch_config(&self) -> integration_websearch::WebSearchConfig {
        let mut config = integration_websearch::WebSearchConfig::default();
        config.brave_api_key.clone_from(&self.api_key);
        config.max_results = self.max_results as usize;
        config.timeout_secs = self.timeout_secs;
        config.fallback_enabled = self.fallback_enabled;
        config.safe_search.clone_from(&self.safe_search);
        config.cache_ttl_minutes = self.cache_ttl_minutes;
        if let Some(ref country) = self.country {
            config.result_country.clone_from(country);
        }
        if let Some(ref language) = self.language {
            config.result_language.clone_from(language);
        }
        if let Some(rpm) = self.rate_limit_rpm {
            // Convert RPM to daily rate (approximate)
            config.rate_limit_daily = rpm * 60 * 24;
        }
        config
    }
}

/// CalDAV calendar server configuration
#[derive(Clone, Serialize, Deserialize)]
pub struct CalDavAppConfig {
    /// CalDAV server URL (e.g., <https://cal.example.com>)
    pub server_url: String,

    /// Username for authentication
    pub username: String,

    /// Password for authentication (sensitive - uses SecretString)
    #[serde(skip_serializing)]
    pub password: SecretString,

    /// Default calendar path (optional)
    #[serde(default)]
    pub calendar_path: Option<String>,

    /// Verify TLS certificates (default: true)
    #[serde(default = "default_true")]
    pub verify_certs: bool,

    /// Connection timeout in seconds (default: 30)
    #[serde(default = "default_caldav_timeout")]
    pub timeout_secs: u64,
}

impl std::fmt::Debug for CalDavAppConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CalDavAppConfig")
            .field("server_url", &self.server_url)
            .field("username", &self.username)
            .field("password", &"[REDACTED]")
            .field("calendar_path", &self.calendar_path)
            .field("verify_certs", &self.verify_certs)
            .field("timeout_secs", &self.timeout_secs)
            .finish()
    }
}

const fn default_caldav_timeout() -> u64 {
    30
}

impl CalDavAppConfig {
    /// Convert to integration_caldav's CalDavConfig
    #[must_use]
    pub fn to_caldav_config(&self) -> integration_caldav::CalDavConfig {
        integration_caldav::CalDavConfig {
            server_url: self.server_url.clone(),
            username: self.username.clone(),
            password: self.password.expose_secret().to_string(),
            calendar_path: self.calendar_path.clone(),
            verify_certs: self.verify_certs,
            timeout_secs: self.timeout_secs,
        }
    }

    /// Get the password as a string reference
    #[must_use]
    pub fn password_str(&self) -> &str {
        self.password.expose_secret()
    }
}

/// Proton Mail configuration (via Proton Bridge)
#[derive(Clone, Serialize, Deserialize)]
pub struct ProtonAppConfig {
    /// IMAP server host (default: 127.0.0.1)
    #[serde(default = "default_proton_host")]
    pub imap_host: String,

    /// IMAP server port (default: 1143 for STARTTLS)
    #[serde(default = "default_imap_port")]
    pub imap_port: u16,

    /// SMTP server host (default: 127.0.0.1)
    #[serde(default = "default_proton_host")]
    pub smtp_host: String,

    /// SMTP server port (default: 1025 for STARTTLS)
    #[serde(default = "default_smtp_port")]
    pub smtp_port: u16,

    /// Email address (Bridge account email)
    pub email: String,

    /// Bridge password (from Bridge UI, NOT Proton account password)
    /// Sensitive - uses SecretString for zeroization
    #[serde(skip_serializing)]
    pub password: SecretString,

    /// TLS configuration
    #[serde(default)]
    pub tls: ProtonTlsAppConfig,
}

impl std::fmt::Debug for ProtonAppConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ProtonAppConfig")
            .field("imap_host", &self.imap_host)
            .field("imap_port", &self.imap_port)
            .field("smtp_host", &self.smtp_host)
            .field("smtp_port", &self.smtp_port)
            .field("email", &self.email)
            .field("password", &"[REDACTED]")
            .field("tls", &self.tls)
            .finish()
    }
}

fn default_proton_host() -> String {
    "127.0.0.1".to_string()
}

const fn default_imap_port() -> u16 {
    1143
}

const fn default_smtp_port() -> u16 {
    1025
}

/// TLS configuration for Proton Bridge connections
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ProtonTlsAppConfig {
    /// Verify TLS certificates (default: true)
    #[serde(default = "default_true")]
    pub verify_certificates: bool,

    /// Minimum TLS version ("1.2" or "1.3")
    #[serde(default = "default_min_tls")]
    pub min_tls_version: String,

    /// Path to custom CA certificate (optional)
    #[serde(default)]
    pub ca_cert_path: Option<String>,
}

fn default_min_tls() -> String {
    "1.2".to_string()
}

impl ProtonAppConfig {
    /// Convert to integration_proton's ProtonConfig
    #[must_use]
    pub fn to_proton_config(&self) -> integration_proton::ProtonConfig {
        integration_proton::ProtonConfig {
            imap_host: self.imap_host.clone(),
            imap_port: self.imap_port,
            smtp_host: self.smtp_host.clone(),
            smtp_port: self.smtp_port,
            email: self.email.clone(),
            password: self.password.expose_secret().to_string(),
            tls: integration_proton::TlsConfig {
                verify_certificates: Some(self.tls.verify_certificates),
                min_tls_version: self.tls.min_tls_version.clone(),
                ca_cert_path: self.tls.ca_cert_path.as_ref().map(std::path::PathBuf::from),
            },
        }
    }

    /// Get the password as a string reference
    #[must_use]
    pub fn password_str(&self) -> &str {
        self.password.expose_secret()
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

/// Retry configuration for external service calls
///
/// Configures exponential backoff retry behavior for all external services.
/// Individual services can override these defaults.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetryAppConfig {
    /// Initial delay before first retry in milliseconds (default: 100ms)
    #[serde(default = "default_retry_initial_delay")]
    pub initial_delay_ms: u64,

    /// Maximum delay between retries in milliseconds (default: 10000ms = 10s)
    #[serde(default = "default_retry_max_delay")]
    pub max_delay_ms: u64,

    /// Multiplier for exponential backoff (default: 2.0)
    #[serde(default = "default_retry_multiplier")]
    pub multiplier: f64,

    /// Maximum number of retry attempts (default: 3)
    #[serde(default = "default_retry_max_retries")]
    pub max_retries: u32,
}

const fn default_retry_initial_delay() -> u64 {
    100
}

const fn default_retry_max_delay() -> u64 {
    10_000
}

const fn default_retry_multiplier() -> f64 {
    2.0
}

const fn default_retry_max_retries() -> u32 {
    3
}

impl Default for RetryAppConfig {
    fn default() -> Self {
        Self {
            initial_delay_ms: default_retry_initial_delay(),
            max_delay_ms: default_retry_max_delay(),
            multiplier: default_retry_multiplier(),
            max_retries: default_retry_max_retries(),
        }
    }
}

impl RetryAppConfig {
    /// Convert to retry::RetryConfig for use with retry operations
    #[must_use]
    pub const fn to_retry_config(&self) -> crate::retry::RetryConfig {
        crate::retry::RetryConfig::new(
            self.initial_delay_ms,
            self.max_delay_ms,
            self.multiplier,
            self.max_retries,
        )
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

/// Health check configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthAppConfig {
    /// Global timeout for all health checks in seconds
    #[serde(default = "default_health_global_timeout")]
    pub global_timeout_secs: u64,

    /// Inference engine health check timeout in seconds (overrides global)
    pub inference_timeout_secs: Option<u64>,

    /// Email service health check timeout in seconds (overrides global)
    pub email_timeout_secs: Option<u64>,

    /// Calendar service health check timeout in seconds (overrides global)
    pub calendar_timeout_secs: Option<u64>,

    /// Weather service health check timeout in seconds (overrides global)
    pub weather_timeout_secs: Option<u64>,
}

const fn default_health_global_timeout() -> u64 {
    5
}

impl Default for HealthAppConfig {
    fn default() -> Self {
        Self {
            global_timeout_secs: default_health_global_timeout(),
            inference_timeout_secs: None,
            email_timeout_secs: None,
            calendar_timeout_secs: None,
            weather_timeout_secs: None,
        }
    }
}

impl HealthAppConfig {
    /// Convert to application::HealthConfig
    #[must_use]
    pub fn to_health_config(&self) -> application::HealthConfig {
        use std::collections::HashMap;

        let mut service_timeouts = HashMap::new();

        if let Some(t) = self.inference_timeout_secs {
            service_timeouts.insert("inference".to_string(), t);
        }
        if let Some(t) = self.email_timeout_secs {
            service_timeouts.insert("email".to_string(), t);
        }
        if let Some(t) = self.calendar_timeout_secs {
            service_timeouts.insert("calendar".to_string(), t);
        }
        if let Some(t) = self.weather_timeout_secs {
            service_timeouts.insert("weather".to_string(), t);
        }

        application::HealthConfig {
            global_timeout_secs: self.global_timeout_secs,
            service_timeouts,
        }
    }
}

// ==============================
// Memory/Knowledge Storage Configuration
// ==============================

/// Memory storage configuration for AI knowledge persistence
///
/// Enables storage of AI interactions, facts, and context for RAG-based
/// retrieval and self-improvement.
#[allow(clippy::struct_excessive_bools)] // Configuration needs multiple boolean flags
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryAppConfig {
    /// Enable memory storage (default: true)
    #[serde(default = "default_true")]
    pub enabled: bool,

    /// Enable RAG context retrieval (default: true)
    #[serde(default = "default_true")]
    pub enable_rag: bool,

    /// Enable automatic learning from interactions (default: true)
    #[serde(default = "default_true")]
    pub enable_learning: bool,

    /// Number of memories to retrieve for RAG context (default: 5)
    #[serde(default = "default_rag_limit")]
    pub rag_limit: usize,

    /// Minimum similarity threshold for RAG retrieval (0.0-1.0, default: 0.5)
    #[serde(default = "default_rag_threshold")]
    pub rag_threshold: f32,

    /// Similarity threshold for memory deduplication (0.0-1.0, default: 0.85)
    #[serde(default = "default_merge_threshold")]
    pub merge_threshold: f32,

    /// Minimum importance score to keep memories (default: 0.1)
    #[serde(default = "default_min_importance")]
    pub min_importance: f32,

    /// Decay factor for memory importance over time (default: 0.95)
    #[serde(default = "default_decay_factor")]
    pub decay_factor: f32,

    /// Enable content encryption (default: true)
    #[serde(default = "default_true")]
    pub enable_encryption: bool,

    /// Path to encryption key file (generated if not exists)
    #[serde(default = "default_encryption_key_path")]
    pub encryption_key_path: String,

    /// Embedding model configuration
    #[serde(default)]
    pub embedding: EmbeddingAppConfig,
}

impl Default for MemoryAppConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            enable_rag: true,
            enable_learning: true,
            rag_limit: default_rag_limit(),
            rag_threshold: default_rag_threshold(),
            merge_threshold: default_merge_threshold(),
            min_importance: default_min_importance(),
            decay_factor: default_decay_factor(),
            enable_encryption: true,
            encryption_key_path: default_encryption_key_path(),
            embedding: EmbeddingAppConfig::default(),
        }
    }
}

/// Embedding model configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbeddingAppConfig {
    /// Embedding model name (default: nomic-embed-text)
    #[serde(default = "default_embedding_model")]
    pub model: String,

    /// Embedding dimension (default: 384 for nomic-embed-text)
    #[serde(default = "default_embedding_dimension")]
    pub dimension: usize,

    /// Request timeout in milliseconds (default: 30000)
    #[serde(default = "default_embedding_timeout")]
    pub timeout_ms: u64,
}

impl Default for EmbeddingAppConfig {
    fn default() -> Self {
        Self {
            model: default_embedding_model(),
            dimension: default_embedding_dimension(),
            timeout_ms: default_embedding_timeout(),
        }
    }
}

impl MemoryAppConfig {
    /// Convert to MemoryServiceConfig
    #[must_use]
    pub const fn to_memory_service_config(&self) -> application::MemoryServiceConfig {
        application::MemoryServiceConfig {
            rag_limit: self.rag_limit,
            rag_threshold: self.rag_threshold,
            merge_threshold: self.merge_threshold,
            min_importance: self.min_importance,
            decay_factor: self.decay_factor,
            enable_encryption: self.enable_encryption,
        }
    }

    /// Convert to MemoryEnhancedChatConfig
    #[must_use]
    pub const fn to_enhanced_chat_config(
        &self,
        system_prompt: Option<String>,
    ) -> application::MemoryEnhancedChatConfig {
        application::MemoryEnhancedChatConfig {
            enable_rag: self.enable_rag,
            enable_learning: self.enable_learning,
            system_prompt,
            min_learning_length: 20,
            default_importance: 0.5,
        }
    }

    /// Convert embedding config to ai_core::EmbeddingConfig
    #[must_use]
    pub fn to_embedding_config(&self, base_url: &str) -> ai_core::EmbeddingConfig {
        ai_core::EmbeddingConfig {
            base_url: base_url.to_string(),
            model: self.embedding.model.clone(),
            dimensions: self.embedding.dimension,
            timeout_ms: self.embedding.timeout_ms,
        }
    }
}

// Default value functions for memory config
const fn default_rag_limit() -> usize {
    5
}

const fn default_rag_threshold() -> f32 {
    0.5
}

const fn default_merge_threshold() -> f32 {
    0.85
}

const fn default_min_importance() -> f32 {
    0.1
}

const fn default_decay_factor() -> f32 {
    0.95
}

fn default_encryption_key_path() -> String {
    "memory_encryption.key".to_string()
}

fn default_embedding_model() -> String {
    "nomic-embed-text".to_string()
}

const fn default_embedding_dimension() -> usize {
    384
}

const fn default_embedding_timeout() -> u64 {
    30000
}
