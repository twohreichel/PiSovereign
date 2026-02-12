//! Persistent retry queue for failed operations with exponential backoff
//!
//! Provides durable storage for failed webhook deliveries, messages, and other
//! retryable operations. Uses SQLite (via sqlx) for persistence with automatic
//! cleanup of completed items and dead letter queue for permanently failed items.
//!
//! # Architecture
//!
//! ```text
//! ┌─────────────────┐     ┌─────────────────┐     ┌─────────────────┐
//! │  Failed Op      │ ──> │  Retry Queue    │ ──> │  Executor       │
//! └─────────────────┘     └─────────────────┘     └─────────────────┘
//!                                │                        │
//!                                │ max retries            │ success
//!                                ▼                        ▼
//!                         ┌─────────────────┐     ┌─────────────────┐
//!                         │  Dead Letter Q  │     │  Completed      │
//!                         └─────────────────┘     └─────────────────┘
//! ```

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use thiserror::Error;
use tracing::{debug, info, instrument, warn};
use uuid::Uuid;

use crate::retry::RetryConfig;

/// Error type for retry queue operations
#[derive(Debug, Error)]
pub enum RetryQueueError {
    /// Database operation failed
    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),

    /// Serialization error
    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    /// Item not found
    #[error("Item not found: {0}")]
    NotFound(String),

    /// Invalid state transition
    #[error("Invalid state transition from {from} to {to}")]
    InvalidTransition { from: String, to: String },
}

/// Status of a retry queue item
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RetryStatus {
    /// Waiting for next retry attempt
    Pending,
    /// Currently being processed
    InProgress,
    /// Successfully completed
    Completed,
    /// All retries exhausted
    Failed,
    /// Manually cancelled
    Cancelled,
}

impl std::fmt::Display for RetryStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Pending => write!(f, "pending"),
            Self::InProgress => write!(f, "in_progress"),
            Self::Completed => write!(f, "completed"),
            Self::Failed => write!(f, "failed"),
            Self::Cancelled => write!(f, "cancelled"),
        }
    }
}

impl std::str::FromStr for RetryStatus {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "pending" => Ok(Self::Pending),
            "in_progress" => Ok(Self::InProgress),
            "completed" => Ok(Self::Completed),
            "failed" => Ok(Self::Failed),
            "cancelled" => Ok(Self::Cancelled),
            _ => Err(format!("Unknown status: {s}")),
        }
    }
}

/// A retry queue item
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetryItem {
    /// Unique identifier
    pub id: String,
    /// Type of operation (e.g., "webhook", "message", "email")
    pub operation_type: String,
    /// JSON payload to retry
    pub payload: String,
    /// Target endpoint or recipient
    pub target: String,
    /// Number of retry attempts made
    pub attempt_count: u32,
    /// Maximum retries before marked as failed
    pub max_retries: u32,
    /// Next scheduled retry time
    pub next_retry_at: DateTime<Utc>,
    /// Current status
    pub status: RetryStatus,
    /// Last error message if failed
    pub last_error: Option<String>,
    /// Original creation time
    pub created_at: DateTime<Utc>,
    /// Last update time
    pub updated_at: DateTime<Utc>,
    /// Correlation ID for tracing
    pub correlation_id: Option<String>,
    /// User ID context
    pub user_id: Option<String>,
    /// Tenant ID context
    pub tenant_id: Option<String>,
}

