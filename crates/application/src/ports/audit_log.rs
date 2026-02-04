//! Port for audit log persistence
//!
//! This port defines the interface for recording and querying audit entries.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use domain::{AuditEntry, AuditEventType};

use crate::error::ApplicationError;

/// Criteria for querying audit entries
#[derive(Debug, Clone, Default)]
pub struct AuditQuery {
    /// Filter by event type
    pub event_type: Option<AuditEventType>,
    /// Filter by actor
    pub actor: Option<String>,
    /// Filter by resource type
    pub resource_type: Option<String>,
    /// Filter by resource ID
    pub resource_id: Option<String>,
    /// Filter by success/failure
    pub success: Option<bool>,
    /// Start of time range
    pub from: Option<DateTime<Utc>>,
    /// End of time range
    pub to: Option<DateTime<Utc>>,
    /// Maximum results to return
    pub limit: Option<u32>,
    /// Offset for pagination
    pub offset: Option<u32>,
}

impl AuditQuery {
    /// Create a new empty query
    pub fn new() -> Self {
        Self::default()
    }

    /// Filter by event type
    pub fn with_event_type(mut self, event_type: AuditEventType) -> Self {
        self.event_type = Some(event_type);
        self
    }

    /// Filter by actor
    pub fn with_actor(mut self, actor: impl Into<String>) -> Self {
        self.actor = Some(actor.into());
        self
    }

    /// Filter by resource
    pub fn with_resource(
        mut self,
        resource_type: impl Into<String>,
        resource_id: impl Into<String>,
    ) -> Self {
        self.resource_type = Some(resource_type.into());
        self.resource_id = Some(resource_id.into());
        self
    }

    /// Filter by success/failure
    pub const fn with_success(mut self, success: bool) -> Self {
        self.success = Some(success);
        self
    }

    /// Filter by time range
    pub const fn with_time_range(mut self, from: DateTime<Utc>, to: DateTime<Utc>) -> Self {
        self.from = Some(from);
        self.to = Some(to);
        self
    }

    /// Limit results
    pub const fn with_limit(mut self, limit: u32) -> Self {
        self.limit = Some(limit);
        self
    }

    /// Set offset for pagination
    pub const fn with_offset(mut self, offset: u32) -> Self {
        self.offset = Some(offset);
        self
    }
}

/// Port for audit log storage
#[async_trait]
pub trait AuditLogPort: Send + Sync {
    /// Record an audit entry
    async fn log(&self, entry: &AuditEntry) -> Result<(), ApplicationError>;

    /// Query audit entries
    async fn query(&self, query: &AuditQuery) -> Result<Vec<AuditEntry>, ApplicationError>;

    /// Get recent audit entries
    async fn get_recent(&self, limit: u32) -> Result<Vec<AuditEntry>, ApplicationError>;

    /// Get entries for a specific resource
    async fn get_for_resource(
        &self,
        resource_type: &str,
        resource_id: &str,
    ) -> Result<Vec<AuditEntry>, ApplicationError>;

    /// Get entries for a specific actor
    async fn get_for_actor(
        &self,
        actor: &str,
        limit: u32,
    ) -> Result<Vec<AuditEntry>, ApplicationError>;

    /// Count entries matching a query
    async fn count(&self, query: &AuditQuery) -> Result<u64, ApplicationError>;
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use domain::AuditBuilder;
    use tokio::sync::Mutex;

    use super::*;

    /// Mock implementation for testing
    #[derive(Default)]
    struct MockAuditLog {
        entries: Arc<Mutex<Vec<AuditEntry>>>,
    }

    #[async_trait]
    impl AuditLogPort for MockAuditLog {
        async fn log(&self, entry: &AuditEntry) -> Result<(), ApplicationError> {
            self.entries.lock().await.push(entry.clone());
            Ok(())
        }

        async fn query(&self, query: &AuditQuery) -> Result<Vec<AuditEntry>, ApplicationError> {
            let entries = self.entries.lock().await;
            let mut results: Vec<_> = entries
                .iter()
                .filter(|e| {
                    if let Some(ref event_type) = query.event_type {
                        if &e.event_type != event_type {
                            return false;
                        }
                    }
                    if let Some(ref actor) = query.actor {
                        if e.actor.as_ref() != Some(actor) {
                            return false;
                        }
                    }
                    if let Some(success) = query.success {
                        if e.success != success {
                            return false;
                        }
                    }
                    true
                })
                .cloned()
                .collect();
            drop(entries);

            if let Some(limit) = query.limit {
                results.truncate(limit as usize);
            }

            Ok(results)
        }

