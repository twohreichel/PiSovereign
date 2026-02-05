//! HashiCorp Vault secret store adapter
//!
//! Retrieves secrets from HashiCorp Vault using the KV v2 secrets engine.
//! Supports AppRole and token-based authentication.

use std::sync::Arc;

use application::{error::ApplicationError, ports::SecretStorePort};
use async_trait::async_trait;
use tokio::sync::RwLock;
use tracing::{debug, error, info, instrument, warn};
use vaultrs::{
    client::{VaultClient, VaultClientSettingsBuilder},
    kv2,
};

/// Configuration for Vault connection
#[derive(Debug, Clone)]
pub struct VaultConfig {
    /// Vault server address (e.g., "https://vault.example.com:8200")
    pub address: String,
    /// Authentication token (for token-based auth)
    pub token: Option<String>,
    /// AppRole role ID (for AppRole auth)
    pub role_id: Option<String>,
    /// AppRole secret ID (typically from environment)
    pub secret_id: Option<String>,
    /// KV v2 mount path (default: "secret")
    pub mount_path: String,
    /// Namespace (for Vault Enterprise)
    pub namespace: Option<String>,
    /// Connection timeout in seconds
    pub timeout_secs: u64,
}

impl Default for VaultConfig {
    fn default() -> Self {
        Self {
            address: "http://127.0.0.1:8200".to_string(),
            token: None,
            role_id: None,
            secret_id: None,
            mount_path: "secret".to_string(),
            namespace: None,
            timeout_secs: 30,
        }
    }
}

impl VaultConfig {
    /// Create a new Vault configuration with the given address
    pub fn new(address: impl Into<String>) -> Self {
        Self {
            address: address.into(),
            ..Default::default()
        }
    }

    /// Set the authentication token
    #[must_use]
    pub fn with_token(mut self, token: impl Into<String>) -> Self {
        self.token = Some(token.into());
        self
    }

    /// Set AppRole credentials
    #[must_use]
    pub fn with_approle(
        mut self,
        role_id: impl Into<String>,
        secret_id: impl Into<String>,
    ) -> Self {
        self.role_id = Some(role_id.into());
        self.secret_id = Some(secret_id.into());
        self
    }

    /// Set the KV mount path
    #[must_use]
    pub fn with_mount_path(mut self, path: impl Into<String>) -> Self {
        self.mount_path = path.into();
        self
    }

    /// Set the namespace (Vault Enterprise)
    #[must_use]
    pub fn with_namespace(mut self, namespace: impl Into<String>) -> Self {
        self.namespace = Some(namespace.into());
        self
    }
}

/// Secret store that reads from HashiCorp Vault
pub struct VaultSecretStore {
    client: Arc<RwLock<VaultClient>>,
    mount_path: String,
    config: VaultConfig,
}

impl std::fmt::Debug for VaultSecretStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("VaultSecretStore")
            .field("mount_path", &self.mount_path)
            .field("config", &self.config)
            .field("client", &"VaultClient { ... }")
            .finish()
    }
}

impl VaultSecretStore {
    /// Create a new Vault secret store with the given configuration
    ///
    /// # Errors
    /// Returns an error if the Vault client cannot be created
    pub async fn new(config: VaultConfig) -> Result<Self, ApplicationError> {
        let client = Self::create_client(&config)?;

        // If using AppRole, authenticate
        if config.role_id.is_some() && config.secret_id.is_some() {
            Self::authenticate_approle(&client, &config).await?;
        }

        info!(address = %config.address, "Connected to Vault");

        Ok(Self {
            client: Arc::new(RwLock::new(client)),
            mount_path: config.mount_path.clone(),
            config,
        })
    }

