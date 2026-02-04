//! SQLite audit log implementation
//!
//! Implements the AuditLogPort using SQLite for persistent audit logging.

use std::{net::IpAddr, sync::Arc};

use application::{
    error::ApplicationError,
    ports::{AuditLogPort, AuditQuery},
};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use domain::{AuditEntry, AuditEventType};
use rusqlite::{Row, params};
use tokio::task;
use tracing::{debug, instrument};

use super::connection::ConnectionPool;

/// SQLite-based audit log implementation
#[derive(Debug, Clone)]
pub struct SqliteAuditLog {
    pool: Arc<ConnectionPool>,
}

impl SqliteAuditLog {
    /// Create a new SQLite audit log
    #[must_use]
    pub fn new(pool: Arc<ConnectionPool>) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl AuditLogPort for SqliteAuditLog {
    #[instrument(skip(self, entry), fields(event_type = %entry.event_type, action = %entry.action))]
    async fn log(&self, entry: &AuditEntry) -> Result<(), ApplicationError> {
        let pool = Arc::clone(&self.pool);
        let entry = entry.clone();

        task::spawn_blocking(move || {
            let conn = pool
                .get()
                .map_err(|e| ApplicationError::Internal(e.to_string()))?;

            conn.execute(
                "INSERT INTO audit_log (timestamp, event_type, actor, resource_type, resource_id, action, details, ip_address, success)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                params![
                    entry.timestamp.to_rfc3339(),
                    entry.event_type.to_string(),
                    entry.actor,
                    entry.resource_type,
                    entry.resource_id,
                    entry.action,
                    entry.details,
                    entry.ip_address.map(|ip| ip.to_string()),
                    entry.success as i32,
                ],
            )
            .map_err(|e| ApplicationError::Internal(e.to_string()))?;

            debug!("Recorded audit entry");
            Ok(())
        })
        .await
        .map_err(|e| ApplicationError::Internal(e.to_string()))?
    }

    #[instrument(skip(self, query))]
    async fn query(&self, query: &AuditQuery) -> Result<Vec<AuditEntry>, ApplicationError> {
        let pool = Arc::clone(&self.pool);
        let query = query.clone();

        task::spawn_blocking(move || {
            let conn = pool
                .get()
                .map_err(|e| ApplicationError::Internal(e.to_string()))?;

            let (sql, params) = build_query_sql(&query);

            let mut stmt = conn
                .prepare(&sql)
                .map_err(|e| ApplicationError::Internal(e.to_string()))?;

            let entries = stmt
                .query_map(
                    rusqlite::params_from_iter(params.iter()),
                    row_to_audit_entry,
                )
                .map_err(|e| ApplicationError::Internal(e.to_string()))?
                .filter_map(Result::ok)
                .collect();

            Ok(entries)
        })
        .await
        .map_err(|e| ApplicationError::Internal(e.to_string()))?
    }

    #[instrument(skip(self))]
    async fn get_recent(&self, limit: u32) -> Result<Vec<AuditEntry>, ApplicationError> {
        let pool = Arc::clone(&self.pool);

        task::spawn_blocking(move || {
            let conn = pool
                .get()
                .map_err(|e| ApplicationError::Internal(e.to_string()))?;

            let mut stmt = conn
                .prepare(
                    "SELECT id, timestamp, event_type, actor, resource_type, resource_id, action, details, ip_address, success
                     FROM audit_log
                     ORDER BY timestamp DESC
                     LIMIT ?1",
                )
                .map_err(|e| ApplicationError::Internal(e.to_string()))?;

            let entries = stmt
                .query_map([limit], row_to_audit_entry)
                .map_err(|e| ApplicationError::Internal(e.to_string()))?
                .filter_map(Result::ok)
                .collect();

            Ok(entries)
        })
        .await
        .map_err(|e| ApplicationError::Internal(e.to_string()))?
    }

