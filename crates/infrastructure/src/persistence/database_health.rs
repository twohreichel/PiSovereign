//! SQLite database health adapter
//!
//! Implements the `DatabaseHealthPort` for SQLite databases using sqlx.

use application::error::ApplicationError;
use application::ports::{DatabaseHealth, DatabaseHealthPort};
use async_trait::async_trait;
use sqlx::SqlitePool;
use tracing::{debug, instrument, warn};

/// SQLite database health adapter
#[derive(Debug, Clone)]
pub struct SqliteDatabaseHealth {
    pool: SqlitePool,
}

impl SqliteDatabaseHealth {
    /// Create a new database health adapter with the given pool
    #[must_use]
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl DatabaseHealthPort for SqliteDatabaseHealth {
    #[instrument(skip(self))]
    async fn is_available(&self) -> bool {
        let result: Result<(i32,), _> =
            sqlx::query_as("SELECT 1").fetch_one(&self.pool).await;

        match result {
            Ok(_) => {
                debug!("Database health check passed");
                true
            },
            Err(e) => {
                warn!(error = %e, "Database health check failed");
                false
            },
        }
    }

    #[instrument(skip(self))]
    async fn check_health(&self) -> Result<DatabaseHealth, ApplicationError> {
        let start = std::time::Instant::now();

        // Execute health check query
        sqlx::query("SELECT 1")
            .execute(&self.pool)
            .await
            .map_err(|e| ApplicationError::Internal(format!("Health check query failed: {e}")))?;

        // Get SQLite version
        let (version,): (String,) = sqlx::query_as("SELECT sqlite_version()")
            .fetch_one(&self.pool)
            .await
            .map_err(|e| ApplicationError::Internal(format!("Version query failed: {e}")))?;

        // Pool size from sqlx options
        let pool_size = self.pool.size();

        // SAFETY: Response time is always small (health check is fast),
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
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::persistence::async_connection::AsyncDatabase;

    async fn setup() -> (AsyncDatabase, SqliteDatabaseHealth) {
        let db = AsyncDatabase::in_memory().await.unwrap();
        db.migrate().await.unwrap();
        let health = SqliteDatabaseHealth::new(db.pool().clone());
        (db, health)
    }

    #[tokio::test]
    async fn is_available_returns_true_for_healthy_db() {
        let (_db, health) = setup().await;
        assert!(health.is_available().await);
    }

    #[tokio::test]
    async fn check_health_returns_version_info() {
        let (_db, health) = setup().await;

        let result = health.check_health().await;
        assert!(result.is_ok());

        let db_health = result.unwrap();
        assert!(db_health.reachable);
        assert!(db_health.version.is_some());
        assert!(db_health.version.unwrap().contains("SQLite"));
    }

    #[tokio::test]
    async fn check_health_includes_pool_size() {
        let (_db, health) = setup().await;
        let result = health.check_health().await.unwrap();
        assert!(result.pool_size.is_some());
    }

    #[tokio::test]
    async fn check_health_includes_response_time() {
        let (_db, health) = setup().await;
        let result = health.check_health().await.unwrap();
        assert!(result.response_time_ms.is_some());
    }

    #[test]
    fn debug_impl_works() {
        // SqliteDatabaseHealth derives Debug, so this should just work
        // We can't easily create one without a pool in a sync test,
        // so just verify the type is Debug at compile time.
        fn assert_debug<T: std::fmt::Debug>() {}
        assert_debug::<SqliteDatabaseHealth>();
    }
}