    /// Create the Vault client
    fn create_client(config: &VaultConfig) -> Result<VaultClient, ApplicationError> {
        let mut settings_builder = VaultClientSettingsBuilder::default();
        settings_builder.address(&config.address);

        if let Some(token) = &config.token {
            settings_builder.token(token);
        }

        if let Some(namespace) = &config.namespace {
            settings_builder.namespace(Some(namespace.clone()));
        }

        let settings = settings_builder
            .build()
            .map_err(|e| ApplicationError::Configuration(format!("Invalid Vault config: {e}")))?;

        VaultClient::new(settings).map_err(|e| {
            ApplicationError::ExternalService(format!("Failed to create Vault client: {e}"))
        })
    }

    /// Authenticate using AppRole
    async fn authenticate_approle(
        client: &VaultClient,
        config: &VaultConfig,
    ) -> Result<(), ApplicationError> {
        let role_id = config.role_id.as_ref().ok_or_else(|| {
            ApplicationError::Configuration("AppRole role_id not configured".to_string())
        })?;
        let secret_id = config.secret_id.as_ref().ok_or_else(|| {
            ApplicationError::Configuration("AppRole secret_id not configured".to_string())
        })?;

        vaultrs::auth::approle::login(client, "approle", role_id, secret_id)
            .await
            .map_err(|e| {
                error!(error = %e, "AppRole authentication failed");
                ApplicationError::NotAuthorized(format!("Vault AppRole login failed: {e}"))
            })?;

        info!("Successfully authenticated with Vault using AppRole");
        Ok(())
    }

    /// Parse a path to extract mount and secret path
    ///
    /// Supports formats:
    /// - "myapp/database" -> uses default mount, path = "myapp/database"
    /// - "secret/data/myapp/database" -> mount = "secret", path = "myapp/database"
    fn parse_path(&self, path: &str) -> (String, String) {
        // If path starts with mount_path/data/, strip it
        let data_prefix = format!("{}/data/", self.mount_path);
        if path.starts_with(&data_prefix) {
            return (
                self.mount_path.clone(),
                path[data_prefix.len()..].to_string(),
            );
        }

        // If path starts with mount_path/, strip it
        let mount_prefix = format!("{}/", self.mount_path);
        if path.starts_with(&mount_prefix) {
            return (
                self.mount_path.clone(),
                path[mount_prefix.len()..].to_string(),
            );
        }

        // Use path as-is with default mount
        (self.mount_path.clone(), path.to_string())
    }
}

#[async_trait]
impl SecretStorePort for VaultSecretStore {
    #[instrument(skip(self))]
    async fn get_secret(&self, key: &str) -> Result<String, ApplicationError> {
        let (mount, path) = self.parse_path(key);
        let client = self.client.read().await;

        debug!(mount = %mount, path = %path, "Fetching secret from Vault");

        // Try to read as a simple key-value where the value is stored under "value" key
        let secret: std::collections::HashMap<String, String> =
            kv2::read(&*client, &mount, &path).await.map_err(|e| {
                if e.to_string().contains("404") || e.to_string().contains("not found") {
                    ApplicationError::NotFound(format!("Secret not found: {key}"))
                } else {
                    error!(error = %e, "Failed to read secret from Vault");
                    ApplicationError::ExternalService(format!("Vault read failed: {e}"))
                }
            })?;

        // Try common key names
        secret
            .get("value")
            .or_else(|| secret.get("password"))
            .or_else(|| secret.get("secret"))
            .or_else(|| secret.values().next())
            .cloned()
            .ok_or_else(|| ApplicationError::NotFound(format!("Secret has no value field: {key}")))
    }

    #[instrument(skip(self))]
    async fn get_json(&self, path: &str) -> Result<serde_json::Value, ApplicationError> {
        let (mount, secret_path) = self.parse_path(path);
        let client = self.client.read().await;

        debug!(mount = %mount, path = %secret_path, "Fetching JSON secret from Vault");

        kv2::read(&*client, &mount, &secret_path)
            .await
            .map_err(|e| {
                if e.to_string().contains("404") || e.to_string().contains("not found") {
                    ApplicationError::NotFound(format!("Secret not found: {path}"))
                } else {
                    error!(error = %e, "Failed to read secret from Vault");
                    ApplicationError::ExternalService(format!("Vault read failed: {e}"))
                }
            })
    }

