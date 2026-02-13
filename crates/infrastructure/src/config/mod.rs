//! Application configuration
//!
//! Split into focused sub-modules by domain:
//! - `server`: HTTP server settings
//! - `security`: Authentication, rate limiting, TLS
//! - `cache`: Cache TTL configuration
//! - `messenger`: WhatsApp, Signal, persistence
//! - `database`: SQLite database settings
//! - `integrations`: Weather, web search, CalDAV, Proton, transit
//! - `resilience`: Telemetry, retry, degraded mode, health
//! - `memory`: Memory/RAG, embeddings, reminders

mod cache;
mod database;
mod integrations;
mod memory;
mod messenger;
mod resilience;
mod security;
mod server;
mod vault;

use ai_core::InferenceConfig;
use ai_speech::SpeechConfig;
use application::ports::SecretStorePort;
use domain::MessengerSource;
use secrecy::{ExposeSecret, SecretString};
use serde::{Deserialize, Serialize};
use std::fmt;
use tracing::{debug, info, warn};

pub use cache::CacheConfig;
pub use database::DatabaseConfig;
pub use integrations::{
    CalDavAppConfig, GeoLocationConfig, ProtonAppConfig, ProtonTlsAppConfig, TransitAppConfig,
    WeatherConfig, WebSearchAppConfig,
};
pub use memory::{EmbeddingAppConfig, MemoryAppConfig, ReminderAppConfig};
pub use messenger::{MessengerPersistenceConfig, SignalConfig, WhatsAppConfig};
pub use resilience::{DegradedModeAppConfig, HealthAppConfig, RetryAppConfig, TelemetryAppConfig};
pub use security::{ApiKeyEntry, PromptSecurityConfig, SecurityConfig};
pub use server::ServerConfig;
pub use vault::VaultAppConfig;