impl RetryItem {
    /// Create a new retry item with default settings
    #[must_use]
    pub fn new(
        operation_type: impl Into<String>,
        payload: impl Into<String>,
        target: impl Into<String>,
    ) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4().to_string(),
            operation_type: operation_type.into(),
            payload: payload.into(),
            target: target.into(),
            attempt_count: 0,
            max_retries: 5,
            next_retry_at: now,
            status: RetryStatus::Pending,
            last_error: None,
            created_at: now,
            updated_at: now,
            correlation_id: None,
            user_id: None,
            tenant_id: None,
        }
    }

    /// Set the maximum number of retries
    #[must_use]
    pub const fn with_max_retries(mut self, max_retries: u32) -> Self {
        self.max_retries = max_retries;
        self
    }

    /// Set a correlation ID for tracing
    #[must_use]
    pub fn with_correlation_id(mut self, correlation_id: impl Into<String>) -> Self {
        self.correlation_id = Some(correlation_id.into());
        self
    }

    /// Set user context
    #[must_use]
    pub fn with_user_id(mut self, user_id: impl Into<String>) -> Self {
        self.user_id = Some(user_id.into());
        self
    }

    /// Set tenant context
    #[must_use]
    pub fn with_tenant_id(mut self, tenant_id: impl Into<String>) -> Self {
        self.tenant_id = Some(tenant_id.into());
        self
    }

    /// Calculate next retry delay using exponential backoff
    #[allow(
        clippy::cast_precision_loss,
        clippy::cast_possible_wrap,
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss
    )]
    fn calculate_next_delay(&self, config: &RetryConfig) -> std::time::Duration {
        let base_ms = config.initial_delay_ms as f64;
        let delay_ms = base_ms * config.multiplier.powi(self.attempt_count as i32);
        let capped_ms = delay_ms.min(config.max_delay_ms as f64);

        // Add jitter if enabled
        let final_ms = if config.jitter_enabled {
            use rand::Rng;
            let jitter = rand::rng().random_range(0.0..config.jitter_factor);
            capped_ms * (1.0 + jitter)
        } else {
            capped_ms
        };

        std::time::Duration::from_millis(final_ms as u64)
    }
}

/// Row type for retry queue queries
#[derive(sqlx::FromRow)]
struct RetryRow {
    id: String,
    operation_type: String,
    payload: String,
    target: String,
    attempt_count: i32,
    max_retries: i32,
    next_retry_at: String,
    status: String,
    last_error: Option<String>,
    created_at: String,
    updated_at: String,
    correlation_id: Option<String>,
    user_id: Option<String>,
    tenant_id: Option<String>,
}

impl RetryRow {
    #[allow(clippy::cast_sign_loss, clippy::wrong_self_convention)]
    fn to_item(self) -> RetryItem {
        RetryItem {
            id: self.id,
            operation_type: self.operation_type,
            payload: self.payload,
            target: self.target,
            attempt_count: self.attempt_count as u32,
            max_retries: self.max_retries as u32,
            next_retry_at: parse_datetime(&self.next_retry_at),
            status: self.status.parse().unwrap_or(RetryStatus::Pending),
            last_error: self.last_error,
            created_at: parse_datetime(&self.created_at),
            updated_at: parse_datetime(&self.updated_at),
            correlation_id: self.correlation_id,
            user_id: self.user_id,
            tenant_id: self.tenant_id,
        }
    }
}

/// Row type for dead letter queue queries
#[derive(sqlx::FromRow)]
struct DlqRow {
    id: String,
    original_id: String,
    operation_type: String,
    payload: String,
    target: String,
    attempt_count: i32,
    last_error: Option<String>,
    created_at: String,
    failed_at: String,
    correlation_id: Option<String>,
    user_id: Option<String>,
    tenant_id: Option<String>,
}

impl DlqRow {
    #[allow(clippy::cast_sign_loss, clippy::wrong_self_convention)]
    fn to_item(self) -> DeadLetterItem {
        DeadLetterItem {
            id: self.id,
            original_id: self.original_id,
            operation_type: self.operation_type,
            payload: self.payload,
            target: self.target,
            attempt_count: self.attempt_count as u32,
            last_error: self.last_error,
            created_at: parse_datetime(&self.created_at),
            failed_at: parse_datetime(&self.failed_at),
            correlation_id: self.correlation_id,
            user_id: self.user_id,
            tenant_id: self.tenant_id,
        }
    }
}

/// A dead letter queue item
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeadLetterItem {
    /// Unique identifier
    pub id: String,
    /// Original retry item ID
    pub original_id: String,
    /// Type of operation
    pub operation_type: String,
    /// JSON payload
    pub payload: String,
    /// Target endpoint or recipient
    pub target: String,
    /// Total attempts made
    pub attempt_count: u32,
    /// Final error message
    pub last_error: Option<String>,
    /// Original creation time
    pub created_at: DateTime<Utc>,
    /// Time when moved to DLQ
    pub failed_at: DateTime<Utc>,
    /// Correlation ID for tracing
    pub correlation_id: Option<String>,
    /// User ID context
    pub user_id: Option<String>,
    /// Tenant ID context
    pub tenant_id: Option<String>,
}