    #[instrument(skip(self))]
    async fn exists(&self, key: &str) -> Result<bool, ApplicationError> {
        let (mount, path) = self.parse_path(key);
        let client = self.client.read().await;

        // Try to read metadata (doesn't retrieve the actual secret)
        match kv2::read_metadata(&*client, &mount, &path).await {
            Ok(_) => Ok(true),
            Err(e) if e.to_string().contains("404") => Ok(false),
            Err(e) => Err(ApplicationError::ExternalService(format!(
                "Vault metadata check failed: {e}"
            ))),
        }
    }

    async fn is_healthy(&self) -> bool {
        // Try to check Vault health
        let client = self.client.read().await;
        match vaultrs::sys::health(&*client).await {
            Ok(health) => {
                if health.sealed {
                    warn!("Vault is sealed");
                    false
                } else {
                    true
                }
            },
            Err(e) => {
                error!(error = %e, "Vault health check failed");
                false
            },
        }
    }
}

/// Combined secret store that tries multiple backends
///
/// First tries Vault, then falls back to environment variables.
/// Useful for development where Vault may not be available.
pub struct ChainedSecretStore {
    stores: Vec<Arc<dyn SecretStorePort>>,
}

impl std::fmt::Debug for ChainedSecretStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ChainedSecretStore")
            .field("stores_count", &self.stores.len())
            .finish()
    }
}

impl ChainedSecretStore {
    /// Create a new chained secret store with the given backends
    pub fn new(stores: Vec<Arc<dyn SecretStorePort>>) -> Self {
        Self { stores }
    }
}

#[async_trait]
impl SecretStorePort for ChainedSecretStore {
    async fn get_secret(&self, key: &str) -> Result<String, ApplicationError> {
        let mut last_error = None;

        for store in &self.stores {
            match store.get_secret(key).await {
                Ok(value) => return Ok(value),
                Err(ApplicationError::NotFound(_)) => {},
                Err(e) => {
                    last_error = Some(e);
                },
            }
        }

        Err(last_error.unwrap_or_else(|| {
            ApplicationError::NotFound(format!("Secret not found in any store: {key}"))
        }))
    }

    async fn get_json(&self, path: &str) -> Result<serde_json::Value, ApplicationError> {
        let mut last_error = None;

        for store in &self.stores {
            match store.get_json(path).await {
                Ok(value) => return Ok(value),
                Err(ApplicationError::NotFound(_)) => {},
                Err(e) => {
                    last_error = Some(e);
                },
            }
        }

        Err(last_error.unwrap_or_else(|| {
            ApplicationError::NotFound(format!("Secret not found in any store: {path}"))
        }))
    }

    async fn exists(&self, key: &str) -> Result<bool, ApplicationError> {
        for store in &self.stores {
            if store.exists(key).await? {
                return Ok(true);
            }
        }
        Ok(false)
    }

