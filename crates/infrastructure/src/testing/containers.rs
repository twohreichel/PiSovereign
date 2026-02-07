//! Container wrappers for testcontainers integration.
//!
//! Provides convenient wrappers around testcontainers for PostgreSQL and Redis,
//! including connection pooling and health checks.

// Allow unused code in this module - these are test utilities that will be used
// by integration tests, not all of which may be written yet
#![allow(dead_code)]

use std::time::Duration;

use testcontainers::{
    ContainerAsync, GenericImage, ImageExt,
    core::{ContainerPort, WaitFor},
    runners::AsyncRunner,
};
use testcontainers_modules::{postgres::Postgres, redis::Redis};
use tracing::{debug, info};

/// Configuration for PostgreSQL container
#[derive(Debug, Clone)]
pub struct PostgresContainerConfig {
    /// Database name to create
    pub database: String,
    /// Username
    pub username: String,
    /// Password
    pub password: String,
    /// Postgres version tag (e.g., "16-alpine")
    pub version: String,
}

impl Default for PostgresContainerConfig {
    fn default() -> Self {
        Self {
            database: "pisovereign_test".to_string(),
            username: "test".to_string(),
            password: "test".to_string(),
            version: "16-alpine".to_string(),
        }
    }
}

/// PostgreSQL container wrapper for integration tests.
///
/// Provides a running PostgreSQL container with connection pooling support.
#[derive(Debug)]
pub struct PostgresContainer {
    #[allow(dead_code)]
    container: ContainerAsync<Postgres>,
    connection_string: String,
    host: String,
    port: u16,
}

impl PostgresContainer {
    /// Start a new PostgreSQL container with default configuration.
    ///
    /// # Errors
    ///
    /// Returns an error if the container fails to start.
    pub async fn start() -> Result<Self, ContainerError> {
        Self::start_with_config(PostgresContainerConfig::default()).await
    }

    /// Start a new PostgreSQL container with custom configuration.
    ///
    /// # Errors
    ///
    /// Returns an error if the container fails to start.
    pub async fn start_with_config(
        config: PostgresContainerConfig,
    ) -> Result<Self, ContainerError> {
        info!(
            database = %config.database,
            version = %config.version,
            "Starting PostgreSQL container"
        );

        let container = Postgres::default()
            .with_db_name(&config.database)
            .with_user(&config.username)
            .with_password(&config.password)
            .with_tag(&config.version)
            .start()
            .await
            .map_err(|e| ContainerError::Start(e.to_string()))?;

        let host = container
            .get_host()
            .await
            .map_err(|e| ContainerError::Start(e.to_string()))?
            .to_string();

        let port = container
            .get_host_port_ipv4(5432)
            .await
            .map_err(|e| ContainerError::Start(e.to_string()))?;

        let connection_string = format!(
            "postgres://{}:{}@{}:{}/{}",
            config.username, config.password, host, port, config.database
        );

        debug!(
            host = %host,
            port = %port,
            "PostgreSQL container started"
        );

        Ok(Self {
            container,
            connection_string,
            host,
            port,
        })
    }

    /// Get the connection string for this PostgreSQL instance.
    pub fn connection_string(&self) -> &str {
        &self.connection_string
    }

    /// Get the host address.
    pub fn host(&self) -> &str {
        &self.host
    }

    /// Get the mapped port.
    pub const fn port(&self) -> u16 {
        self.port
    }
}

/// Configuration for Redis container
#[derive(Debug, Clone)]
pub struct RedisContainerConfig {
    /// Redis version tag (e.g., "7-alpine")
    pub version: String,
}

impl Default for RedisContainerConfig {
    fn default() -> Self {
        Self {
            version: "7-alpine".to_string(),
        }
    }
}

/// Redis container wrapper for integration tests.
///
/// Provides a running Redis container for caching tests.
#[derive(Debug)]
pub struct RedisContainer {
    #[allow(dead_code)]
    container: ContainerAsync<Redis>,
    connection_string: String,
    host: String,
    port: u16,
}

impl RedisContainer {
    /// Start a new Redis container with default configuration.
    ///
    /// # Errors
    ///
    /// Returns an error if the container fails to start.
    pub async fn start() -> Result<Self, ContainerError> {
        Self::start_with_config(RedisContainerConfig::default()).await
    }