/// Shared default for boolean `true` fields across config structs
pub(crate) const fn default_true() -> bool {
    true
}

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

    /// Public transit configuration (optional, for ÖPNV connections)
    #[serde(default)]
    pub transit: Option<TransitAppConfig>,

    /// Reminder configuration (optional, for reminder system settings)
    #[serde(default)]
    pub reminder: Option<ReminderAppConfig>,

    /// Vault secret store configuration (optional)
    #[serde(default)]
    pub vault: VaultAppConfig,
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

    /// Resolve secrets from a secret store (e.g., Vault) into the config
    ///
    /// Only populates fields that are currently empty/None. Existing config
    /// values are never overridden, allowing config.toml or env vars to
    /// take precedence over Vault.
    ///
    /// # Secret paths
    ///
    /// Secrets are read as JSON objects from Vault KV v2 paths:
    /// - `{prefix}/whatsapp`: `access_token`, `app_secret`
    /// - `{prefix}/websearch`: `api_key`
    /// - `{prefix}/caldav`: `password`
    /// - `{prefix}/proton`: `password`
    /// - `{prefix}/speech`: `openai_api_key`
    ///
    /// # Errors
    ///
    /// Logs warnings for individual secret resolution failures but does not
    /// fail the entire operation. Returns error only for critical failures.
    pub async fn resolve_secrets(
        &mut self,
        store: &dyn SecretStorePort,
    ) -> Result<(), application::error::ApplicationError> {
        let prefix = self.vault.secret_prefix.clone();
        info!(prefix = %prefix, "Resolving secrets from secret store");

        // WhatsApp secrets
        self.resolve_whatsapp_secrets(store, &prefix).await;

        // Web search secrets
        self.resolve_websearch_secrets(store, &prefix).await;

        // CalDAV secrets
        self.resolve_caldav_secrets(store, &prefix).await;

        // Proton Mail secrets
        self.resolve_proton_secrets(store, &prefix).await;

        // Speech secrets
        self.resolve_speech_secrets(store, &prefix).await;

        // Signal secrets
        self.resolve_signal_secrets(store, &prefix).await;

        info!("Secret resolution completed");
        Ok(())
    }

    async fn resolve_whatsapp_secrets(&mut self, store: &dyn SecretStorePort, prefix: &str) {
        let path = format!("{prefix}/whatsapp");
        match store.get_json(&path).await {
            Ok(json) => {
                if self.whatsapp.access_token.is_none() {
                    if let Some(val) = json.get("access_token").and_then(|v| v.as_str()) {
                        if !val.is_empty() {
                            self.whatsapp.access_token = Some(SecretString::from(val.to_owned()));
                            debug!("Loaded whatsapp.access_token from secret store");
                        }
                    }
                }
                if self.whatsapp.app_secret.is_none() {
                    if let Some(val) = json.get("app_secret").and_then(|v| v.as_str()) {
                        if !val.is_empty() {
                            self.whatsapp.app_secret = Some(SecretString::from(val.to_owned()));
                            debug!("Loaded whatsapp.app_secret from secret store");
                        }
                    }
                }
            },
            Err(e) => warn!(path = %path, error = %e, "Failed to resolve WhatsApp secrets"),
        }
    }

    async fn resolve_websearch_secrets(&mut self, store: &dyn SecretStorePort, prefix: &str) {
        if let Some(ref mut ws) = self.websearch {
            if ws.api_key.is_none() {
                let path = format!("{prefix}/websearch");
                match store.get_json(&path).await {
                    Ok(json) => {
                        if let Some(val) = json.get("api_key").and_then(|v| v.as_str()) {
                            if !val.is_empty() {
                                ws.api_key = Some(val.to_owned());
                                debug!("Loaded websearch.api_key from secret store");
                            }
                        }
                    },
                    Err(e) => {
                        warn!(path = %path, error = %e, "Failed to resolve web search secrets");
                    },
                }
            }
        }
    }

    async fn resolve_caldav_secrets(&mut self, store: &dyn SecretStorePort, prefix: &str) {
        if let Some(ref mut caldav) = self.caldav {
            let path = format!("{prefix}/caldav");
            match store.get_json(&path).await {
                Ok(json) => {
                    if caldav.password.expose_secret().is_empty() {
                        if let Some(val) = json.get("password").and_then(|v| v.as_str()) {
                            if !val.is_empty() {
                                caldav.password = SecretString::from(val.to_owned());
                                debug!("Loaded caldav.password from secret store");
                            }
                        }
                    }
                },
                Err(e) => warn!(path = %path, error = %e, "Failed to resolve CalDAV secrets"),
            }
        }
    }

    async fn resolve_proton_secrets(&mut self, store: &dyn SecretStorePort, prefix: &str) {
        if let Some(ref mut proton) = self.proton {
            let path = format!("{prefix}/proton");
            match store.get_json(&path).await {
                Ok(json) => {
                    if proton.password.expose_secret().is_empty() {
                        if let Some(val) = json.get("password").and_then(|v| v.as_str()) {
                            if !val.is_empty() {
                                proton.password = SecretString::from(val.to_owned());
                                debug!("Loaded proton.password from secret store");
                            }
                        }
                    }
                },
                Err(e) => warn!(path = %path, error = %e, "Failed to resolve Proton secrets"),
            }
        }
    }

    async fn resolve_signal_secrets(&mut self, store: &dyn SecretStorePort, prefix: &str) {
        if self.signal.phone_number.is_empty() {
            let path = format!("{prefix}/signal");
            match store.get_json(&path).await {
                Ok(json) => {
                    if let Some(val) = json.get("phone_number").and_then(|v| v.as_str()) {
                        if !val.is_empty() {
                            self.signal.phone_number = val.to_owned();
                            debug!("Loaded signal.phone_number from secret store");
                        }
                    }
                },
                Err(e) => warn!(path = %path, error = %e, "Failed to resolve Signal secrets"),
            }
        }
    }

    async fn resolve_speech_secrets(&mut self, store: &dyn SecretStorePort, prefix: &str) {
        if let Some(ref mut speech) = self.speech {
            if speech.openai_api_key.is_none() {
                let path = format!("{prefix}/speech");
                match store.get_json(&path).await {
                    Ok(json) => {
                        if let Some(val) = json.get("openai_api_key").and_then(|v| v.as_str()) {
                            if !val.is_empty() {
                                speech.openai_api_key = Some(val.to_owned());
                                debug!("Loaded speech.openai_api_key from secret store");
                            }
                        }
                    },
                    Err(e) => {
                        warn!(path = %path, error = %e, "Failed to resolve speech secrets");
                    },
                }
            }
        }
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
        let config = PromptSecurityConfig {
            sensitivity: "low".to_string(),
            ..Default::default()
        };
        assert_eq!(
            config.sensitivity_level(),
            application::services::SecuritySensitivity::Low
        );

        let config = PromptSecurityConfig {
            sensitivity: "medium".to_string(),
            ..Default::default()
        };
        assert_eq!(
            config.sensitivity_level(),
            application::services::SecuritySensitivity::Medium
        );

        let config = PromptSecurityConfig {
            sensitivity: "high".to_string(),
            ..Default::default()
        };
        assert_eq!(
            config.sensitivity_level(),
            application::services::SecuritySensitivity::High
        );

        // Unknown defaults to medium
        let config = PromptSecurityConfig {
            sensitivity: "unknown".to_string(),
            ..Default::default()
        };
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
            persistence: MessengerPersistenceConfig::default(),
        };
        let json = serde_json::to_string(&config).unwrap();
        let parsed: SignalConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.phone_number, "+1234567890");
        assert_eq!(parsed.socket_path, "/custom/socket");
        assert_eq!(parsed.data_path, Some("/data".to_string()));
        assert_eq!(parsed.timeout_ms, 60000);
        assert_eq!(parsed.whitelist.len(), 1);
        assert!(parsed.persistence.enabled);
    }

    #[test]
    fn signal_config_debug() {
        let config = SignalConfig::default();
        let debug = format!("{config:?}");
        assert!(debug.contains("SignalConfig"));
        assert!(debug.contains("phone_number"));
        assert!(debug.contains("socket_path"));
    }

    #[test]
    fn messenger_persistence_config_default() {
        let config = MessengerPersistenceConfig::default();
        assert!(config.enabled);
        assert!(config.enable_encryption);
        assert!(config.enable_rag);
        assert!(config.enable_learning);
        assert!(config.retention_days.is_none());
        assert!(config.max_messages_per_conversation.is_none());
        assert_eq!(config.context_window, 50);
    }

    #[test]
    fn messenger_persistence_config_serialization() {
        let config = MessengerPersistenceConfig {
            enabled: true,
            enable_encryption: false,
            enable_rag: true,
            enable_learning: false,
            retention_days: Some(90),
            max_messages_per_conversation: Some(1000),
            context_window: 25,
        };
        let json = serde_json::to_string(&config).unwrap();
        let parsed: MessengerPersistenceConfig = serde_json::from_str(&json).unwrap();
        assert!(parsed.enabled);
        assert!(!parsed.enable_encryption);
        assert!(parsed.enable_rag);
        assert!(!parsed.enable_learning);
        assert_eq!(parsed.retention_days, Some(90));
        assert_eq!(parsed.max_messages_per_conversation, Some(1000));
        assert_eq!(parsed.context_window, 25);
    }

    #[test]
    fn messenger_persistence_config_debug() {
        let config = MessengerPersistenceConfig::default();
        let debug = format!("{config:?}");
        assert!(debug.contains("MessengerPersistenceConfig"));
        assert!(debug.contains("enabled"));
        assert!(debug.contains("enable_rag"));
    }

    #[test]
    fn whatsapp_config_access_token_str() {
        use secrecy::SecretString;

        let config = WhatsAppConfig {
            access_token: Some(SecretString::from("test_token")),
            ..Default::default()
        };
        assert_eq!(config.access_token_str(), Some("test_token"));

        let config_no_token = WhatsAppConfig::default();
        assert!(config_no_token.access_token_str().is_none());
    }

    #[test]
    fn whatsapp_config_app_secret_str() {
        use secrecy::{ExposeSecret, SecretString};

        let config = WhatsAppConfig {
            app_secret: Some(SecretString::from("test_secret")),
            ..Default::default()
        };
        assert_eq!(
            config.app_secret_str().map(ExposeSecret::expose_secret),
            Some("test_secret")
        );

        let config_no_secret = WhatsAppConfig::default();
        assert!(config_no_secret.app_secret_str().is_none());
    }
}