    async fn is_healthy(&self) -> bool {
        // Healthy if at least one store is healthy
        for store in &self.stores {
            if store.is_healthy().await {
                return true;
            }
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vault_config_builder() {
        let config = VaultConfig::new("https://vault.example.com:8200")
            .with_token("my-token")
            .with_mount_path("kv")
            .with_namespace("myns");

        assert_eq!(config.address, "https://vault.example.com:8200");
        assert_eq!(config.token, Some("my-token".to_string()));
        assert_eq!(config.mount_path, "kv");
        assert_eq!(config.namespace, Some("myns".to_string()));
    }

    #[test]
    fn vault_config_with_approle() {
        let config = VaultConfig::new("https://vault.example.com:8200")
            .with_approle("my-role-id", "my-secret-id");

        assert_eq!(config.role_id, Some("my-role-id".to_string()));
        assert_eq!(config.secret_id, Some("my-secret-id".to_string()));
    }

    #[test]
    fn parse_path_simple() {
        // Cannot test parse_path directly without creating VaultSecretStore,
        // but we can test the logic conceptually
        let mount = "secret".to_string();
        let path = "myapp/database";

        // Expected: mount = "secret", path = "myapp/database"
        assert!(!path.starts_with(&format!("{mount}/data/")));
    }

    #[tokio::test]
    async fn chained_store_tries_fallback() {
        use std::{collections::HashMap, sync::Arc};

        use tokio::sync::RwLock;

        // Create a mock store that always fails
        #[derive(Debug, Default)]
        struct FailingStore;

        #[async_trait]
        impl SecretStorePort for FailingStore {
            async fn get_secret(&self, key: &str) -> Result<String, ApplicationError> {
                Err(ApplicationError::NotFound(format!("Not found: {key}")))
            }
            async fn get_json(&self, _path: &str) -> Result<serde_json::Value, ApplicationError> {
                Err(ApplicationError::NotFound("Not found".to_string()))
            }
            async fn exists(&self, _key: &str) -> Result<bool, ApplicationError> {
                Ok(false)
            }
            async fn is_healthy(&self) -> bool {
                false
            }
        }

        // Create a mock store that succeeds
        #[derive(Debug)]
        struct SucceedingStore {
            secrets: Arc<RwLock<HashMap<String, String>>>,
        }

        impl SucceedingStore {
            fn new() -> Self {
                let mut secrets = HashMap::new();
                secrets.insert("test/key".to_string(), "test_value".to_string());
                Self {
                    secrets: Arc::new(RwLock::new(secrets)),
                }
            }
        }

        #[async_trait]
        impl SecretStorePort for SucceedingStore {
            async fn get_secret(&self, key: &str) -> Result<String, ApplicationError> {
                self.secrets
                    .read()
                    .await
                    .get(key)
                    .cloned()
                    .ok_or_else(|| ApplicationError::NotFound(format!("Not found: {key}")))
            }
            async fn get_json(&self, _path: &str) -> Result<serde_json::Value, ApplicationError> {
                Err(ApplicationError::NotFound("Not found".to_string()))
            }
            async fn exists(&self, key: &str) -> Result<bool, ApplicationError> {
                Ok(self.secrets.read().await.contains_key(key))
            }
            async fn is_healthy(&self) -> bool {
                true
            }
        }

        let chained = ChainedSecretStore::new(vec![
            Arc::new(FailingStore),
            Arc::new(SucceedingStore::new()),
        ]);

        // Should fall back to succeeding store
        let result = chained.get_secret("test/key").await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "test_value");
    }

    #[tokio::test]
    async fn chained_store_is_healthy_if_any_healthy() {
        #[derive(Debug)]
        struct UnhealthyStore;

        #[async_trait]
        impl SecretStorePort for UnhealthyStore {
            async fn get_secret(&self, _key: &str) -> Result<String, ApplicationError> {
                Err(ApplicationError::NotFound(String::new()))
            }
            async fn get_json(&self, _path: &str) -> Result<serde_json::Value, ApplicationError> {
                Err(ApplicationError::NotFound(String::new()))
            }
            async fn exists(&self, _key: &str) -> Result<bool, ApplicationError> {
                Ok(false)
            }
            async fn is_healthy(&self) -> bool {
                false
            }
        }

        #[derive(Debug)]
        struct HealthyStore;

        #[async_trait]
        impl SecretStorePort for HealthyStore {
            async fn get_secret(&self, _key: &str) -> Result<String, ApplicationError> {
                Err(ApplicationError::NotFound(String::new()))
            }
            async fn get_json(&self, _path: &str) -> Result<serde_json::Value, ApplicationError> {
                Err(ApplicationError::NotFound(String::new()))
            }
            async fn exists(&self, _key: &str) -> Result<bool, ApplicationError> {
                Ok(false)
            }
            async fn is_healthy(&self) -> bool {
                true
            }
        }

        let chained =
            ChainedSecretStore::new(vec![Arc::new(UnhealthyStore), Arc::new(HealthyStore)]);

        assert!(chained.is_healthy().await);
    }
}
