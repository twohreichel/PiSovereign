//! Database health check port
//!
//! Defines the interface for database connectivity and health checks.

use async_trait::async_trait;
#[cfg(test)]
use mockall::automock;

use crate::error::ApplicationError;

/// Database health information
#[derive(Debug, Clone)]
pub struct DatabaseHealth {
    /// Whether the database is reachable and responding
    pub reachable: bool,
    /// Database version or identifier (if available)
    pub version: Option<String>,
    /// Connection pool size (current active connections)
    pub pool_size: Option<u32>,
    /// Response time of the health check in milliseconds
    pub response_time_ms: Option<u64>,
}

impl DatabaseHealth {
    /// Create a healthy database status
    #[must_use]
    pub const fn healthy() -> Self {
        Self {
            reachable: true,
            version: None,
            pool_size: None,
            response_time_ms: None,
        }
    }

    /// Create a healthy status with version info
    #[must_use]
    pub fn healthy_with_version(version: impl Into<String>) -> Self {
        Self {
            reachable: true,
            version: Some(version.into()),
            pool_size: None,
            response_time_ms: None,
        }
    }

    /// Create an unhealthy status
    #[must_use]
    pub const fn unhealthy() -> Self {
        Self {
            reachable: false,
            version: None,
            pool_size: None,
            response_time_ms: None,
        }
    }

    /// Add response time to the health status
    #[must_use]
    pub const fn with_response_time(mut self, ms: u64) -> Self {
        self.response_time_ms = Some(ms);
        self
    }

    /// Add pool size to the health status
    #[must_use]
    pub const fn with_pool_size(mut self, size: u32) -> Self {
        self.pool_size = Some(size);
        self
    }
}

/// Port for database health checking operations
///
/// This port allows the application layer to check database connectivity
/// without coupling to specific database implementations.
#[cfg_attr(test, automock)]
#[async_trait]
pub trait DatabaseHealthPort: Send + Sync {
    /// Check if the database is available and responding
    ///
    /// Performs a lightweight query (e.g., `SELECT 1`) to verify connectivity.
    async fn is_available(&self) -> bool;

    /// Get detailed health information about the database
    ///
    /// Returns comprehensive health data including version, pool state, etc.
    async fn check_health(&self) -> Result<DatabaseHealth, ApplicationError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn _assert_object_safe(_: &dyn DatabaseHealthPort) {}

    #[test]
    fn trait_is_send_sync() {
        fn assert_send_sync<T: Send + Sync + ?Sized>() {}
        assert_send_sync::<dyn DatabaseHealthPort>();
    }

    #[test]
    fn database_health_healthy() {
        let health = DatabaseHealth::healthy();
        assert!(health.reachable);
        assert!(health.version.is_none());
    }

    #[test]
    fn database_health_with_version() {
        let health = DatabaseHealth::healthy_with_version("SQLite 3.42.0");
        assert!(health.reachable);
        assert_eq!(health.version.as_deref(), Some("SQLite 3.42.0"));
    }

    #[test]
    fn database_health_unhealthy() {
        let health = DatabaseHealth::unhealthy();
        assert!(!health.reachable);
    }

    #[test]
    fn database_health_with_response_time() {
        let health = DatabaseHealth::healthy().with_response_time(42);
        assert_eq!(health.response_time_ms, Some(42));
    }

    #[test]
    fn database_health_with_pool_size() {
        let health = DatabaseHealth::healthy().with_pool_size(5);
        assert_eq!(health.pool_size, Some(5));
    }
}