    #[instrument(skip(self))]
    async fn get_for_resource(
        &self,
        resource_type: &str,
        resource_id: &str,
    ) -> Result<Vec<AuditEntry>, ApplicationError> {
        let pool = Arc::clone(&self.pool);
        let resource_type = resource_type.to_string();
        let resource_id = resource_id.to_string();

        task::spawn_blocking(move || {
            let conn = pool
                .get()
                .map_err(|e| ApplicationError::Internal(e.to_string()))?;

            let mut stmt = conn
                .prepare(
                    "SELECT id, timestamp, event_type, actor, resource_type, resource_id, action, details, ip_address, success
                     FROM audit_log
                     WHERE resource_type = ?1 AND resource_id = ?2
                     ORDER BY timestamp DESC",
                )
                .map_err(|e| ApplicationError::Internal(e.to_string()))?;

            let entries = stmt
                .query_map([&resource_type, &resource_id], row_to_audit_entry)
                .map_err(|e| ApplicationError::Internal(e.to_string()))?
                .filter_map(Result::ok)
                .collect();

            Ok(entries)
        })
        .await
        .map_err(|e| ApplicationError::Internal(e.to_string()))?
    }

    #[instrument(skip(self))]
    async fn get_for_actor(
        &self,
        actor: &str,
        limit: u32,
    ) -> Result<Vec<AuditEntry>, ApplicationError> {
        let pool = Arc::clone(&self.pool);
        let actor = actor.to_string();

        task::spawn_blocking(move || {
            let conn = pool
                .get()
                .map_err(|e| ApplicationError::Internal(e.to_string()))?;

            let mut stmt = conn
                .prepare(
                    "SELECT id, timestamp, event_type, actor, resource_type, resource_id, action, details, ip_address, success
                     FROM audit_log
                     WHERE actor = ?1
                     ORDER BY timestamp DESC
                     LIMIT ?2",
                )
                .map_err(|e| ApplicationError::Internal(e.to_string()))?;

            let entries = stmt
                .query_map(params![actor, limit], row_to_audit_entry)
                .map_err(|e| ApplicationError::Internal(e.to_string()))?
                .filter_map(Result::ok)
                .collect();

            Ok(entries)
        })
        .await
        .map_err(|e| ApplicationError::Internal(e.to_string()))?
    }

    #[instrument(skip(self, query))]
    async fn count(&self, query: &AuditQuery) -> Result<u64, ApplicationError> {
        let pool = Arc::clone(&self.pool);
        let query = query.clone();

        task::spawn_blocking(move || {
            let conn = pool
                .get()
                .map_err(|e| ApplicationError::Internal(e.to_string()))?;

            let (sql, params) = build_count_sql(&query);

            let count: i64 = conn
                .query_row(&sql, rusqlite::params_from_iter(params.iter()), |row| {
                    row.get(0)
                })
                .map_err(|e| ApplicationError::Internal(e.to_string()))?;

            Ok(count as u64)
        })
        .await
        .map_err(|e| ApplicationError::Internal(e.to_string()))?
    }
}

/// Convert a database row to an audit entry
fn row_to_audit_entry(row: &Row<'_>) -> rusqlite::Result<AuditEntry> {
    let id: i64 = row.get(0)?;
    let timestamp_str: String = row.get(1)?;
    let event_type_str: String = row.get(2)?;
    let actor: Option<String> = row.get(3)?;
    let resource_type: Option<String> = row.get(4)?;
    let resource_id: Option<String> = row.get(5)?;
    let action: String = row.get(6)?;
    let details: Option<String> = row.get(7)?;
    let ip_str: Option<String> = row.get(8)?;
    let success: i32 = row.get(9)?;

    let timestamp = DateTime::parse_from_rfc3339(&timestamp_str)
        .map(|dt| dt.with_timezone(&Utc))
        .unwrap_or_else(|_| Utc::now());

    let event_type = parse_event_type(&event_type_str);

    let ip_address = ip_str.and_then(|s| s.parse::<IpAddr>().ok());

    Ok(AuditEntry {
        id: Some(id),
        timestamp,
        event_type,
        actor,
        resource_type,
        resource_id,
        action,
        details,
        ip_address,
        success: success != 0,
    })
}

