//! Environment-based secret store adapter
//!
//! Reads secrets from environment variables. Useful for local development
//! and containerized deployments where secrets are injected via environment.

use application::{error::ApplicationError, ports::SecretStorePort};
use async_trait::async_trait;
use std::env;
use tracing::{debug, instrument, warn};

/// Secret store that reads from environment variables
///
/// Keys are transformed to uppercase with slashes replaced by underscores.
/// For example: "database/password" becomes "DATABASE_PASSWORD"
#[derive(Debug, Clone, Default)]
pub struct EnvSecretStore {
    /// Optional prefix for all environment variable lookups
    prefix: Option<String>,
}

impl EnvSecretStore {
    /// Create a new environment secret store
    pub fn new() -> Self {
        Self::default()
    }

    /// Create with a prefix for all environment variable lookups
    ///
    /// # Example
    /// ```
    /// use infrastructure::adapters::EnvSecretStore;
    ///
    /// let store = EnvSecretStore::with_prefix("PISOVEREIGN");
    /// // Looking up "database/password" will check "PISOVEREIGN_DATABASE_PASSWORD"
    /// ```
    pub fn with_prefix(prefix: impl Into<String>) -> Self {
        Self {
            prefix: Some(prefix.into()),
        }
    }

    /// Transform a key path to an environment variable name
    ///
    /// Converts slashes to underscores, hyphens to underscores, and uppercases.
    fn key_to_env_var(&self, key: &str) -> String {
        let normalized = key
            .replace('/', "_")
            .replace('-', "_")
            .to_uppercase();

        match &self.prefix {
            Some(prefix) => format!("{prefix}_{normalized}"),
            None => normalized,
        }
    }
}

#[async_trait]
impl SecretStorePort for EnvSecretStore {
    #[instrument(skip(self), fields(env_var))]
    async fn get_secret(&self, key: &str) -> Result<String, ApplicationError> {
        let env_var = self.key_to_env_var(key);
        tracing::Span::current().record("env_var", &env_var);

        match env::var(&env_var) {
            Ok(value) => {
                debug!("Retrieved secret from environment variable");
                Ok(value)
            }
            Err(env::VarError::NotPresent) => {
                warn!(env_var = %env_var, "Secret not found in environment");
                Err(ApplicationError::NotFound(format!(
                    "Secret not found: {key} (env: {env_var})"
                )))
            }
            Err(env::VarError::NotUnicode(_)) => {
                Err(ApplicationError::Configuration(format!(
                    "Secret contains invalid UTF-8: {env_var}"
                )))
            }
        }
    }

    #[instrument(skip(self))]
    async fn get_json(&self, path: &str) -> Result<serde_json::Value, ApplicationError> {
        let value = self.get_secret(path).await?;
        serde_json::from_str(&value).map_err(|e| {
            ApplicationError::Configuration(format!(
                "Failed to parse secret as JSON: {e}"
            ))
        })
    }

    async fn exists(&self, key: &str) -> Result<bool, ApplicationError> {
        let env_var = self.key_to_env_var(key);
        Ok(env::var(&env_var).is_ok())
    }

    async fn is_healthy(&self) -> bool {
        // Environment variables are always accessible
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use application::ports::SecretStoreExt;

    #[test]
    fn key_transformation_simple() {
        let store = EnvSecretStore::new();
        assert_eq!(store.key_to_env_var("api_key"), "API_KEY");
    }

    #[test]
    fn key_transformation_with_slashes() {
        let store = EnvSecretStore::new();
        assert_eq!(store.key_to_env_var("database/password"), "DATABASE_PASSWORD");
    }

    #[test]
    fn key_transformation_with_hyphens() {
        let store = EnvSecretStore::new();
        assert_eq!(store.key_to_env_var("my-secret-key"), "MY_SECRET_KEY");
    }

    #[test]
    fn key_transformation_with_prefix() {
        let store = EnvSecretStore::with_prefix("PISOVEREIGN");
        assert_eq!(
            store.key_to_env_var("database/password"),
            "PISOVEREIGN_DATABASE_PASSWORD"
        );
    }

    #[test]
    fn key_transformation_complex_path() {
        let store = EnvSecretStore::with_prefix("APP");
        assert_eq!(
            store.key_to_env_var("secret/data/myapp/db-credentials"),
            "APP_SECRET_DATA_MYAPP_DB_CREDENTIALS"
        );
    }

    #[tokio::test]
    async fn get_secret_from_existing_env() {
        // Use PATH which is guaranteed to exist on all systems
        let store = EnvSecretStore::new();
        let result = store.get_secret("path").await;

        // PATH should exist and have a value
        assert!(result.is_ok());
        assert!(!result.unwrap().is_empty());
    }

    #[tokio::test]
    async fn get_secret_not_found() {
        let store = EnvSecretStore::new();
        let result = store.get_secret("definitely/not/exists/xyz789").await;

        assert!(matches!(result, Err(ApplicationError::NotFound(_))));
    }

    #[tokio::test]
    async fn exists_returns_true_for_existing() {
        // Use PATH which is guaranteed to exist
        let store = EnvSecretStore::new();
        let result = store.exists("path").await.unwrap();

        assert!(result);
    }

    #[tokio::test]
    async fn exists_returns_false_for_missing() {
        let store = EnvSecretStore::new();
        let result = store.exists("missing/key/abc").await.unwrap();

        assert!(!result);
    }

    #[tokio::test]
    async fn is_healthy_always_true() {
        let store = EnvSecretStore::new();
        assert!(store.is_healthy().await);
    }

    #[tokio::test]
    async fn get_json_parses_valid_json_from_env() {
        // Test the get_json method with a non-JSON value (should fail)
        // Note: We can't easily set env vars in tests due to unsafe restrictions,
        // so we test the error path
        let store = EnvSecretStore::new();

        // PATH is not valid JSON, so this should fail with a parse error
        let result: Result<serde_json::Value, _> = store.get_json("path").await;
        assert!(matches!(result, Err(ApplicationError::Configuration(_))));
    }

    #[tokio::test]
    async fn get_typed_requires_extension_trait() {
        use serde::Deserialize;

        #[derive(Debug, Deserialize, PartialEq)]
        struct Config {
            host: String,
            port: u16,
        }

        // This test verifies the extension trait compiles and is usable
        // We can't easily test with real JSON env vars due to unsafe restrictions
        let store = EnvSecretStore::new();

        // This should fail because the env var doesn't exist
        let result: Result<Config, _> = store.get_typed("nonexistent/config").await;
        assert!(matches!(result, Err(ApplicationError::NotFound(_))));
    }
}
