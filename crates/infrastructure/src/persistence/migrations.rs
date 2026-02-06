//! Database migrations
//!
//! Manages database schema versioning and migrations.
//!
//! ## Migration Files
//!
//! SQL migration files are stored in the `/migrations` directory at the project root.
//! These files serve as documentation and can be used for manual database setup.
//! The actual migration code is embedded in this module for runtime execution.
//!
//! ## Rollback Strategy
//!
//! Rollbacks are manual - if a migration fails:
//! 1. Check the error message for details
//! 2. Fix the underlying issue
//! 3. Manually repair the database if needed
//! 4. Re-run migrations
//!
//! ## Adding New Migrations
//!
//! 1. Create a new SQL file: `migrations/VXXX__description.sql`
//! 2. Increment `SCHEMA_VERSION` constant
//! 3. Add a new `migrate_vX` function
//! 4. Update `run_migrations` to call the new function

use rusqlite::Connection;
use tracing::{debug, error, info};

use super::connection::DatabaseError;

/// Current schema version
const SCHEMA_VERSION: i32 = 5;

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
            if let Err(e) = migrate_v1(conn) {
                error!(
                    version = 1,
                    error = %e,
                    "Migration V001 (initial schema) failed. Check migrations/V001__initial_schema.sql for the expected schema."
                );
                return Err(e);
            }
        }

        if current_version < 2 {
            if let Err(e) = migrate_v2(conn) {
                error!(
                    version = 2,
                    error = %e,
                    "Migration V002 (user profiles) failed. Check migrations/V002__user_profiles.sql for the expected schema."
                );
                return Err(e);
            }
        }

        if current_version < 3 {
            if let Err(e) = migrate_v3(conn) {
                error!(
                    version = 3,
                    error = %e,
                    "Migration V003 (email drafts) failed. Check migrations/V003__email_drafts.sql for the expected schema."
                );
                return Err(e);
            }
        }

        if current_version < 4 {
            if let Err(e) = migrate_v4(conn) {
                error!(
                    version = 4,
                    error = %e,
                    "Migration V004 (message sequence) failed. Check migrations/V004__message_sequence.sql for the expected schema."
                );
                return Err(e);
            }
        }

        if current_version < 5 {
            if let Err(e) = migrate_v5(conn) {
                error!(
                    version = 5,
                    error = %e,
                    "Migration V005 (audit request_id) failed. Check migrations/V005__audit_request_id.sql for the expected schema."
                );
                return Err(e);
            }
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
/// See: migrations/V001__initial_schema.sql
fn migrate_v1(conn: &Connection) -> Result<(), DatabaseError> {
    debug!("Applying migration V001: Initial schema");

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
/// See: migrations/V002__user_profiles.sql
fn migrate_v2(conn: &Connection) -> Result<(), DatabaseError> {
    debug!("Applying migration V002: User profiles");

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

/// Migration to version 3: Email drafts
/// See: migrations/V003__email_drafts.sql
fn migrate_v3(conn: &Connection) -> Result<(), DatabaseError> {
    debug!("Applying migration V003: Email drafts");

    conn.execute_batch(
        "
        -- Email drafts table
        CREATE TABLE IF NOT EXISTS email_drafts (
            id TEXT PRIMARY KEY,
            user_id TEXT NOT NULL,
            to_address TEXT NOT NULL,
            cc TEXT,
            subject TEXT NOT NULL,
            body TEXT NOT NULL,
            created_at TEXT NOT NULL,
            expires_at TEXT NOT NULL
        );

        -- Indexes for efficient lookups
        CREATE INDEX IF NOT EXISTS idx_email_drafts_user_id ON email_drafts(user_id);
        CREATE INDEX IF NOT EXISTS idx_email_drafts_expires_at ON email_drafts(expires_at);
        ",
    )?;

    Ok(())
}

/// Migration to version 4: Message sequence numbers
/// See: migrations/V004__message_sequence.sql
fn migrate_v4(conn: &Connection) -> Result<(), DatabaseError> {
    debug!("Applying migration V004: Message sequence numbers");

    // Add sequence_number column with default 0 for existing rows
    conn.execute(
        "ALTER TABLE messages ADD COLUMN sequence_number INTEGER NOT NULL DEFAULT 0",
        [],
    )?;

    // Create index for efficient ordering queries
    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_messages_sequence ON messages(conversation_id, sequence_number)",
        [],
    )?;

    // Update existing messages to have sequence numbers based on creation order
    // Use a subquery to assign sequential numbers per conversation
    conn.execute_batch(
        "
        UPDATE messages
        SET sequence_number = (
            SELECT COUNT(*) + 1
            FROM messages m2
            WHERE m2.conversation_id = messages.conversation_id
            AND (m2.created_at < messages.created_at
                 OR (m2.created_at = messages.created_at AND m2.id < messages.id))
        );
        ",
    )?;

    Ok(())
}

/// Migration to version 5: Add request_id to audit_log for tracing
/// See: migrations/V005__audit_request_id.sql
fn migrate_v5(conn: &Connection) -> Result<(), DatabaseError> {
    debug!("Applying migration V005: Audit log request_id");

    // Add request_id column (nullable for historical entries)
    conn.execute("ALTER TABLE audit_log ADD COLUMN request_id TEXT", [])?;

    // Create index for efficient queries by request_id
    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_audit_log_request_id ON audit_log(request_id)",
        [],
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
        assert!(tables.contains(&"email_drafts".to_string()));
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

    #[test]
    fn email_drafts_table_schema() {
        let conn = create_test_connection();
        run_migrations(&conn).unwrap();

        // Insert an email draft
        conn.execute(
            "INSERT INTO email_drafts (id, user_id, to_address, cc, subject, body, created_at, expires_at)
             VALUES ('draft1', 'user1', 'recipient@example.com', 'cc1@example.com,cc2@example.com', 'Test Subject', 'Test body content', '2024-01-01T00:00:00Z', '2024-01-08T00:00:00Z')",
            [],
        )
        .unwrap();

        // Verify we can query it back
        let (to, cc, subject, body): (String, Option<String>, String, String) = conn
            .query_row(
                "SELECT to_address, cc, subject, body FROM email_drafts WHERE id = 'draft1'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .unwrap();

        assert_eq!(to, "recipient@example.com");
        assert_eq!(cc.unwrap(), "cc1@example.com,cc2@example.com");
        assert_eq!(subject, "Test Subject");
        assert_eq!(body, "Test body content");
    }

    #[test]
    fn email_drafts_allows_null_cc() {
        let conn = create_test_connection();
        run_migrations(&conn).unwrap();

        // Insert a draft without CC
        conn.execute(
            "INSERT INTO email_drafts (id, user_id, to_address, subject, body, created_at, expires_at)
             VALUES ('draft2', 'user1', 'recipient@example.com', 'Subject', 'Body', '2024-01-01T00:00:00Z', '2024-01-08T00:00:00Z')",
            [],
        )
        .unwrap();

        // Verify NULL CC value
        let cc: Option<String> = conn
            .query_row(
                "SELECT cc FROM email_drafts WHERE id = 'draft2'",
                [],
                |row| row.get(0),
            )
            .unwrap();

        assert!(cc.is_none());
    }
}
