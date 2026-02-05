//! Database migrations
//!
//! Manages database schema versioning and migrations.

use rusqlite::Connection;
use tracing::{debug, info};

use super::connection::DatabaseError;

/// Current schema version
const SCHEMA_VERSION: i32 = 2;

/// Run all pending migrations
pub fn run_migrations(conn: &Connection) -> Result<(), DatabaseError> {
    let current_version = get_schema_version(conn)?;

    if current_version < SCHEMA_VERSION {
        info!(
            from_version = current_version,
            to_version = SCHEMA_VERSION,
            "Running database migrations"
        );

        if current_version < 1 {
            migrate_v1(conn)?;
        }

        if current_version < 2 {
            migrate_v2(conn)?;
        }

        set_schema_version(conn, SCHEMA_VERSION)?;
        info!(version = SCHEMA_VERSION, "Database migrations complete");
    } else {
        debug!(version = current_version, "Database schema is up to date");
    }

    Ok(())
}

/// Get current schema version
fn get_schema_version(conn: &Connection) -> Result<i32, DatabaseError> {
    // Create schema_version table if it doesn't exist
    conn.execute(
        "CREATE TABLE IF NOT EXISTS schema_version (
            version INTEGER PRIMARY KEY
        )",
        [],
    )?;

    let version: i32 = conn
        .query_row(
            "SELECT COALESCE(MAX(version), 0) FROM schema_version",
            [],
            |row| row.get(0),
        )
        .unwrap_or(0);

    Ok(version)
}

/// Set schema version
fn set_schema_version(conn: &Connection, version: i32) -> Result<(), DatabaseError> {
    conn.execute("DELETE FROM schema_version", [])?;
    conn.execute(
        "INSERT INTO schema_version (version) VALUES (?1)",
        [version],
    )?;
    Ok(())
}

/// Migration to version 1: Initial schema
fn migrate_v1(conn: &Connection) -> Result<(), DatabaseError> {
    debug!("Applying migration v1: Initial schema");

    conn.execute_batch(
        "
        -- Conversations table
        CREATE TABLE IF NOT EXISTS conversations (
            id TEXT PRIMARY KEY,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL,
            title TEXT,
            system_prompt TEXT
        );

        -- Messages table
        CREATE TABLE IF NOT EXISTS messages (
            id TEXT PRIMARY KEY,
            conversation_id TEXT NOT NULL,
            role TEXT NOT NULL CHECK(role IN ('user', 'assistant', 'system')),
            content TEXT NOT NULL,
            created_at TEXT NOT NULL,
            tokens INTEGER,
            model TEXT,
            FOREIGN KEY (conversation_id) REFERENCES conversations(id) ON DELETE CASCADE
        );

        -- Approval requests table
        CREATE TABLE IF NOT EXISTS approval_requests (
            id TEXT PRIMARY KEY,
            user_id TEXT NOT NULL,
            command TEXT NOT NULL,
            description TEXT NOT NULL,
            status TEXT NOT NULL DEFAULT 'pending' CHECK(status IN ('pending', 'approved', 'denied', 'expired', 'cancelled')),
            created_at TEXT NOT NULL,
            expires_at TEXT NOT NULL,
            updated_at TEXT NOT NULL,
            reason TEXT
        );

        -- Audit log table
        CREATE TABLE IF NOT EXISTS audit_log (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            timestamp TEXT NOT NULL,
            event_type TEXT NOT NULL,
            actor TEXT,
            resource_type TEXT,
            resource_id TEXT,
            action TEXT NOT NULL,
            details TEXT,
            ip_address TEXT,
            success INTEGER NOT NULL DEFAULT 1
        );

        -- Indexes
        CREATE INDEX IF NOT EXISTS idx_messages_conversation ON messages(conversation_id);
        CREATE INDEX IF NOT EXISTS idx_messages_created ON messages(created_at);
        CREATE INDEX IF NOT EXISTS idx_approvals_status ON approval_requests(status);
        CREATE INDEX IF NOT EXISTS idx_approvals_user ON approval_requests(user_id);
        CREATE INDEX IF NOT EXISTS idx_approvals_expires ON approval_requests(expires_at);
        CREATE INDEX IF NOT EXISTS idx_audit_timestamp ON audit_log(timestamp);
        CREATE INDEX IF NOT EXISTS idx_audit_event_type ON audit_log(event_type);
        ",
    )?;

    Ok(())
}

