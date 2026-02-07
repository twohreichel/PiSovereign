//! SQLite database health adapter
//!
//! Implements the `DatabaseHealthPort` for SQLite databases using the connection pool.

use std::sync::Arc;

use application::error::ApplicationError;
use application::ports::{DatabaseHealth, DatabaseHealthPort};
use async_trait::async_trait;
use tracing::{debug, instrument, warn};

use super::ConnectionPool;

/// SQLite database health adapter
///
/// Provides health checking for SQLite databases using the r2d2 connection pool.
pub struct SqliteDatabaseHealth {
    pool: Arc<ConnectionPool>,
}

impl std::fmt::Debug for SqliteDatabaseHealth {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SqliteDatabaseHealth")
            .field("pool", &"<ConnectionPool>")
            .finish()
    }
}

impl SqliteDatabaseHealth {
    /// Create a new database health adapter with the given connection pool
    #[must_use]
    pub const fn new(pool: Arc<ConnectionPool>) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl DatabaseHealthPort for SqliteDatabaseHealth {
    #[instrument(skip(self))]
    async fn is_available(&self) -> bool {
        let pool = Arc::clone(&self.pool);
        let result = tokio::task::spawn_blocking(move || {
            pool.get()
                .ok()
                .and_then(|conn| {
                    conn.query_row("SELECT 1", [], |row| row.get::<_, i32>(0))
                        .ok()
                })
                .is_some()
        })
        .await;

        match result {
            Ok(available) => {
                if available {
                    debug!("Database health check passed");
                } else {
                    warn!("Database health check failed: unable to execute query");
                }
                available
            },
            Err(e) => {
                warn!(error = %e, "Database health check failed: task panicked");
                false
            },
        }
    }

    #[instrument(skip(self))]
    async fn check_health(&self) -> Result<DatabaseHealth, ApplicationError> {
        let pool = Arc::clone(&self.pool);
        let start = std::time::Instant::now();

        let result = tokio::task::spawn_blocking(move || {
            // Try to get a connection and run a simple query
            let conn = pool.get().map_err(|e| {
                ApplicationError::Internal(format!("Failed to get database connection: {e}"))
            })?;

            // Execute health check query
            let _: i32 = conn
                .query_row("SELECT 1", [], |row| row.get(0))
                .map_err(|e| {
                    ApplicationError::Internal(format!("Health check query failed: {e}"))
                })?;

            // Get SQLite version
            let version: String = conn
                .query_row("SELECT sqlite_version()", [], |row| row.get(0))
                .unwrap_or_else(|_| "unknown".to_string());

            // Get pool state
            let state = pool.state();
            let pool_size = state.connections;

            Ok::<_, ApplicationError>((version, pool_size))
        })
        .await
        .map_err(|e| {
            ApplicationError::Internal(format!("Database health check task failed: {e}"))
        })?;

        match result {
            Ok((version, pool_size)) => {
                // SAFETY: Response time is always small (health check timeout is seconds),
                // so milliseconds will fit in u64.
                #[allow(clippy::cast_possible_truncation)]
                let response_time_ms = start.elapsed().as_millis() as u64;

                debug!(
                    version = %version,
                    pool_size = pool_size,
                    response_time_ms = response_time_ms,
                    "Database health check passed"
                );

                Ok(
                    DatabaseHealth::healthy_with_version(format!("SQLite {version}"))
                        .with_pool_size(pool_size)
                        .with_response_time(response_time_ms),
                )
            },
            Err(e) => {
                warn!(error = %e, "Database health check failed");
                Err(e)
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{config::DatabaseConfig, persistence::create_pool};

    fn create_test_pool() -> Arc<ConnectionPool> {
        let config = DatabaseConfig {
            path: ":memory:".to_string(),
            max_connections: 2,
            run_migrations: true,
        };
        Arc::new(create_pool(&config).expect("Failed to create pool"))
    }

    #[tokio::test]
    async fn is_available_returns_true_for_healthy_db() {
        let pool = create_test_pool();
        let health = SqliteDatabaseHealth::new(pool);

        assert!(health.is_available().await);
    }

    #[tokio::test]
    async fn check_health_returns_version_info() {
        let pool = create_test_pool();
        let health = SqliteDatabaseHealth::new(pool);

        let result = health.check_health().await;
        assert!(result.is_ok());

        let db_health = result.unwrap();
        assert!(db_health.reachable);
        assert!(db_health.version.is_some());
        assert!(db_health.version.unwrap().contains("SQLite"));
    }

    #[tokio::test]
    async fn check_health_includes_pool_size() {
        let pool = create_test_pool();
        let health = SqliteDatabaseHealth::new(pool);

        let result = health.check_health().await.unwrap();
        assert!(result.pool_size.is_some());
    }

    #[tokio::test]
    async fn check_health_includes_response_time() {
        let pool = create_test_pool();
        let health = SqliteDatabaseHealth::new(pool);

        let result = health.check_health().await.unwrap();
        assert!(result.response_time_ms.is_some());
    }

    #[test]
    fn debug_impl_works() {
        let pool = create_test_pool();
        let health = SqliteDatabaseHealth::new(pool);
        let debug_str = format!("{health:?}");
        assert!(debug_str.contains("SqliteDatabaseHealth"));
    }
}
