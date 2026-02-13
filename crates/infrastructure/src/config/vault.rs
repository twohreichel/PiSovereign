//! Vault secret store configuration
//!
//! Configuration for HashiCorp Vault integration, enabling centralized
//! secret management. Secrets are loaded from Vault at startup and injected
//! into the application configuration.

use secrecy::{ExposeSecret, SecretString};
use serde::{Deserialize, Serialize};

use crate::adapters::VaultConfig;

/// Vault secret store configuration
///
/// When enabled, secrets are loaded from HashiCorp Vault at startup
/// and injected into the appropriate configuration sections.
/// Secrets already set in config.toml are not overridden.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VaultAppConfig {
    /// Enable Vault secret store integration
    #[serde(default)]
    pub enabled: bool,

    /// Vault server address
    #[serde(default = "default_vault_address")]
    pub address: String,

    /// Authentication token (prefer env var PISOVEREIGN_VAULT_TOKEN)
    #[serde(default, skip_serializing)]
    pub token: Option<SecretString>,

    /// AppRole role ID (alternative to token auth)
    #[serde(default)]
    pub role_id: Option<String>,

    /// AppRole secret ID (alternative to token auth)
    #[serde(default, skip_serializing)]
    pub secret_id: Option<SecretString>,

    /// KV v2 mount path
    #[serde(default = "default_mount_path")]
    pub mount_path: String,

    /// Secret path prefix for PiSovereign secrets
    #[serde(default = "default_secret_prefix")]
    pub secret_prefix: String,

    /// Vault Enterprise namespace
    #[serde(default)]
    pub namespace: Option<String>,

    /// Connection timeout in seconds
    #[serde(default = "default_timeout_secs")]
    pub timeout_secs: u64,

    /// Enable environment variable fallback via `ChainedSecretStore`
    #[serde(default = "super::default_true")]
    pub env_fallback: bool,

    /// Environment variable prefix for fallback lookups
    #[serde(default = "default_env_prefix")]
    pub env_prefix: Option<String>,
}

fn default_vault_address() -> String {
    "http://127.0.0.1:8200".to_string()
}

fn default_mount_path() -> String {
    "secret".to_string()
}

fn default_secret_prefix() -> String {
    "pisovereign".to_string()
}

const fn default_timeout_secs() -> u64 {
    5
}

#[allow(clippy::unnecessary_wraps)]
fn default_env_prefix() -> Option<String> {
    Some(String::from("PISOVEREIGN"))
}

impl Default for VaultAppConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            address: default_vault_address(),
            token: None,
            role_id: None,
            secret_id: None,
            mount_path: default_mount_path(),
            secret_prefix: default_secret_prefix(),
            namespace: None,
            timeout_secs: default_timeout_secs(),
            env_fallback: true,
            env_prefix: default_env_prefix(),
        }
    }
}

impl VaultAppConfig {
    /// Convert to the adapter-level `VaultConfig`
    #[must_use]
    pub fn to_vault_config(&self) -> VaultConfig {
        let mut config = VaultConfig::new(&self.address);
        config.mount_path.clone_from(&self.mount_path);
        config.timeout_secs = self.timeout_secs;

        if let Some(ref token) = self.token {
            config.token = Some(token.expose_secret().to_string());
        }

        if let (Some(role_id), Some(secret_id)) = (&self.role_id, &self.secret_id) {
            config.role_id = Some(role_id.clone());
            config.secret_id = Some(secret_id.expose_secret().to_string());
        }

        if let Some(ref ns) = self.namespace {
            config.namespace = Some(ns.clone());
        }

        config
    }

    /// Build the full secret path for a given service
    #[must_use]
    pub fn secret_path(&self, service: &str) -> String {
        format!("{}/{}", self.secret_prefix, service)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_is_disabled() {
        let config = VaultAppConfig::default();
        assert!(!config.enabled);
        assert_eq!(config.address, "http://127.0.0.1:8200");
        assert_eq!(config.mount_path, "secret");
        assert_eq!(config.secret_prefix, "pisovereign");
        assert_eq!(config.timeout_secs, 5);
        assert!(config.env_fallback);
        assert_eq!(config.env_prefix, Some("PISOVEREIGN".to_string()));
    }

    #[test]
    fn to_vault_config_basic() {
        let config = VaultAppConfig {
            enabled: true,
            address: "http://vault:8200".to_string(),
            mount_path: "kv".to_string(),
            timeout_secs: 10,
            ..Default::default()
        };

        let vault_config = config.to_vault_config();
        assert_eq!(vault_config.address, "http://vault:8200");
        assert_eq!(vault_config.mount_path, "kv");
        assert_eq!(vault_config.timeout_secs, 10);
        assert!(vault_config.token.is_none());
    }

    #[test]
    fn to_vault_config_with_token() {
        let config = VaultAppConfig {
            enabled: true,
            token: Some(SecretString::from("test-token")),
            ..Default::default()
        };

        let vault_config = config.to_vault_config();
        assert_eq!(vault_config.token.as_deref(), Some("test-token"));
    }

    #[test]
    fn to_vault_config_with_approle() {
        let config = VaultAppConfig {
            enabled: true,
            role_id: Some("role-123".to_string()),
            secret_id: Some(SecretString::from("secret-456")),
            ..Default::default()
        };

        let vault_config = config.to_vault_config();
        assert_eq!(vault_config.role_id.as_deref(), Some("role-123"));
        assert_eq!(vault_config.secret_id.as_deref(), Some("secret-456"));
    }

    #[test]
    fn secret_path_construction() {
        let config = VaultAppConfig::default();
        assert_eq!(config.secret_path("whatsapp"), "pisovereign/whatsapp");
        assert_eq!(config.secret_path("signal"), "pisovereign/signal");
    }

    #[test]
    fn secret_path_custom_prefix() {
        let config = VaultAppConfig {
            secret_prefix: "myapp".to_string(),
            ..Default::default()
        };
        assert_eq!(config.secret_path("proton"), "myapp/proton");
    }

    #[test]
    fn serde_roundtrip() {
        let toml_str = r#"
            enabled = true
            address = "http://vault:8200"
            mount_path = "secret"
            secret_prefix = "pisovereign"
            timeout_secs = 5
            env_fallback = true
        "#;

        let config: VaultAppConfig = toml::from_str(toml_str).unwrap();
        assert!(config.enabled);
        assert_eq!(config.address, "http://vault:8200");
        assert!(config.token.is_none());
    }

    #[test]
    fn serde_default_values() {
        let config: VaultAppConfig = toml::from_str("").unwrap();
        assert!(!config.enabled);
        assert_eq!(config.address, "http://127.0.0.1:8200");
        assert_eq!(config.mount_path, "secret");
    }

    #[test]
    fn to_vault_config_with_namespace() {
        let config = VaultAppConfig {
            namespace: Some("engineering".to_string()),
            ..Default::default()
        };

        let vault_config = config.to_vault_config();
        assert_eq!(vault_config.namespace.as_deref(), Some("engineering"));
    }
}
