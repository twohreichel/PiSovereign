//! Security configuration: API keys, rate limiting, TLS, prompt injection.

use std::net::IpAddr;

use serde::{Deserialize, Serialize};

use super::default_true;

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
