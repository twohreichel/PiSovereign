//! Port for secret storage and retrieval
//!
//! This port defines the interface for securely retrieving secrets from
//! various backends (environment variables, HashiCorp Vault, etc.).

use async_trait::async_trait;
use serde::de::DeserializeOwned;

use crate::error::ApplicationError;

/// Port for secret storage operations
///
/// Implementations can retrieve secrets from various backends:
/// - Environment variables (for local development)
/// - HashiCorp Vault (for production)
/// - Other secret managers (AWS Secrets Manager, etc.)
///
/// This trait is object-safe to allow dynamic dispatch. For typed secret
/// retrieval, use the [`SecretStoreExt`] extension trait.
#[async_trait]
pub trait SecretStorePort: Send + Sync {
    /// Retrieve a secret by its key/path
    ///
    /// # Arguments
    /// * `key` - The key or path to the secret (e.g., "database/password" or "API_KEY")
    ///
    /// # Returns
    /// The secret value as a string, or an error if not found
    async fn get_secret(&self, key: &str) -> Result<String, ApplicationError>;

    /// Retrieve a structured secret as JSON value
    ///
    /// # Arguments
    /// * `path` - The path to the secret (e.g., "secret/data/myapp/database")
    ///
    /// # Returns
    /// The secret as a JSON value, or an error if not found
    async fn get_json(&self, path: &str) -> Result<serde_json::Value, ApplicationError>;

    /// Check if a secret exists
    ///
    /// # Arguments
    /// * `key` - The key or path to check
    ///
    /// # Returns
    /// `true` if the secret exists, `false` otherwise
    async fn exists(&self, key: &str) -> Result<bool, ApplicationError>;

    /// Check if the secret store is healthy and accessible
    async fn is_healthy(&self) -> bool;
}

/// Extension trait for typed secret retrieval
///
/// This trait provides a convenient method to retrieve and deserialize
/// secrets into strongly-typed structures.
#[async_trait]
pub trait SecretStoreExt: SecretStorePort {
    /// Retrieve a structured secret and deserialize it
    ///
    /// # Arguments
    /// * `path` - The path to the secret (e.g., "secret/data/myapp/database")
    ///
    /// # Returns
    /// The deserialized secret, or an error if not found or deserialization fails
    async fn get_typed<T: DeserializeOwned + Send>(&self, path: &str)
        -> Result<T, ApplicationError> {
        let value = self.get_json(path).await?;
        serde_json::from_value(value).map_err(|e| {
            ApplicationError::Configuration(format!("Failed to deserialize secret: {e}"))
        })
    }
}

// Blanket implementation for all types implementing SecretStorePort
impl<S: SecretStorePort + ?Sized> SecretStoreExt for S {}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::sync::Arc;
    use tokio::sync::RwLock;

    /// Mock secret store for testing
    #[derive(Debug, Default)]
    pub struct MockSecretStore {
        secrets: Arc<RwLock<HashMap<String, String>>>,
    }

    impl MockSecretStore {
        pub fn new() -> Self {
            Self::default()
        }

        pub async fn set_secret(&self, key: impl Into<String>, value: impl Into<String>) {
            self.secrets
                .write()
                .await
                .insert(key.into(), value.into());
        }
    }

    #[async_trait]
    impl SecretStorePort for MockSecretStore {
        async fn get_secret(&self, key: &str) -> Result<String, ApplicationError> {
            self.secrets
                .read()
                .await
                .get(key)
                .cloned()
                .ok_or_else(|| ApplicationError::NotFound(format!("Secret not found: {key}")))
        }

        async fn get_json(&self, path: &str) -> Result<serde_json::Value, ApplicationError> {
            let value = self.get_secret(path).await?;
            serde_json::from_str(&value)
                .map_err(|e| ApplicationError::Configuration(format!("Failed to parse secret: {e}")))
        }

        async fn exists(&self, key: &str) -> Result<bool, ApplicationError> {
            Ok(self.secrets.read().await.contains_key(key))
        }

        async fn is_healthy(&self) -> bool {
            true
        }
    }

    #[tokio::test]
    async fn mock_store_get_secret() {
        let store = MockSecretStore::new();
        store.set_secret("test/key", "secret_value").await;

        let result = store.get_secret("test/key").await.unwrap();
        assert_eq!(result, "secret_value");
    }

    #[tokio::test]
    async fn mock_store_secret_not_found() {
        let store = MockSecretStore::new();

        let result = store.get_secret("nonexistent").await;
        assert!(matches!(result, Err(ApplicationError::NotFound(_))));
    }

    #[tokio::test]
    async fn mock_store_exists() {
        let store = MockSecretStore::new();
        store.set_secret("exists/key", "value").await;

        assert!(store.exists("exists/key").await.unwrap());
        assert!(!store.exists("not/exists").await.unwrap());
    }

    #[tokio::test]
    async fn mock_store_get_typed() {
        use serde::Deserialize;

        #[derive(Debug, Deserialize, PartialEq)]
        struct DbConfig {
            username: String,
            password: String,
        }

        let store = MockSecretStore::new();
        store
            .set_secret(
                "database/config",
                r#"{"username": "admin", "password": "secret123"}"#,
            )
            .await;

        let config: DbConfig = store.get_typed("database/config").await.unwrap();
        assert_eq!(config.username, "admin");
        assert_eq!(config.password, "secret123");
    }

    #[tokio::test]
    async fn mock_store_is_healthy() {
        let store = MockSecretStore::new();
        assert!(store.is_healthy().await);
    }
}