/// Persistent retry queue store backed by SQLite (via sqlx)
#[derive(Clone)]
pub struct RetryQueueStore {
    pool: SqlitePool,
    retry_config: RetryConfig,
}

impl std::fmt::Debug for RetryQueueStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RetryQueueStore")
            .field("retry_config", &self.retry_config)
            .finish_non_exhaustive()
    }
}

impl RetryQueueStore {
    /// Create a new retry queue store with default retry config
    #[must_use]
    pub fn new(pool: SqlitePool) -> Self {
        Self {
            pool,
            retry_config: RetryConfig::default(),
        }
    }

    /// Create a new retry queue store with custom retry config
    #[must_use]
    pub fn with_config(pool: SqlitePool, retry_config: RetryConfig) -> Self {
        Self { pool, retry_config }
    }

    /// Enqueue a new item for retry
    #[instrument(skip(self), fields(operation = %item.operation_type, target = %item.target))]
    #[allow(clippy::cast_possible_wrap)]
    pub async fn enqueue(&self, item: RetryItem) -> Result<String, RetryQueueError> {
        sqlx::query(
            "INSERT INTO retry_queue (
                id, operation_type, payload, target, attempt_count, max_retries,
                next_retry_at, status, last_error, created_at, updated_at,
                correlation_id, user_id, tenant_id
            ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14)",
        )
        .bind(&item.id)
        .bind(&item.operation_type)
        .bind(&item.payload)
        .bind(&item.target)
        .bind(item.attempt_count as i32)
        .bind(item.max_retries as i32)
        .bind(item.next_retry_at.to_rfc3339())
        .bind(item.status.to_string())
        .bind(&item.last_error)
        .bind(item.created_at.to_rfc3339())
        .bind(item.updated_at.to_rfc3339())
        .bind(&item.correlation_id)
        .bind(&item.user_id)
        .bind(&item.tenant_id)
        .execute(&self.pool)
        .await?;

