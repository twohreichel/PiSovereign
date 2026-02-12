//! SQLite audit log implementation
//!
//! Implements the `AuditLogPort` using sqlx for persistent audit logging.

use std::net::IpAddr;

use application::{
    error::ApplicationError,
    ports::{AuditLogPort, AuditQuery},
};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use domain::{AuditEntry, AuditEventType};
use sqlx::SqlitePool;
use tracing::{debug, instrument};

use super::error::map_sqlx_error;

/// SQLite-based audit log implementation
#[derive(Debug, Clone)]
pub struct SqliteAuditLog {
    pool: SqlitePool,
}

impl SqliteAuditLog {
    /// Create a new SQLite audit log
    #[must_use]
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

/// Row type for audit log queries
#[derive(sqlx::FromRow)]
struct AuditRow {
    id: i64,
    timestamp: String,
    event_type: String,
    actor: Option<String>,
    resource_type: Option<String>,
    resource_id: Option<String>,
    action: String,
    details: Option<String>,
    ip_address: Option<String>,
    success: i32,
    request_id: Option<String>,
}

impl AuditRow {
    fn to_entry(self) -> AuditEntry {
        let timestamp = DateTime::parse_from_rfc3339(&self.timestamp)
            .map_or_else(|_| Utc::now(), |dt| dt.with_timezone(&Utc));
        let event_type = parse_event_type(&self.event_type);
        let ip_address = self.ip_address.and_then(|s| s.parse::<IpAddr>().ok());
        let request_id = self.request_id.and_then(|s| uuid::Uuid::parse_str(&s).ok());

        AuditEntry {
            id: Some(self.id),
            timestamp,
            event_type,
            actor: self.actor,
            resource_type: self.resource_type,
            resource_id: self.resource_id,
            action: self.action,
            details: self.details,
            ip_address,
            success: self.success != 0,
            request_id,
        }
    }
}

#[async_trait]
impl AuditLogPort for SqliteAuditLog {
    #[instrument(skip(self, entry), fields(event_type = %entry.event_type, action = %entry.action))]
    async fn log(&self, entry: &AuditEntry) -> Result<(), ApplicationError> {
        sqlx::query(
            "INSERT INTO audit_log (timestamp, event_type, actor, resource_type, resource_id, \
             action, details, ip_address, success, request_id)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)",
        )
        .bind(entry.timestamp.to_rfc3339())
        .bind(entry.event_type.to_string())
        .bind(&entry.actor)
        .bind(&entry.resource_type)
        .bind(&entry.resource_id)
        .bind(&entry.action)
        .bind(&entry.details)
        .bind(entry.ip_address.map(|ip| ip.to_string()))
        .bind(i32::from(entry.success))
        .bind(entry.request_id.map(|id| id.to_string()))
        .execute(&self.pool)
        .await
        .map_err(map_sqlx_error)?;

        debug!("Recorded audit entry");
        Ok(())
    }