/// Migration to version 2: User profiles
fn migrate_v2(conn: &Connection) -> Result<(), DatabaseError> {
    debug!("Applying migration v2: User profiles");

    conn.execute_batch(
        "
        -- User profiles table
        CREATE TABLE IF NOT EXISTS user_profiles (
            user_id TEXT PRIMARY KEY,
            latitude REAL,
            longitude REAL,
            timezone TEXT NOT NULL DEFAULT 'UTC',
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL
        );

        -- Indexes
        CREATE INDEX IF NOT EXISTS idx_user_profiles_timezone ON user_profiles(timezone);
        ",
    )?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_connection() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "
            PRAGMA foreign_keys = ON;
            PRAGMA journal_mode = WAL;
            ",
        )
        .unwrap();
        conn
    }

    #[test]
    fn run_migrations_creates_tables() {
        let conn = create_test_connection();
        run_migrations(&conn).unwrap();

        // Verify tables exist
        let tables: Vec<String> = conn
            .prepare("SELECT name FROM sqlite_master WHERE type='table' ORDER BY name")
            .unwrap()
            .query_map([], |row| row.get(0))
            .unwrap()
            .filter_map(Result::ok)
            .collect();

        assert!(tables.contains(&"conversations".to_string()));
        assert!(tables.contains(&"messages".to_string()));
        assert!(tables.contains(&"approval_requests".to_string()));
        assert!(tables.contains(&"audit_log".to_string()));
        assert!(tables.contains(&"user_profiles".to_string()));
    }

    #[test]
    fn migrations_are_idempotent() {
        let conn = create_test_connection();
        run_migrations(&conn).unwrap();
        run_migrations(&conn).unwrap(); // Should not fail
    }

    #[test]
    fn schema_version_tracked() {
        let conn = create_test_connection();
        run_migrations(&conn).unwrap();

        let version = get_schema_version(&conn).unwrap();
        assert_eq!(version, SCHEMA_VERSION);
    }

    #[test]
    fn messages_table_has_role_constraint() {
        let conn = create_test_connection();
        run_migrations(&conn).unwrap();

        // Insert a conversation first
        conn.execute(
            "INSERT INTO conversations (id, created_at, updated_at) VALUES ('c1', '2024-01-01', '2024-01-01')",
            [],
        )
        .unwrap();

        // Valid role should work
        let result = conn.execute(
            "INSERT INTO messages (id, conversation_id, role, content, created_at) VALUES ('m1', 'c1', 'user', 'hi', '2024-01-01')",
            [],
        );
        assert!(result.is_ok());

        // Invalid role should fail
        let result = conn.execute(
            "INSERT INTO messages (id, conversation_id, role, content, created_at) VALUES ('m2', 'c1', 'invalid', 'hi', '2024-01-01')",
            [],
        );
        assert!(result.is_err());
    }

    #[test]
    fn cascade_delete_messages() {
        let conn = create_test_connection();
        run_migrations(&conn).unwrap();

        // Insert conversation and message
        conn.execute(
            "INSERT INTO conversations (id, created_at, updated_at) VALUES ('c1', '2024-01-01', '2024-01-01')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO messages (id, conversation_id, role, content, created_at) VALUES ('m1', 'c1', 'user', 'hi', '2024-01-01')",
            [],
        )
        .unwrap();

        // Delete conversation
        conn.execute("DELETE FROM conversations WHERE id = 'c1'", [])
            .unwrap();

        // Message should be deleted
        let count: i32 = conn
            .query_row(
                "SELECT COUNT(*) FROM messages WHERE conversation_id = 'c1'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 0);
    }

    #[test]
    fn user_profiles_table_schema() {
        let conn = create_test_connection();
        run_migrations(&conn).unwrap();

        // Insert a user profile
        conn.execute(
            "INSERT INTO user_profiles (user_id, latitude, longitude, timezone, created_at, updated_at)
             VALUES ('user1', 52.52, 13.405, 'Europe/Berlin', '2024-01-01', '2024-01-01')",
            [],
        )
        .unwrap();

        // Verify we can query it back
        let (lat, lon, tz): (f64, f64, String) = conn
            .query_row(
                "SELECT latitude, longitude, timezone FROM user_profiles WHERE user_id = 'user1'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap();

        assert!((lat - 52.52).abs() < 0.001);
        assert!((lon - 13.405).abs() < 0.001);
        assert_eq!(tz, "Europe/Berlin");
    }

    #[test]
    fn user_profiles_allows_null_location() {
        let conn = create_test_connection();
        run_migrations(&conn).unwrap();

        // Insert a user profile without location
        conn.execute(
            "INSERT INTO user_profiles (user_id, timezone, created_at, updated_at)
             VALUES ('user2', 'UTC', '2024-01-01', '2024-01-01')",
            [],
        )
        .unwrap();

        // Verify NULL values
        let (lat, lon): (Option<f64>, Option<f64>) = conn
            .query_row(
                "SELECT latitude, longitude FROM user_profiles WHERE user_id = 'user2'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();

        assert!(lat.is_none());
        assert!(lon.is_none());
    }
}