        async fn get_recent(&self, limit: u32) -> Result<Vec<AuditEntry>, ApplicationError> {
            let entries = self.entries.lock().await;
            Ok(entries.iter().rev().take(limit as usize).cloned().collect())
        }

        async fn get_for_resource(
            &self,
            resource_type: &str,
            resource_id: &str,
        ) -> Result<Vec<AuditEntry>, ApplicationError> {
            let entries = self.entries.lock().await;
            Ok(entries
                .iter()
                .filter(|e| {
                    e.resource_type.as_deref() == Some(resource_type)
                        && e.resource_id.as_deref() == Some(resource_id)
                })
                .cloned()
                .collect())
        }

        async fn get_for_actor(
            &self,
            actor: &str,
            limit: u32,
        ) -> Result<Vec<AuditEntry>, ApplicationError> {
            let entries = self.entries.lock().await;
            Ok(entries
                .iter()
                .filter(|e| e.actor.as_deref() == Some(actor))
                .take(limit as usize)
                .cloned()
                .collect())
        }

        async fn count(&self, query: &AuditQuery) -> Result<u64, ApplicationError> {
            let results = self.query(query).await?;
            Ok(results.len() as u64)
        }
    }

    #[tokio::test]
    async fn log_and_query() {
        let log = MockAuditLog::default();

        let entry = AuditBuilder::auth_success("user-123");
        log.log(&entry).await.unwrap();

        let results = log.get_recent(10).await.unwrap();
        assert_eq!(results.len(), 1);
    }

    #[tokio::test]
    async fn query_by_event_type() {
        let log = MockAuditLog::default();

        log.log(&AuditBuilder::auth_success("user-1"))
            .await
            .unwrap();
        log.log(&AuditBuilder::command_executed("user-1", "echo", "cmd-1"))
            .await
            .unwrap();
        log.log(&AuditBuilder::auth_failure("bad key"))
            .await
            .unwrap();

        let query = AuditQuery::new().with_event_type(AuditEventType::Authentication);
        let results = log.query(&query).await.unwrap();

        assert_eq!(results.len(), 2);
    }

    #[tokio::test]
    async fn query_by_success() {
        let log = MockAuditLog::default();

        log.log(&AuditBuilder::auth_success("user-1"))
            .await
            .unwrap();
        log.log(&AuditBuilder::auth_failure("bad key"))
            .await
            .unwrap();

        let query = AuditQuery::new().with_success(false);
        let results = log.query(&query).await.unwrap();

        assert_eq!(results.len(), 1);
        assert!(!results[0].success);
    }

    #[tokio::test]
    async fn get_for_resource() {
        let log = MockAuditLog::default();

        log.log(&AuditBuilder::approval_requested(
            "user-1",
            "apr-123",
            "send_email",
        ))
        .await
        .unwrap();
        log.log(&AuditBuilder::approval_granted("apr-123"))
            .await
            .unwrap();
        log.log(&AuditBuilder::approval_requested(
            "user-2", "apr-456", "delete",
        ))
        .await
        .unwrap();

        let results = log.get_for_resource("approval", "apr-123").await.unwrap();
        assert_eq!(results.len(), 2);
    }

    #[tokio::test]
    async fn get_for_actor() {
        let log = MockAuditLog::default();

        log.log(&AuditBuilder::auth_success("user-123"))
            .await
            .unwrap();
        log.log(&AuditBuilder::command_executed("user-123", "echo", "cmd-1"))
            .await
            .unwrap();
        log.log(&AuditBuilder::auth_success("user-456"))
            .await
            .unwrap();

        let results = log.get_for_actor("user-123", 10).await.unwrap();
        assert_eq!(results.len(), 2);
    }

    #[tokio::test]
    async fn count_entries() {
        let log = MockAuditLog::default();

        for _ in 0..5 {
            log.log(&AuditBuilder::auth_success("user")).await.unwrap();
        }

        let count = log.count(&AuditQuery::new()).await.unwrap();
        assert_eq!(count, 5);
    }

    #[test]
    fn query_builder() {
        let query = AuditQuery::new()
            .with_event_type(AuditEventType::Security)
            .with_actor("admin")
            .with_success(false)
            .with_limit(100);

        assert_eq!(query.event_type, Some(AuditEventType::Security));
        assert_eq!(query.actor, Some("admin".to_string()));
        assert_eq!(query.success, Some(false));
        assert_eq!(query.limit, Some(100));
    }
}