    #[instrument(skip(self, query))]
    async fn query(&self, query: &AuditQuery) -> Result<Vec<AuditEntry>, ApplicationError> {
        // Build dynamic query
        let mut sql = String::from(
            "SELECT id, timestamp, event_type, actor, resource_type, resource_id, \
             action, details, ip_address, success, request_id
             FROM audit_log WHERE 1=1",
        );
        let mut binds: Vec<String> = Vec::new();

        if let Some(ref event_type) = query.event_type {
            binds.push(event_type.to_string());
            sql.push_str(&format!(" AND event_type = ${}", binds.len()));
        }
        if let Some(ref actor) = query.actor {
            binds.push(actor.clone());
            sql.push_str(&format!(" AND actor = ${}", binds.len()));
        }
        if let Some(ref resource_type) = query.resource_type {
            binds.push(resource_type.clone());
            sql.push_str(&format!(" AND resource_type = ${}", binds.len()));
        }
        if let Some(ref resource_id) = query.resource_id {
            binds.push(resource_id.clone());
            sql.push_str(&format!(" AND resource_id = ${}", binds.len()));
        }
        if let Some(success) = query.success {
            binds.push(i32::from(success).to_string());
            sql.push_str(&format!(" AND success = ${}", binds.len()));
        }
        if let Some(from) = query.from {
            binds.push(from.to_rfc3339());
            sql.push_str(&format!(" AND timestamp >= ${}", binds.len()));
        }
        if let Some(to) = query.to {
            binds.push(to.to_rfc3339());
            sql.push_str(&format!(" AND timestamp <= ${}", binds.len()));
        }

        sql.push_str(" ORDER BY timestamp DESC");

        if let Some(limit) = query.limit {
            binds.push(limit.to_string());
            sql.push_str(&format!(" LIMIT ${}", binds.len()));
        }
        if let Some(offset) = query.offset {
            binds.push(offset.to_string());
            sql.push_str(&format!(" OFFSET ${}", binds.len()));
        }

        let mut q = sqlx::query_as::<_, AuditRow>(&sql);
        for bind in &binds {
            q = q.bind(bind);
        }

        let rows: Vec<AuditRow> = q.fetch_all(&self.pool).await.map_err(map_sqlx_error)?;
        Ok(rows.into_iter().map(AuditRow::to_entry).collect())
    }

    #[instrument(skip(self))]
    async fn get_recent(&self, limit: u32) -> Result<Vec<AuditEntry>, ApplicationError> {
        let rows: Vec<AuditRow> = sqlx::query_as(
            "SELECT id, timestamp, event_type, actor, resource_type, resource_id, \
             action, details, ip_address, success, request_id
             FROM audit_log
             ORDER BY timestamp DESC
             LIMIT $1",
        )
        .bind(limit)
        .fetch_all(&self.pool)
        .await
        .map_err(map_sqlx_error)?;

        Ok(rows.into_iter().map(AuditRow::to_entry).collect())
    }

    #[instrument(skip(self))]
    async fn get_for_resource(
        &self,
        resource_type: &str,
        resource_id: &str,
    ) -> Result<Vec<AuditEntry>, ApplicationError> {
        let rows: Vec<AuditRow> = sqlx::query_as(
            "SELECT id, timestamp, event_type, actor, resource_type, resource_id, \
             action, details, ip_address, success, request_id
             FROM audit_log
             WHERE resource_type = $1 AND resource_id = $2
             ORDER BY timestamp DESC",
        )
        .bind(resource_type)
        .bind(resource_id)
        .fetch_all(&self.pool)
        .await
        .map_err(map_sqlx_error)?;

        Ok(rows.into_iter().map(AuditRow::to_entry).collect())
    }

    #[instrument(skip(self))]
    async fn get_for_actor(
        &self,
        actor: &str,
        limit: u32,
    ) -> Result<Vec<AuditEntry>, ApplicationError> {
        let rows: Vec<AuditRow> = sqlx::query_as(
            "SELECT id, timestamp, event_type, actor, resource_type, resource_id, \
             action, details, ip_address, success, request_id
             FROM audit_log
             WHERE actor = $1
             ORDER BY timestamp DESC
             LIMIT $2",
        )
        .bind(actor)
        .bind(limit)
        .fetch_all(&self.pool)
        .await
        .map_err(map_sqlx_error)?;

        Ok(rows.into_iter().map(AuditRow::to_entry).collect())
    }

