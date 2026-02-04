//! Database connection management
//!
//! Provides SQLite connection pooling via r2d2.

use std::path::Path;

use r2d2::{Pool, PooledConnection};
use r2d2_sqlite::SqliteConnectionManager;
use rusqlite::Connection;
use thiserror::Error;
use tracing::{debug, info};

use crate::config::DatabaseConfig;

/// Database errors
#[derive(Debug, Error)]
pub enum DatabaseError {
    #[error("Connection pool error: {0}")]
    Pool(#[from] r2d2::Error),

    #[error("SQLite error: {0}")]
    Sqlite(#[from] rusqlite::Error),

    #[error("Migration error: {0}")]
    Migration(String),

    #[error("Database not found: {0}")]
    NotFound(String),
}

/// SQLite connection pool type alias
pub type ConnectionPool = Pool<SqliteConnectionManager>;

/// Pooled connection type alias
pub type PooledConn = PooledConnection<SqliteConnectionManager>;

/// Create a new connection pool
pub fn create_pool(config: &DatabaseConfig) -> Result<ConnectionPool, DatabaseError> {
    info!(path = %config.path, max_connections = config.max_connections, "Creating database connection pool");

    let manager = if config.path == ":memory:" {
        SqliteConnectionManager::memory()
    } else {
        // Create parent directories if they don't exist
        if let Some(parent) = Path::new(&config.path).parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent).map_err(|e| {
                    DatabaseError::Migration(format!("Failed to create database directory: {e}"))
                })?;
            }
        }
        SqliteConnectionManager::file(&config.path)
    };

    let pool = Pool::builder()
        .max_size(config.max_connections)
        .build(manager)?;

    // Initialize the database
    {
        let conn = pool.get()?;
        initialize_database(&conn)?;
    }

    if config.run_migrations {
        let conn = pool.get()?;
        crate::persistence::migrations::run_migrations(&conn)?;
    }

    debug!("Database connection pool created successfully");
    Ok(pool)
}

/// Initialize database with basic settings
fn initialize_database(conn: &Connection) -> Result<(), DatabaseError> {
    // Enable foreign keys
    conn.execute_batch(
        "
        PRAGMA foreign_keys = ON;
        PRAGMA journal_mode = WAL;
        PRAGMA synchronous = NORMAL;
        PRAGMA busy_timeout = 5000;
        ",
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn memory_config() -> DatabaseConfig {
        DatabaseConfig {
            path: ":memory:".to_string(),
            max_connections: 1,
            run_migrations: true,
        }
    }

    #[test]
    fn create_in_memory_pool() {
        let pool = create_pool(&memory_config());
        assert!(pool.is_ok());
    }

    #[test]
    fn pool_connection_works() {
        let pool = create_pool(&memory_config()).unwrap();
        let conn = pool.get();
        assert!(conn.is_ok());
    }

    #[test]
    fn database_config_default() {
        let config = DatabaseConfig::default();
        assert_eq!(config.path, "pisovereign.db");
        assert_eq!(config.max_connections, 5);
        assert!(config.run_migrations);
    }

    #[test]
    fn database_config_debug() {
        let config = DatabaseConfig::default();
        let debug = format!("{config:?}");
        assert!(debug.contains("DatabaseConfig"));
    }

    #[test]
    fn database_error_display() {
        let err = DatabaseError::NotFound("test.db".to_string());
        assert!(err.to_string().contains("test.db"));
    }
}