/// Parse event type from string
fn parse_event_type(s: &str) -> AuditEventType {
    match s {
        "authentication" => AuditEventType::Authentication,
        "authorization" => AuditEventType::Authorization,
        "command_execution" => AuditEventType::CommandExecution,
        "approval" => AuditEventType::Approval,
        "config_change" => AuditEventType::ConfigChange,
        "system" => AuditEventType::System,
        "data_access" => AuditEventType::DataAccess,
        "integration" => AuditEventType::Integration,
        "security" => AuditEventType::Security,
        _ => AuditEventType::System, // Default fallback
    }
}

/// Build the SQL query based on the audit query parameters
fn build_query_sql(query: &AuditQuery) -> (String, Vec<String>) {
    let mut sql = String::from(
        "SELECT id, timestamp, event_type, actor, resource_type, resource_id, action, details, ip_address, success
         FROM audit_log WHERE 1=1",
    );
    let mut params: Vec<String> = Vec::new();
    let mut param_idx = 1;

    if let Some(ref event_type) = query.event_type {
        sql.push_str(&format!(" AND event_type = ?{param_idx}"));
        params.push(event_type.to_string());
        param_idx += 1;
    }

    if let Some(ref actor) = query.actor {
        sql.push_str(&format!(" AND actor = ?{param_idx}"));
        params.push(actor.clone());
        param_idx += 1;
    }

    if let Some(ref resource_type) = query.resource_type {
        sql.push_str(&format!(" AND resource_type = ?{param_idx}"));
        params.push(resource_type.clone());
        param_idx += 1;
    }

    if let Some(ref resource_id) = query.resource_id {
        sql.push_str(&format!(" AND resource_id = ?{param_idx}"));
        params.push(resource_id.clone());
        param_idx += 1;
    }

    if let Some(success) = query.success {
        sql.push_str(&format!(" AND success = ?{param_idx}"));
        params.push((success as i32).to_string());
        param_idx += 1;
    }

    if let Some(from) = query.from {
        sql.push_str(&format!(" AND timestamp >= ?{param_idx}"));
        params.push(from.to_rfc3339());
        param_idx += 1;
    }

    if let Some(to) = query.to {
        sql.push_str(&format!(" AND timestamp <= ?{param_idx}"));
        params.push(to.to_rfc3339());
        param_idx += 1;
    }

    sql.push_str(" ORDER BY timestamp DESC");

    if let Some(limit) = query.limit {
        sql.push_str(&format!(" LIMIT ?{param_idx}"));
        params.push(limit.to_string());
        param_idx += 1;
    }

    if let Some(offset) = query.offset {
        sql.push_str(&format!(" OFFSET ?{param_idx}"));
        params.push(offset.to_string());
    }

    (sql, params)
}

/// Build the count SQL based on the query parameters
fn build_count_sql(query: &AuditQuery) -> (String, Vec<String>) {
    let mut sql = String::from("SELECT COUNT(*) FROM audit_log WHERE 1=1");
    let mut params: Vec<String> = Vec::new();
    let mut param_idx = 1;

    if let Some(ref event_type) = query.event_type {
        sql.push_str(&format!(" AND event_type = ?{param_idx}"));
        params.push(event_type.to_string());
        param_idx += 1;
    }

    if let Some(ref actor) = query.actor {
        sql.push_str(&format!(" AND actor = ?{param_idx}"));
        params.push(actor.clone());
        param_idx += 1;
    }

    if let Some(ref resource_type) = query.resource_type {
        sql.push_str(&format!(" AND resource_type = ?{param_idx}"));
        params.push(resource_type.clone());
        param_idx += 1;
    }

    if let Some(ref resource_id) = query.resource_id {
        sql.push_str(&format!(" AND resource_id = ?{param_idx}"));
        params.push(resource_id.clone());
        param_idx += 1;
    }

    if let Some(success) = query.success {
        sql.push_str(&format!(" AND success = ?{param_idx}"));
        params.push((success as i32).to_string());
        param_idx += 1;
    }

    if let Some(from) = query.from {
        sql.push_str(&format!(" AND timestamp >= ?{param_idx}"));
        params.push(from.to_rfc3339());
        param_idx += 1;
    }

    if let Some(to) = query.to {
        sql.push_str(&format!(" AND timestamp <= ?{param_idx}"));
        params.push(to.to_rfc3339());
    }

    (sql, params)
}