    #[instrument(skip(self, query))]
    async fn count(&self, query: &AuditQuery) -> Result<u64, ApplicationError> {
        let mut sql = String::from("SELECT COUNT(*) FROM audit_log WHERE 1=1");
        let mut binds: Vec<String> = Vec::new();

        if let Some(ref event_type) = query.event_type {
            binds.push(event_type.to_string());
            sql.push_str(&format!(" AND event_type = ${}", binds.len()));
        }
        if let Some(ref actor) = query.actor {
            binds.push(actor.clone());
            sql.push_str(&format!(" AND actor = ${}", binds.len()));
        }
        if let Some(ref resource_type) = query.resource_type {
            binds.push(resource_type.clone());
            sql.push_str(&format!(" AND resource_type = ${}", binds.len()));
        }
        if let Some(ref resource_id) = query.resource_id {
            binds.push(resource_id.clone());
            sql.push_str(&format!(" AND resource_id = ${}", binds.len()));
        }
        if let Some(success) = query.success {
            binds.push(i32::from(success).to_string());
            sql.push_str(&format!(" AND success = ${}", binds.len()));
        }
        if let Some(from) = query.from {
            binds.push(from.to_rfc3339());
            sql.push_str(&format!(" AND timestamp >= ${}", binds.len()));
        }
        if let Some(to) = query.to {
            binds.push(to.to_rfc3339());
            sql.push_str(&format!(" AND timestamp <= ${}", binds.len()));
        }

        let mut q = sqlx::query_scalar::<_, i64>(&sql);
        for bind in &binds {
            q = q.bind(bind);
        }

        let count: i64 = q.fetch_one(&self.pool).await.map_err(map_sqlx_error)?;

        #[allow(clippy::cast_sign_loss)]
        Ok(count as u64)
    }
}

/// Parse event type from string
fn parse_event_type(s: &str) -> AuditEventType {
    match s {
        "authentication" => AuditEventType::Authentication,
        "authorization" => AuditEventType::Authorization,
        "command_execution" => AuditEventType::CommandExecution,
        "approval" => AuditEventType::Approval,
        "config_change" => AuditEventType::ConfigChange,
        "data_access" => AuditEventType::DataAccess,
        "integration" => AuditEventType::Integration,
        "security" => AuditEventType::Security,
        _ => AuditEventType::System,
    }
}

#[cfg(test)]
mod tests {
    use std::net::{IpAddr, Ipv4Addr};

    use chrono::Duration;

    use super::*;
    use crate::persistence::async_connection::AsyncDatabase;

    async fn setup() -> (AsyncDatabase, SqliteAuditLog) {
        let db = AsyncDatabase::in_memory().await.unwrap();
        db.migrate().await.unwrap();
        let audit = SqliteAuditLog::new(db.pool().clone());
        (db, audit)
    }

