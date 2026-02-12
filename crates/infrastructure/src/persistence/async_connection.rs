//! Async database connection using sqlx
//!
//! Provides async database operations using sqlx with SQLite.
//! This is the primary database layer — all stores use this pool.
//! Migrations are managed via sqlx's `migrate!()` macro using SQL
//! files in the workspace `migrations/` directory.

use std::{path::Path, str::FromStr};

use sqlx::{
    SqlitePool,
    sqlite::{SqliteConnectOptions, SqlitePoolOptions},
};
use tracing::{debug, info, instrument, warn};

/// Error type for async database operations
#[derive(Debug, thiserror::Error)]
pub enum AsyncDatabaseError {
    #[error("Database error: {0}")]
    Sqlx(#[from] sqlx::Error),

    #[error("Migration error: {0}")]
    Migration(#[from] sqlx::migrate::MigrateError),

    #[error("Configuration error: {0}")]
    Config(String),
}

/// Configuration for async database connection
#[derive(Debug, Clone)]
pub struct AsyncDatabaseConfig {
    /// Database URL (e.g., "sqlite:data.db" or "sqlite::memory:")
    pub url: String,
    /// Maximum number of connections in the pool
    pub max_connections: u32,
    /// Minimum number of connections to keep open
    pub min_connections: u32,
    /// Enable WAL mode for better concurrency
    pub wal_mode: bool,
    /// Enable foreign keys
    pub foreign_keys: bool,
}

impl Default for AsyncDatabaseConfig {
    fn default() -> Self {
        Self {
            url: "sqlite:pisovereign.db".to_string(),
            max_connections: 5,
            min_connections: 1,
            wal_mode: true,
            foreign_keys: true,
        }
    }
}

impl AsyncDatabaseConfig {
    /// Create an in-memory database configuration for testing
    #[must_use]
    pub fn in_memory() -> Self {
        Self {
            url: "sqlite::memory:".to_string(),
            max_connections: 1, // Single connection for in-memory
            min_connections: 1,
            wal_mode: false, // Not supported for in-memory
            foreign_keys: true,
        }
    }

    /// Create a file-based database configuration
    #[must_use]
    pub fn file(path: impl AsRef<Path>) -> Self {
        let path_str = path.as_ref().display().to_string();
        Self {
            url: format!("sqlite:{path_str}"),
            ..Default::default()
        }
    }
}

/// Async database connection pool
#[derive(Debug, Clone)]
pub struct AsyncDatabase {
    pool: SqlitePool,
}

impl AsyncDatabase {
    /// Create a new async database connection pool
    #[instrument(skip_all, fields(url = %config.url))]
    pub async fn new(config: &AsyncDatabaseConfig) -> Result<Self, AsyncDatabaseError> {
        let options = SqliteConnectOptions::from_str(&config.url)?
            .create_if_missing(true)
            .foreign_keys(config.foreign_keys);

        let pool = SqlitePoolOptions::new()
            .max_connections(config.max_connections)
            .min_connections(config.min_connections)
            .connect_with(options)
            .await?;

        // Enable WAL mode if configured
        if config.wal_mode && !config.url.contains(":memory:") {
            sqlx::query("PRAGMA journal_mode=WAL")
                .execute(&pool)
                .await?;
            debug!("WAL mode enabled");
        }

        // Set busy timeout for concurrent access
        sqlx::query("PRAGMA busy_timeout=5000")
            .execute(&pool)
            .await?;

        // Set synchronous mode to NORMAL for WAL mode (good balance of safety and speed)
        if config.wal_mode && !config.url.contains(":memory:") {
            sqlx::query("PRAGMA synchronous=NORMAL")
                .execute(&pool)
                .await?;
        }

        info!(
            max_connections = config.max_connections,
            "Async database pool created"
        );

        Ok(Self { pool })
    }

    /// Create an in-memory database for testing
    pub async fn in_memory() -> Result<Self, AsyncDatabaseError> {
        Self::new(&AsyncDatabaseConfig::in_memory()).await
    }

    /// Get the underlying pool for raw queries
    #[must_use]
    pub fn pool(&self) -> &SqlitePool {
        &self.pool
    }

    /// Run database migrations using the workspace migration SQL files.
    ///
    /// Handles both fresh databases and existing databases that were previously
    /// managed by the legacy rusqlite migration system. For legacy databases,
    /// existing migrations are adopted (marked as already applied) before
    /// running any pending migrations.
    #[instrument(skip(self))]
    pub async fn migrate(&self) -> Result<(), AsyncDatabaseError> {
        let migrator = sqlx::migrate!("../../migrations");

        // Adopt existing databases that were managed by the legacy migration system
        self.adopt_legacy_database(&migrator).await?;

        // Run any pending migrations
        migrator.run(&self.pool).await?;

        info!("Database migrations completed");
        Ok(())
    }

    /// Detect and adopt databases that were previously managed by the legacy
    /// rusqlite migration system (tracked via `PRAGMA user_version`).
    ///
    /// This creates the `_sqlx_migrations` tracking table and marks all existing
    /// migrations as already applied, so sqlx won't attempt to re-run them.
    async fn adopt_legacy_database(
        &self,
        migrator: &sqlx::migrate::Migrator,
    ) -> Result<(), AsyncDatabaseError> {
        // Check if sqlx is already managing this database
        let has_sqlx_table: bool = sqlx::query_scalar(
            "SELECT COUNT(*) > 0 FROM sqlite_master \
             WHERE type='table' AND name='_sqlx_migrations'",
        )
        .fetch_one(&self.pool)
        .await
        .unwrap_or(false);

        if has_sqlx_table {
            return Ok(());
        }

        // Check the legacy version tracker
        let user_version: i64 = sqlx::query_scalar("SELECT * FROM pragma_user_version")
            .fetch_one(&self.pool)
            .await?;

        if user_version == 0 {
            // Fresh database — sqlx will handle everything from scratch
            return Ok(());
        }

        warn!(
            user_version,
            "Adopting legacy database for sqlx migration tracking"
        );

        // Create the sqlx migrations tracking table
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS _sqlx_migrations (
                version BIGINT PRIMARY KEY,
                description TEXT NOT NULL,
                installed_on TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
                success BOOLEAN NOT NULL,
                checksum BLOB NOT NULL,
                execution_time BIGINT NOT NULL DEFAULT 0
            )",
        )
        .execute(&self.pool)
        .await?;

        // Mark all migrations up to user_version as already applied
        let mut adopted: i64 = 0;
        for migration in migrator.migrations.as_ref() {
            if migration.version <= user_version {
                sqlx::query(
                    "INSERT OR IGNORE INTO _sqlx_migrations \
                     (version, description, success, checksum, execution_time) \
                     VALUES ($1, $2, true, $3, 0)",
                )
                .bind(migration.version)
                .bind(migration.description.as_ref())
                .bind(migration.checksum.as_ref())
                .execute(&self.pool)
                .await?;
                adopted += 1;
            }
        }

        info!(adopted, "Legacy migrations adopted successfully");
        Ok(())
    }

    /// Close all connections in the pool
    pub async fn close(&self) {
        self.pool.close().await;
        debug!("Database pool closed");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn create_in_memory_database() {
        let db = AsyncDatabase::in_memory().await.unwrap();
        let _ = db.pool();
    }

    #[tokio::test]
    async fn run_migrations() {
        let db = AsyncDatabase::in_memory().await.unwrap();
        db.migrate().await.unwrap();

        // Verify core tables exist
        let result: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM conversations")
            .fetch_one(db.pool())
            .await
            .unwrap();
        assert_eq!(result.0, 0);
    }

    #[tokio::test]
    async fn all_migration_tables_created() {
        let db = AsyncDatabase::in_memory().await.unwrap();
        db.migrate().await.unwrap();

        // Verify all tables from all 9 migrations exist
        let tables = [
            "conversations",
            "messages",
            "approval_requests",
            "audit_log",
            "user_profiles",
            "email_drafts",
            "retry_queue",
            "dead_letter_queue",
            "memories",
            "memory_embeddings",
            "reminders",
        ];

        for table in &tables {
            let result: (i64,) = sqlx::query_as(&format!(
                "SELECT COUNT(*) FROM sqlite_master \
                 WHERE type='table' AND name='{table}'"
            ))
            .fetch_one(db.pool())
            .await
            .unwrap();
            assert_eq!(result.0, 1, "Table {table} should exist after migrations");
        }
    }

    #[tokio::test]
    async fn migrations_are_idempotent() {
        let db = AsyncDatabase::in_memory().await.unwrap();
        // Running twice should not fail
        db.migrate().await.unwrap();
        db.migrate().await.unwrap();
    }

    #[tokio::test]
    async fn wal_mode_for_file_database() {
        let temp_dir = std::env::temp_dir();
        let db_path = temp_dir.join("test_wal_async.db");

        let config = AsyncDatabaseConfig::file(&db_path);
        let db = AsyncDatabase::new(&config).await.unwrap();
        db.migrate().await.unwrap();

        let result: (String,) = sqlx::query_as("PRAGMA journal_mode")
            .fetch_one(db.pool())
            .await
            .unwrap();
        assert_eq!(result.0.to_lowercase(), "wal");

        db.close().await;
        let _ = std::fs::remove_file(&db_path);
        let _ = std::fs::remove_file(format!("{}-wal", db_path.display()));
        let _ = std::fs::remove_file(format!("{}-shm", db_path.display()));
    }

    #[tokio::test]
    async fn default_config() {
        let config = AsyncDatabaseConfig::default();
        assert_eq!(config.max_connections, 5);
        assert!(config.wal_mode);
        assert!(config.foreign_keys);
    }

    #[tokio::test]
    async fn adopt_legacy_database_noop_on_fresh() {
        let db = AsyncDatabase::in_memory().await.unwrap();
        let migrator = sqlx::migrate!("../../migrations");

        // On fresh database, adoption should be a no-op
        db.adopt_legacy_database(&migrator).await.unwrap();

        // sqlx table should NOT exist yet (fresh db, no adoption needed)
        let has_table: bool = sqlx::query_scalar(
            "SELECT COUNT(*) > 0 FROM sqlite_master \
             WHERE type='table' AND name='_sqlx_migrations'",
        )
        .fetch_one(db.pool())
        .await
        .unwrap();
        assert!(!has_table);
    }
}