        info!(id = %item.id, "Enqueued item for retry");
        Ok(item.id)
    }

    /// Fetch items due for retry, marking them as in-progress
    #[instrument(skip(self))]
    #[allow(clippy::cast_possible_wrap)]
    pub async fn fetch_due_items(&self, limit: usize) -> Result<Vec<RetryItem>, RetryQueueError> {
        let now = Utc::now().to_rfc3339();

        let rows: Vec<RetryRow> = sqlx::query_as(
            "SELECT id, operation_type, payload, target, attempt_count, max_retries,
                    next_retry_at, status, last_error, created_at, updated_at,
                    correlation_id, user_id, tenant_id
             FROM retry_queue
             WHERE status = 'pending' AND next_retry_at <= $1
             ORDER BY next_retry_at ASC
             LIMIT $2",
        )
        .bind(&now)
        .bind(limit as i64)
        .fetch_all(&self.pool)
        .await?;

        let items: Vec<RetryItem> = rows.into_iter().map(RetryRow::to_item).collect();

        // Mark fetched items as in-progress
        if !items.is_empty() {
            let now_str = Utc::now().to_rfc3339();
            for item in &items {
                sqlx::query(
                    "UPDATE retry_queue SET status = 'in_progress', updated_at = $1 WHERE id = $2",
                )
                .bind(&now_str)
                .bind(&item.id)
                .execute(&self.pool)
                .await?;
            }
            debug!(count = items.len(), "Fetched due items for processing");
        }

        Ok(items)
    }

    /// Mark an item as completed and remove it from the queue
    #[instrument(skip(self))]
    pub async fn mark_completed(&self, id: &str) -> Result<(), RetryQueueError> {
        let result =
            sqlx::query("DELETE FROM retry_queue WHERE id = $1 AND status = 'in_progress'")
                .bind(id)
                .execute(&self.pool)
                .await?;

        if result.rows_affected() == 0 {
            return Err(RetryQueueError::NotFound(id.to_string()));
        }

        info!(%id, "Retry item completed successfully");
        Ok(())
    }

    /// Mark an item as failed, scheduling next retry or moving to DLQ
    #[instrument(skip(self), fields(error = %error_message))]
    pub async fn mark_failed(
        &self,
        id: &str,
        error_message: &str,
    ) -> Result<bool, RetryQueueError> {
        // Get current item state
        let row: RetryRow = sqlx::query_as(
            "SELECT id, operation_type, payload, target, attempt_count, max_retries,
                    next_retry_at, status, last_error, created_at, updated_at,
                    correlation_id, user_id, tenant_id
             FROM retry_queue WHERE id = $1",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| RetryQueueError::NotFound(id.to_string()))?;

        let item = row.to_item();
        let new_attempt_count = item.attempt_count + 1;
        let now = Utc::now();

        if new_attempt_count >= item.max_retries {
            // Move to dead letter queue
            self.move_to_dlq(&item, error_message).await?;
            warn!(
                %id,
                attempts = new_attempt_count,
                "Item exhausted retries, moved to dead letter queue"
            );
            return Ok(false);
        }

        // Calculate next retry time
        let delay = item.calculate_next_delay(&self.retry_config);
        let next_retry = now + chrono::Duration::from_std(delay).unwrap_or_default();

        #[allow(clippy::cast_possible_wrap)]
        let _ = sqlx::query(
            "UPDATE retry_queue SET
                status = 'pending',
                attempt_count = $1,
                next_retry_at = $2,
                last_error = $3,
                updated_at = $4
             WHERE id = $5",
        )
        .bind(new_attempt_count as i32)
        .bind(next_retry.to_rfc3339())
        .bind(error_message)
        .bind(now.to_rfc3339())
        .bind(id)
        .execute(&self.pool)
        .await?;

        debug!(
            %id,
            attempt = new_attempt_count,
            next_retry = %next_retry,
            "Scheduled next retry"
        );

        Ok(true)
    }

    /// Move an item to the dead letter queue
    async fn move_to_dlq(
        &self,
        item: &RetryItem,
        error_message: &str,
    ) -> Result<(), RetryQueueError> {
        let now = Utc::now();

        #[allow(clippy::cast_possible_wrap)]
        let _dlq_result = sqlx::query(
            "INSERT INTO retry_dead_letter (
                id, original_id, operation_type, payload, target, attempt_count,
                final_error, failed_at, created_at, correlation_id, user_id, tenant_id
            ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)",
        )
        .bind(Uuid::new_v4().to_string())
        .bind(&item.id)
        .bind(&item.operation_type)
        .bind(&item.payload)
        .bind(&item.target)
        .bind((item.attempt_count + 1) as i32)
        .bind(error_message)
        .bind(item.created_at.to_rfc3339())
        .bind(now.to_rfc3339())
        .bind(&item.correlation_id)
        .bind(&item.user_id)
        .bind(&item.tenant_id)
        .execute(&self.pool)
        .await?;

        sqlx::query("DELETE FROM retry_queue WHERE id = $1")
            .bind(&item.id)
            .execute(&self.pool)
            .await?;

        Ok(())
    }

    /// Cancel a pending retry item
    #[instrument(skip(self))]
    pub async fn cancel(&self, id: &str) -> Result<(), RetryQueueError> {
        let result = sqlx::query(
            "UPDATE retry_queue SET status = 'cancelled', updated_at = $1 \
             WHERE id = $2 AND status IN ('pending', 'in_progress')",
        )
        .bind(Utc::now().to_rfc3339())
        .bind(id)
        .execute(&self.pool)
        .await?;

        if result.rows_affected() == 0 {
            return Err(RetryQueueError::NotFound(id.to_string()));
        }

        info!(%id, "Retry item cancelled");
        Ok(())
    }

    /// Get queue statistics
    pub async fn get_stats(&self) -> Result<QueueStats, RetryQueueError> {
        let pending: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM retry_queue WHERE status = 'pending'")
                .fetch_one(&self.pool)
                .await?;

        let in_progress: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM retry_queue WHERE status = 'in_progress'")
                .fetch_one(&self.pool)
                .await?;

        let dead_letter: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM dead_letter_queue")
            .fetch_one(&self.pool)
            .await?;

        #[allow(clippy::cast_sign_loss)]
        Ok(QueueStats {
            pending: pending as u64,
            in_progress: in_progress as u64,
            dead_letter: dead_letter as u64,
        })
    }

    /// Get items from the dead letter queue
    #[allow(clippy::cast_possible_wrap)]
    pub async fn get_dead_letter_items(
        &self,
        limit: usize,
    ) -> Result<Vec<DeadLetterItem>, RetryQueueError> {
        let rows: Vec<DlqRow> = sqlx::query_as(
            "SELECT id, original_id, operation_type, payload, target, attempt_count,
                    last_error, created_at, failed_at, correlation_id, user_id, tenant_id
             FROM dead_letter_queue
             ORDER BY failed_at DESC
             LIMIT $1",
        )
        .bind(limit as i64)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows.into_iter().map(DlqRow::to_item).collect())
    }

    /// Requeue an item from the dead letter queue
    #[instrument(skip(self))]
    pub async fn requeue_from_dlq(&self, dlq_id: &str) -> Result<String, RetryQueueError> {
        let row: DlqRow = sqlx::query_as(
            "SELECT id, original_id, operation_type, payload, target, attempt_count,
                    last_error, created_at, failed_at, correlation_id, user_id, tenant_id
             FROM dead_letter_queue WHERE id = $1",
        )
        .bind(dlq_id)
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| RetryQueueError::NotFound(dlq_id.to_string()))?;

        let dlq_item = row.to_item();
        let now = Utc::now();
        let new_id = Uuid::new_v4().to_string();

        sqlx::query(
            "INSERT INTO retry_queue (
                id, operation_type, payload, target, attempt_count, max_retries,
                next_retry_at, status, last_error, created_at, updated_at,
                correlation_id, user_id, tenant_id
            ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14)",
        )
        .bind(&new_id)
        .bind(&dlq_item.operation_type)
        .bind(&dlq_item.payload)
        .bind(&dlq_item.target)
        .bind(0_i32)
        .bind(5_i32)
        .bind(now.to_rfc3339())
        .bind("pending")
        .bind(Option::<String>::None)
        .bind(now.to_rfc3339())
        .bind(now.to_rfc3339())
        .bind(&dlq_item.correlation_id)
        .bind(&dlq_item.user_id)
        .bind(&dlq_item.tenant_id)
        .execute(&self.pool)
        .await?;

        sqlx::query("DELETE FROM dead_letter_queue WHERE id = $1")
            .bind(dlq_id)
            .execute(&self.pool)
            .await?;

        info!(dlq_id = %dlq_id, new_id = %new_id, "Requeued item from DLQ");
        Ok(new_id)
    }

    /// Clean up old completed/cancelled items older than retention period
    pub async fn cleanup_old_items(&self, retention_days: u32) -> Result<u64, RetryQueueError> {
        let cutoff = Utc::now() - chrono::Duration::days(i64::from(retention_days));

        let result = sqlx::query(
            "DELETE FROM retry_queue \
             WHERE status IN ('completed', 'cancelled') AND updated_at < $1",
        )
        .bind(cutoff.to_rfc3339())
        .execute(&self.pool)
        .await?;

        let deleted = result.rows_affected();
        if deleted > 0 {
            info!(deleted, "Cleaned up old retry queue items");
        }

        Ok(deleted)
    }
}