    #[tokio::test]
    async fn log_and_query_entry() {
        let (_db, audit_log) = setup().await;

        let entry = AuditEntry::success(AuditEventType::CommandExecution, "test_action")
            .with_actor("user123")
            .with_resource("conversation", "conv-001");

        audit_log.log(&entry).await.unwrap();

        let entries = audit_log.get_recent(10).await.unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].action, "test_action");
        assert_eq!(entries[0].actor, Some("user123".to_string()));
    }

    #[tokio::test]
    async fn get_for_resource() {
        let (_db, audit_log) = setup().await;

        let e1 = AuditEntry::success(AuditEventType::DataAccess, "read")
            .with_resource("conversation", "conv-001");
        let e2 = AuditEntry::success(AuditEventType::DataAccess, "write")
            .with_resource("conversation", "conv-001");
        let e3 = AuditEntry::success(AuditEventType::DataAccess, "read")
            .with_resource("conversation", "conv-002");

        audit_log.log(&e1).await.unwrap();
        audit_log.log(&e2).await.unwrap();
        audit_log.log(&e3).await.unwrap();

        let entries = audit_log
            .get_for_resource("conversation", "conv-001")
            .await
            .unwrap();
        assert_eq!(entries.len(), 2);
    }

    #[tokio::test]
    async fn get_for_actor() {
        let (_db, audit_log) = setup().await;

        let e1 = AuditEntry::success(AuditEventType::Authentication, "login").with_actor("user1");
        let e2 = AuditEntry::success(AuditEventType::CommandExecution, "echo").with_actor("user1");
        let e3 = AuditEntry::success(AuditEventType::Authentication, "login").with_actor("user2");

        audit_log.log(&e1).await.unwrap();
        audit_log.log(&e2).await.unwrap();
        audit_log.log(&e3).await.unwrap();

        let entries = audit_log.get_for_actor("user1", 10).await.unwrap();
        assert_eq!(entries.len(), 2);
    }

    #[tokio::test]
    async fn query_with_event_type_filter() {
        let (_db, audit_log) = setup().await;

        audit_log
            .log(&AuditEntry::success(
                AuditEventType::Authentication,
                "login",
            ))
            .await
            .unwrap();
        audit_log
            .log(&AuditEntry::success(
                AuditEventType::CommandExecution,
                "echo",
            ))
            .await
            .unwrap();
        audit_log
            .log(&AuditEntry::success(
                AuditEventType::Authentication,
                "logout",
            ))
            .await
            .unwrap();

        let query = AuditQuery::new().with_event_type(AuditEventType::Authentication);
        let entries = audit_log.query(&query).await.unwrap();
        assert_eq!(entries.len(), 2);
    }

    #[tokio::test]
    async fn query_with_success_filter() {
        let (_db, audit_log) = setup().await;

        audit_log
            .log(&AuditEntry::success(
                AuditEventType::Authentication,
                "login",
            ))
            .await
            .unwrap();
        audit_log
            .log(&AuditEntry::failure(
                AuditEventType::Authentication,
                "login",
            ))
            .await
            .unwrap();
        audit_log
            .log(&AuditEntry::success(
                AuditEventType::Authentication,
                "logout",
            ))
            .await
            .unwrap();

        let query = AuditQuery::new().with_success(false);
        let entries = audit_log.query(&query).await.unwrap();
        assert_eq!(entries.len(), 1);
        assert!(!entries[0].success);
    }

    #[tokio::test]
    async fn query_with_time_range() {
        let (_db, audit_log) = setup().await;
        let now = Utc::now();

        audit_log
            .log(&AuditEntry::success(AuditEventType::System, "startup"))
            .await
            .unwrap();

        let from = now - Duration::hours(1);
        let to = now + Duration::hours(1);
        let query = AuditQuery::new().with_time_range(from, to);
        let entries = audit_log.query(&query).await.unwrap();
        assert_eq!(entries.len(), 1);
    }

    #[tokio::test]
    async fn query_with_pagination() {
        let (_db, audit_log) = setup().await;

        for i in 0..10 {
            let entry = AuditEntry::success(AuditEventType::System, format!("action_{i}"));
            audit_log.log(&entry).await.unwrap();
        }

        let page1 = audit_log
            .query(&AuditQuery::new().with_limit(3).with_offset(0))
            .await
            .unwrap();
        assert_eq!(page1.len(), 3);

        let page2 = audit_log
            .query(&AuditQuery::new().with_limit(3).with_offset(3))
            .await
            .unwrap();
        assert_eq!(page2.len(), 3);
    }

    #[tokio::test]
    async fn count_entries() {
        let (_db, audit_log) = setup().await;

        for i in 0..5 {
            let entry = AuditEntry::success(AuditEventType::CommandExecution, format!("cmd_{i}"));
            audit_log.log(&entry).await.unwrap();
        }

        let count = audit_log.count(&AuditQuery::new()).await.unwrap();
        assert_eq!(count, 5);
    }

    #[tokio::test]
    async fn log_entry_with_ip_address() {
        let (_db, audit_log) = setup().await;

        let ip = IpAddr::V4(Ipv4Addr::new(192, 168, 1, 100));
        let entry = AuditEntry::success(AuditEventType::Authentication, "login")
            .with_actor("user1")
            .with_ip_address(ip);

        audit_log.log(&entry).await.unwrap();

        let entries = audit_log.get_recent(1).await.unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].ip_address, Some(ip));
    }

    #[tokio::test]
    async fn log_entry_with_details() {
        let (_db, audit_log) = setup().await;

        let entry = AuditEntry::success(AuditEventType::ConfigChange, "update_setting")
            .with_details(r#"{"key": "timeout", "old": 30, "new": 60}"#);

        audit_log.log(&entry).await.unwrap();

        let entries = audit_log.get_recent(1).await.unwrap();
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