    /// Start a new Redis container with custom configuration.
    ///
    /// # Errors
    ///
    /// Returns an error if the container fails to start.
    pub async fn start_with_config(config: RedisContainerConfig) -> Result<Self, ContainerError> {
        info!(version = %config.version, "Starting Redis container");

        let container = Redis::default()
            .with_tag(&config.version)
            .start()
            .await
            .map_err(|e| ContainerError::Start(e.to_string()))?;

        let host = container
            .get_host()
            .await
            .map_err(|e| ContainerError::Start(e.to_string()))?
            .to_string();

        let port = container
            .get_host_port_ipv4(6379)
            .await
            .map_err(|e| ContainerError::Start(e.to_string()))?;

        let connection_string = format!("redis://{}:{}", host, port);

        debug!(
            host = %host,
            port = %port,
            "Redis container started"
        );

        Ok(Self {
            container,
            connection_string,
            host,
            port,
        })
    }

    /// Get the connection string for this Redis instance.
    pub fn connection_string(&self) -> &str {
        &self.connection_string
    }

    /// Get the host address.
    pub fn host(&self) -> &str {
        &self.host
    }

    /// Get the mapped port.
    pub const fn port(&self) -> u16 {
        self.port
    }
}

/// Generic container for custom images (e.g., Ollama)
#[derive(Debug)]
pub struct GenericContainer {
    #[allow(dead_code)]
    container: ContainerAsync<GenericImage>,
    host: String,
    ports: Vec<(u16, u16)>, // (container_port, host_port)
}

/// Configuration for a generic container
#[derive(Debug, Clone)]
pub struct GenericContainerConfig {
    /// Docker image name
    pub image: String,
    /// Image tag
    pub tag: String,
    /// Ports to expose (container ports)
    pub exposed_ports: Vec<u16>,
    /// Wait strategy (text to wait for in logs)
    pub wait_for_message: Option<String>,
    /// Startup timeout
    pub startup_timeout: Duration,
    /// Environment variables
    pub env_vars: Vec<(String, String)>,
}

impl GenericContainer {
    /// Start a container with the given configuration.
    ///
    /// # Errors
    ///
    /// Returns an error if the container fails to start.
    pub async fn start(config: GenericContainerConfig) -> Result<Self, ContainerError> {
        info!(
            image = %config.image,
            tag = %config.tag,
            "Starting generic container"
        );

        let mut image = GenericImage::new(&config.image, &config.tag);

        // Add exposed ports
        for port in &config.exposed_ports {
            image = image.with_exposed_port(ContainerPort::Tcp(*port));
        }

        // Add wait condition
        if let Some(ref message) = config.wait_for_message {
            image = image.with_wait_for(WaitFor::message_on_stdout(message.clone()));
        }

        // Add environment variables - this converts to ContainerRequest
        let mut container_request = image.with_env_var("_INIT", "1"); // Dummy to convert type
        for (key, value) in &config.env_vars {
            container_request = container_request.with_env_var(key, value);
        }

        let container = container_request
            .start()
            .await
            .map_err(|e| ContainerError::Start(e.to_string()))?;

        let host = container
            .get_host()
            .await
            .map_err(|e| ContainerError::Start(e.to_string()))?
            .to_string();

        let mut ports = Vec::new();
        for container_port in &config.exposed_ports {
            let host_port = container
                .get_host_port_ipv4(*container_port)
                .await
                .map_err(|e| ContainerError::Start(e.to_string()))?;
            ports.push((*container_port, host_port));
        }

        debug!(
            host = %host,
            ports = ?ports,
            "Generic container started"
        );

        Ok(Self {
            container,
            host,
            ports,
        })
    }

    /// Get the host address.
    pub fn host(&self) -> &str {
        &self.host
    }

    /// Get the mapped port for a container port.
    pub fn get_port(&self, container_port: u16) -> Option<u16> {
        self.ports
            .iter()
            .find(|(cp, _)| *cp == container_port)
            .map(|(_, hp)| *hp)
    }

    /// Get all port mappings.
    pub fn ports(&self) -> &[(u16, u16)] {
        &self.ports
    }
}

/// Errors that can occur when working with containers
#[derive(Debug, thiserror::Error)]
pub enum ContainerError {
    /// Container failed to start
    #[error("Container failed to start: {0}")]
    Start(String),

    /// Failed to connect to container
    #[error("Failed to connect to container: {0}")]
    Connection(String),

    /// Container health check failed
    #[error("Container health check failed: {0}")]
    HealthCheck(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn postgres_config_default() {
        let config = PostgresContainerConfig::default();
        assert_eq!(config.database, "pisovereign_test");
        assert_eq!(config.username, "test");
        assert_eq!(config.password, "test");
        assert_eq!(config.version, "16-alpine");
    }

    #[test]
    fn redis_config_default() {
        let config = RedisContainerConfig::default();
        assert_eq!(config.version, "7-alpine");
    }

    #[test]
    fn container_error_display() {
        let error = ContainerError::Start("test error".to_string());
        assert!(error.to_string().contains("test error"));
    }

    // Note: Integration tests that actually start containers should be in
    // a separate integration test file and marked with #[ignore] by default
    // since they require Docker to be running
}