/// Queue statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueueStats {
    /// Number of pending items
    pub pending: u64,
    /// Number of items currently being processed
    pub in_progress: u64,
    /// Number of items in dead letter queue
    pub dead_letter: u64,
}

/// Parse ISO8601 datetime string
fn parse_datetime(s: &str) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(s).map_or_else(|_| Utc::now(), |dt| dt.with_timezone(&Utc))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::persistence::async_connection::AsyncDatabase;

    async fn setup() -> (AsyncDatabase, RetryQueueStore) {
        let db = AsyncDatabase::in_memory().await.unwrap();
        db.migrate().await.unwrap();
        let store = RetryQueueStore::new(db.pool().clone());
        (db, store)
    }

    #[tokio::test]
    async fn enqueue_and_fetch() {
        let (_db, store) = setup().await;

        let item = RetryItem::new("webhook", r#"{"test": true}"#, "https://example.com/hook")
            .with_correlation_id("test-123");

        let id = store.enqueue(item).await.unwrap();

        let due = store.fetch_due_items(10).await.unwrap();
        assert_eq!(due.len(), 1);
        assert_eq!(due[0].id, id);
        assert_eq!(due[0].operation_type, "webhook");
    }

    #[tokio::test]
    async fn mark_completed() {
        let (_db, store) = setup().await;

        let item = RetryItem::new("webhook", "{}", "https://example.com");
        let id = store.enqueue(item).await.unwrap();

        let _ = store.fetch_due_items(10).await.unwrap();
        store.mark_completed(&id).await.unwrap();

        let stats = store.get_stats().await.unwrap();
        assert_eq!(stats.pending, 0);
        assert_eq!(stats.in_progress, 0);
    }

    #[tokio::test]
    async fn retry_backoff() {
        let (_db, store) = setup().await;
        // Override config for deterministic test
        let store = RetryQueueStore::with_config(
            store.pool.clone(),
            RetryConfig {
                initial_delay_ms: 100,
                max_delay_ms: 10000,
                multiplier: 2.0,
                max_retries: 5,
                jitter_enabled: false,
                jitter_factor: 0.0,
            },
        );

        let item = RetryItem::new("webhook", "{}", "https://example.com").with_max_retries(3);
        let id = store.enqueue(item).await.unwrap();

        let _ = store.fetch_due_items(10).await.unwrap();
        let will_retry = store.mark_failed(&id, "Connection refused").await.unwrap();
        assert!(will_retry);

        let stats = store.get_stats().await.unwrap();
        assert_eq!(stats.pending, 1);
        assert_eq!(stats.dead_letter, 0);
    }

    #[tokio::test]
    async fn dead_letter_queue() {
        let (_db, store) = setup().await;

        let item = RetryItem::new("webhook", "{}", "https://example.com").with_max_retries(1);
        let id = store.enqueue(item).await.unwrap();

        let _ = store.fetch_due_items(10).await.unwrap();
        let will_retry = store.mark_failed(&id, "Connection refused").await.unwrap();
        assert!(!will_retry);

        let stats = store.get_stats().await.unwrap();
        assert_eq!(stats.pending, 0);
        assert_eq!(stats.dead_letter, 1);

        let dlq_items = store.get_dead_letter_items(10).await.unwrap();
        assert_eq!(dlq_items.len(), 1);
        assert_eq!(dlq_items[0].original_id, id);
    }

    #[tokio::test]
    async fn requeue_from_dlq() {
        let (_db, store) = setup().await;

        let item = RetryItem::new("webhook", "{}", "https://example.com").with_max_retries(1);
        let id = store.enqueue(item).await.unwrap();

        let _ = store.fetch_due_items(10).await.unwrap();
        store.mark_failed(&id, "Error").await.unwrap();

        let dlq_items = store.get_dead_letter_items(10).await.unwrap();
        let dlq_id = &dlq_items[0].id;

        let new_id = store.requeue_from_dlq(dlq_id).await.unwrap();
        assert_ne!(new_id, id);

        let stats = store.get_stats().await.unwrap();
        assert_eq!(stats.pending, 1);
        assert_eq!(stats.dead_letter, 0);
    }

    #[tokio::test]
    async fn cancel_item() {
        let (_db, store) = setup().await;

        let item = RetryItem::new("webhook", "{}", "https://example.com");
        let id = store.enqueue(item).await.unwrap();

        store.cancel(&id).await.unwrap();

        let due = store.fetch_due_items(10).await.unwrap();
        assert!(due.is_empty());
    }

    #[test]
    fn retry_status_display() {
        assert_eq!(RetryStatus::Pending.to_string(), "pending");
        assert_eq!(RetryStatus::InProgress.to_string(), "in_progress");
        assert_eq!(RetryStatus::Completed.to_string(), "completed");
        assert_eq!(RetryStatus::Failed.to_string(), "failed");
        assert_eq!(RetryStatus::Cancelled.to_string(), "cancelled");
    }

    #[test]
    fn retry_status_parse() {
        assert_eq!(
            "pending".parse::<RetryStatus>().unwrap(),
            RetryStatus::Pending
        );
        assert_eq!(
            "in_progress".parse::<RetryStatus>().unwrap(),
            RetryStatus::InProgress
        );
        assert!("invalid".parse::<RetryStatus>().is_err());
    }

    #[test]
    fn retry_queue_store_is_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<RetryQueueStore>();
    }
}