#[cfg(test)]
mod tests {
    use std::net::{IpAddr, Ipv4Addr};

    use chrono::Duration;

    use super::*;
    use crate::{config::DatabaseConfig, persistence::create_pool};

    fn create_test_pool() -> Arc<ConnectionPool> {
        let config = DatabaseConfig {
            path: ":memory:".to_string(),
            max_connections: 1,
            run_migrations: true,
        };
        Arc::new(create_pool(&config).expect("Failed to create pool"))
    }

    #[tokio::test]
    async fn log_and_query_entry() {
        let pool = create_test_pool();
        let audit_log = SqliteAuditLog::new(pool);

        let entry = AuditEntry::success(AuditEventType::CommandExecution, "test_action")
            .with_actor("user123")
            .with_resource("conversation", "conv-001");

        audit_log.log(&entry).await.expect("Failed to log entry");

        let entries = audit_log.get_recent(10).await.expect("Failed to query");
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].action, "test_action");
        assert_eq!(entries[0].actor, Some("user123".to_string()));
    }

    #[tokio::test]
    async fn get_for_resource() {
        let pool = create_test_pool();
        let audit_log = SqliteAuditLog::new(pool);

        // Log entries for different resources
        let entry1 = AuditEntry::success(AuditEventType::DataAccess, "read")
            .with_resource("conversation", "conv-001");
        let entry2 = AuditEntry::success(AuditEventType::DataAccess, "write")
            .with_resource("conversation", "conv-001");
        let entry3 = AuditEntry::success(AuditEventType::DataAccess, "read")
            .with_resource("conversation", "conv-002");

        audit_log.log(&entry1).await.unwrap();
        audit_log.log(&entry2).await.unwrap();
        audit_log.log(&entry3).await.unwrap();

        let entries = audit_log
            .get_for_resource("conversation", "conv-001")
            .await
            .expect("Failed to query");

        assert_eq!(entries.len(), 2);
    }

    #[tokio::test]
    async fn get_for_actor() {
        let pool = create_test_pool();
        let audit_log = SqliteAuditLog::new(pool);

        let entry1 =
            AuditEntry::success(AuditEventType::Authentication, "login").with_actor("user1");
        let entry2 =
            AuditEntry::success(AuditEventType::CommandExecution, "echo").with_actor("user1");
        let entry3 =
            AuditEntry::success(AuditEventType::Authentication, "login").with_actor("user2");

        audit_log.log(&entry1).await.unwrap();
        audit_log.log(&entry2).await.unwrap();
        audit_log.log(&entry3).await.unwrap();

        let entries = audit_log
            .get_for_actor("user1", 10)
            .await
            .expect("Failed to query");

        assert_eq!(entries.len(), 2);
    }

    #[tokio::test]
    async fn query_with_event_type_filter() {
        let pool = create_test_pool();
        let audit_log = SqliteAuditLog::new(pool);

        let entry1 = AuditEntry::success(AuditEventType::Authentication, "login");
        let entry2 = AuditEntry::success(AuditEventType::CommandExecution, "echo");
        let entry3 = AuditEntry::success(AuditEventType::Authentication, "logout");

        audit_log.log(&entry1).await.unwrap();
        audit_log.log(&entry2).await.unwrap();
        audit_log.log(&entry3).await.unwrap();

        let query = AuditQuery::new().with_event_type(AuditEventType::Authentication);
        let entries = audit_log.query(&query).await.expect("Failed to query");

        assert_eq!(entries.len(), 2);
    }

    #[tokio::test]
    async fn query_with_success_filter() {
        let pool = create_test_pool();
        let audit_log = SqliteAuditLog::new(pool);

        let entry1 = AuditEntry::success(AuditEventType::Authentication, "login");
        let entry2 = AuditEntry::failure(AuditEventType::Authentication, "login");
        let entry3 = AuditEntry::success(AuditEventType::Authentication, "logout");

        audit_log.log(&entry1).await.unwrap();
        audit_log.log(&entry2).await.unwrap();
        audit_log.log(&entry3).await.unwrap();

        let query = AuditQuery::new().with_success(false);
        let entries = audit_log.query(&query).await.expect("Failed to query");

        assert_eq!(entries.len(), 1);
        assert!(!entries[0].success);
    }

    #[tokio::test]
    async fn query_with_time_range() {
        let pool = create_test_pool();
        let audit_log = SqliteAuditLog::new(pool);

        let now = Utc::now();
        let entry1 = AuditEntry::success(AuditEventType::System, "startup");

        audit_log.log(&entry1).await.unwrap();

        // Query with a time range that includes now
        let from = now - Duration::hours(1);
        let to = now + Duration::hours(1);
        let query = AuditQuery::new().with_time_range(from, to);
        let entries = audit_log.query(&query).await.expect("Failed to query");

        assert_eq!(entries.len(), 1);
    }

    #[tokio::test]
    async fn query_with_pagination() {
        let pool = create_test_pool();
        let audit_log = SqliteAuditLog::new(pool);

        for i in 0..10 {
            let entry = AuditEntry::success(AuditEventType::System, format!("action_{i}"));
            audit_log.log(&entry).await.unwrap();
        }

        let query = AuditQuery::new().with_limit(3).with_offset(0);
        let page1 = audit_log.query(&query).await.expect("Failed to query");
        assert_eq!(page1.len(), 3);

        let query = AuditQuery::new().with_limit(3).with_offset(3);
        let page2 = audit_log.query(&query).await.expect("Failed to query");
        assert_eq!(page2.len(), 3);
    }

    #[tokio::test]
    async fn count_entries() {
        let pool = create_test_pool();
        let audit_log = SqliteAuditLog::new(pool);

        for i in 0..5 {
            let entry = AuditEntry::success(AuditEventType::CommandExecution, format!("cmd_{i}"));
            audit_log.log(&entry).await.unwrap();
        }

        let query = AuditQuery::new();
        let count = audit_log.count(&query).await.expect("Failed to count");

        assert_eq!(count, 5);
    }

    #[tokio::test]
    async fn log_entry_with_ip_address() {
        let pool = create_test_pool();
        let audit_log = SqliteAuditLog::new(pool);

        let ip = IpAddr::V4(Ipv4Addr::new(192, 168, 1, 100));
        let entry = AuditEntry::success(AuditEventType::Authentication, "login")
            .with_actor("user1")
            .with_ip_address(ip);

        audit_log.log(&entry).await.expect("Failed to log entry");

        let entries = audit_log.get_recent(1).await.expect("Failed to query");
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].ip_address, Some(ip));
    }

    #[tokio::test]
    async fn log_entry_with_details() {
        let pool = create_test_pool();
        let audit_log = SqliteAuditLog::new(pool);

        let entry = AuditEntry::success(AuditEventType::ConfigChange, "update_setting")
            .with_details(r#"{"key": "timeout", "old": 30, "new": 60}"#);

        audit_log.log(&entry).await.expect("Failed to log entry");

        let entries = audit_log.get_recent(1).await.expect("Failed to query");
        assert_eq!(entries.len(), 1);
        assert!(entries[0].details.as_ref().unwrap().contains("timeout"));
    }

    #[test]
    fn parse_all_event_types() {
        assert_eq!(
            parse_event_type("authentication"),
            AuditEventType::Authentication
        );
        assert_eq!(
            parse_event_type("authorization"),
            AuditEventType::Authorization
        );
        assert_eq!(
            parse_event_type("command_execution"),
            AuditEventType::CommandExecution
        );
        assert_eq!(parse_event_type("approval"), AuditEventType::Approval);
        assert_eq!(
            parse_event_type("config_change"),
            AuditEventType::ConfigChange
        );
        assert_eq!(parse_event_type("system"), AuditEventType::System);
        assert_eq!(parse_event_type("data_access"), AuditEventType::DataAccess);
        assert_eq!(parse_event_type("integration"), AuditEventType::Integration);
        assert_eq!(parse_event_type("security"), AuditEventType::Security);
        assert_eq!(parse_event_type("unknown"), AuditEventType::System);
    }

    #[test]
    fn sqlite_audit_log_is_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<SqliteAuditLog>();
    }
}
