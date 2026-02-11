//! Async database connection using sqlx
//!
//! Provides async database operations using sqlx with SQLite.
//! This is the preferred implementation for new code as it avoids
//! blocking the async runtime.

use std::{path::Path, str::FromStr};

use sqlx::{
    SqlitePool,
    sqlite::{SqliteConnectOptions, SqlitePoolOptions},
};
use tracing::{debug, info, instrument};

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

        // WAL mode needs to be set via pragma after connection
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
    pub const fn pool(&self) -> &SqlitePool {
        &self.pool
    }

    /// Run database migrations
    #[instrument(skip(self))]
    pub async fn migrate(&self) -> Result<(), AsyncDatabaseError> {
        // Embedded migrations will be compiled in
        // For now, we run the same schema as the r2d2 version
        self.run_initial_schema().await?;
        info!("Database migrations completed");
        Ok(())
    }

    /// Run the initial database schema
    async fn run_initial_schema(&self) -> Result<(), AsyncDatabaseError> {
        // Schema version tracking
        sqlx::query(
            r"
            CREATE TABLE IF NOT EXISTS schema_version (
                version INTEGER PRIMARY KEY,
                applied_at TEXT NOT NULL DEFAULT (datetime('now'))
            )
            ",
        )
        .execute(&self.pool)
        .await?;

        // Conversations table
        sqlx::query(
            r"
            CREATE TABLE IF NOT EXISTS conversations (
                id TEXT PRIMARY KEY,
                title TEXT,
                system_prompt TEXT,
                metadata TEXT,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                source TEXT NOT NULL DEFAULT 'http',
                phone_number TEXT
            )
            ",
        )
        .execute(&self.pool)
        .await?;

        // Messages table
        sqlx::query(
            r"
            CREATE TABLE IF NOT EXISTS messages (
                id TEXT PRIMARY KEY,
                conversation_id TEXT NOT NULL REFERENCES conversations(id) ON DELETE CASCADE,
                role TEXT NOT NULL CHECK (role IN ('user', 'assistant', 'system')),
                content TEXT NOT NULL,
                created_at TEXT NOT NULL,
                metadata TEXT
            )
            ",
        )
        .execute(&self.pool)
        .await?;

        // Message index for conversation queries
        sqlx::query(
            r"
            CREATE INDEX IF NOT EXISTS idx_messages_conversation 
            ON messages(conversation_id, created_at)
            ",
        )
        .execute(&self.pool)
        .await?;

        // Approval queue table
        sqlx::query(
            r"
            CREATE TABLE IF NOT EXISTS approval_queue (
                id TEXT PRIMARY KEY,
                action_type TEXT NOT NULL,
                payload TEXT NOT NULL,
                user_id TEXT NOT NULL,
                status TEXT NOT NULL DEFAULT 'pending',
                created_at TEXT NOT NULL,
                expires_at TEXT,
                resolved_at TEXT,
                resolved_by TEXT,
                rejection_reason TEXT
            )
            ",
        )
        .execute(&self.pool)
        .await?;

        // Audit log table
        sqlx::query(
            r"
            CREATE TABLE IF NOT EXISTS audit_log (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                timestamp TEXT NOT NULL DEFAULT (datetime('now')),
                user_id TEXT,
                action TEXT NOT NULL,
                details TEXT,
                success INTEGER NOT NULL,
                error_message TEXT
            )
            ",
        )
        .execute(&self.pool)
        .await?;

        // Audit log index for time-based queries
        sqlx::query(
            r"
            CREATE INDEX IF NOT EXISTS idx_audit_timestamp 
            ON audit_log(timestamp DESC)
            ",
        )
        .execute(&self.pool)
        .await?;

        debug!("Initial schema created");
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
        // Pool may be lazy, just verify it was created
        let _ = db.pool();
    }

    #[tokio::test]
    async fn run_migrations() {
        let db = AsyncDatabase::in_memory().await.unwrap();
        db.migrate().await.unwrap();

        // Verify tables exist
        let result: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM conversations")
            .fetch_one(db.pool())
            .await
            .unwrap();
        assert_eq!(result.0, 0);
    }

    #[tokio::test]
    async fn wal_mode_for_file_database() {
        let temp_dir = std::env::temp_dir();
        let db_path = temp_dir.join("test_wal.db");

        let config = AsyncDatabaseConfig::file(&db_path);
        let db = AsyncDatabase::new(&config).await.unwrap();
        db.migrate().await.unwrap();

        // Check WAL mode is enabled
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
}
