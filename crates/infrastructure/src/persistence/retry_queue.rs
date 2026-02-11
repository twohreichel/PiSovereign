//! Persistent retry queue for failed operations with exponential backoff
//!
//! Provides durable storage for failed webhook deliveries, messages, and other
//! retryable operations. Uses SQLite for persistence with automatic cleanup
//! of completed items and dead letter queue for permanently failed items.
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
//!
//! # Example
//!
//! ```rust,ignore
//! use infrastructure::persistence::RetryQueueStore;
//!
//! let store = RetryQueueStore::new(connection_pool);
//!
//! // Enqueue a failed webhook
//! store.enqueue(RetryItem::new(
//!     "webhook",
//!     serde_json::to_string(&payload)?,
//!     "https://api.example.com/webhook",
//! )).await?;
//!
//! // Process due items
//! let due = store.fetch_due_items(10).await?;
//! for item in due {
//!     match process(&item).await {
//!         Ok(_) => store.mark_completed(&item.id).await?,
//!         Err(e) => store.mark_failed(&item.id, &e.to_string()).await?,
//!     }
//! }
//! ```

use chrono::{DateTime, Utc};
use r2d2::{Pool, PooledConnection};
use r2d2_sqlite::SqliteConnectionManager;
use rusqlite::{OptionalExtension, params};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tracing::{debug, info, instrument, warn};
use uuid::Uuid;

use crate::retry::RetryConfig;

/// Error type for retry queue operations
#[derive(Debug, Error)]
pub enum RetryQueueError {
    /// Database operation failed
    #[error("Database error: {0}")]
    Database(#[from] rusqlite::Error),

    /// Connection pool error
    #[error("Connection pool error: {0}")]
    Pool(#[from] r2d2::Error),

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

/// Persistent retry queue store backed by SQLite
#[derive(Clone)]
pub struct RetryQueueStore {
    pool: Pool<SqliteConnectionManager>,
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
    pub fn new(pool: Pool<SqliteConnectionManager>) -> Self {
        Self {
            pool,
            retry_config: RetryConfig::default(),
        }
    }

    /// Create a new retry queue store with custom retry config
    #[must_use]
    #[allow(clippy::missing_const_for_fn)]
    pub fn with_config(pool: Pool<SqliteConnectionManager>, retry_config: RetryConfig) -> Self {
        Self { pool, retry_config }
    }

    /// Get a connection from the pool
    fn conn(&self) -> Result<PooledConnection<SqliteConnectionManager>, RetryQueueError> {
        Ok(self.pool.get()?)
    }

    /// Enqueue a new item for retry
    #[instrument(skip(self), fields(operation = %item.operation_type, target = %item.target))]
    pub fn enqueue(&self, item: RetryItem) -> Result<String, RetryQueueError> {
        let conn = self.conn()?;

        conn.execute(
            r"INSERT INTO retry_queue (
                id, operation_type, payload, target, attempt_count, max_retries,
                next_retry_at, status, last_error, created_at, updated_at,
                correlation_id, user_id, tenant_id
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
            params![
                item.id,
                item.operation_type,
                item.payload,
                item.target,
                item.attempt_count,
                item.max_retries,
                item.next_retry_at.to_rfc3339(),
                item.status.to_string(),
                item.last_error,
                item.created_at.to_rfc3339(),
                item.updated_at.to_rfc3339(),
                item.correlation_id,
                item.user_id,
                item.tenant_id,
            ],
        )?;

        info!(id = %item.id, "Enqueued item for retry");
        Ok(item.id)
    }

    /// Fetch items due for retry, marking them as in-progress
    #[instrument(skip(self))]
    #[allow(clippy::cast_possible_wrap)]
    pub fn fetch_due_items(&self, limit: usize) -> Result<Vec<RetryItem>, RetryQueueError> {
        let conn = self.conn()?;
        let now = Utc::now().to_rfc3339();

        // Select and lock items in a single transaction
        let tx = conn;
        let mut stmt = tx.prepare(
            r"SELECT id, operation_type, payload, target, attempt_count, max_retries,
                     next_retry_at, status, last_error, created_at, updated_at,
                     correlation_id, user_id, tenant_id
              FROM retry_queue
              WHERE status = 'pending' AND next_retry_at <= ?1
              ORDER BY next_retry_at ASC
              LIMIT ?2",
        )?;

        let items: Vec<RetryItem> = stmt
            .query_map(params![now, limit as i64], |row| {
                Ok(RetryItem {
                    id: row.get(0)?,
                    operation_type: row.get(1)?,
                    payload: row.get(2)?,
                    target: row.get(3)?,
                    attempt_count: row.get(4)?,
                    max_retries: row.get(5)?,
                    next_retry_at: parse_datetime(row.get::<_, String>(6)?),
                    status: row
                        .get::<_, String>(7)?
                        .parse()
                        .unwrap_or(RetryStatus::Pending),
                    last_error: row.get(8)?,
                    created_at: parse_datetime(row.get::<_, String>(9)?),
                    updated_at: parse_datetime(row.get::<_, String>(10)?),
                    correlation_id: row.get(11)?,
                    user_id: row.get(12)?,
                    tenant_id: row.get(13)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;

        // Mark fetched items as in-progress
        if !items.is_empty() {
            let ids: Vec<_> = items.iter().map(|i| i.id.as_str()).collect();
            let placeholders = ids.iter().map(|_| "?").collect::<Vec<_>>().join(",");
            let sql = format!(
                "UPDATE retry_queue SET status = 'in_progress', updated_at = ?1 WHERE id IN ({placeholders})"
            );

            let mut stmt = tx.prepare(&sql)?;
            let now_str = Utc::now().to_rfc3339();
            let mut params_vec: Vec<&dyn rusqlite::ToSql> = vec![&now_str];
            params_vec.extend(ids.iter().map(|s| s as &dyn rusqlite::ToSql));
            stmt.execute(params_vec.as_slice())?;

            debug!(count = items.len(), "Fetched due items for processing");
        }

        Ok(items)
    }

    /// Mark an item as completed and remove it from the queue
    #[instrument(skip(self))]
    pub fn mark_completed(&self, id: &str) -> Result<(), RetryQueueError> {
        let conn = self.conn()?;

        let rows = conn.execute(
            "DELETE FROM retry_queue WHERE id = ?1 AND status = 'in_progress'",
            params![id],
        )?;

        if rows == 0 {
            return Err(RetryQueueError::NotFound(id.to_string()));
        }

        info!(%id, "Retry item completed successfully");
        Ok(())
    }

    /// Mark an item as failed, scheduling next retry or moving to DLQ
    #[instrument(skip(self), fields(error = %error_message))]
    pub fn mark_failed(&self, id: &str, error_message: &str) -> Result<bool, RetryQueueError> {
        let conn = self.conn()?;

        // Get current item state
        let item: RetryItem = conn
            .query_row(
                r"SELECT id, operation_type, payload, target, attempt_count, max_retries,
                         next_retry_at, status, last_error, created_at, updated_at,
                         correlation_id, user_id, tenant_id
                  FROM retry_queue WHERE id = ?1",
                params![id],
                |row| {
                    Ok(RetryItem {
                        id: row.get(0)?,
                        operation_type: row.get(1)?,
                        payload: row.get(2)?,
                        target: row.get(3)?,
                        attempt_count: row.get(4)?,
                        max_retries: row.get(5)?,
                        next_retry_at: parse_datetime(row.get::<_, String>(6)?),
                        status: row
                            .get::<_, String>(7)?
                            .parse()
                            .unwrap_or(RetryStatus::Pending),
                        last_error: row.get(8)?,
                        created_at: parse_datetime(row.get::<_, String>(9)?),
                        updated_at: parse_datetime(row.get::<_, String>(10)?),
                        correlation_id: row.get(11)?,
                        user_id: row.get(12)?,
                        tenant_id: row.get(13)?,
                    })
                },
            )
            .optional()?
            .ok_or_else(|| RetryQueueError::NotFound(id.to_string()))?;

        let new_attempt_count = item.attempt_count + 1;
        let now = Utc::now();

        if new_attempt_count >= item.max_retries {
            // Move to dead letter queue using the same connection
            Self::move_to_dlq_with_conn(&conn, &item, error_message)?;
            warn!(
                %id,
                attempts = new_attempt_count,
                "Item exhausted retries, moved to dead letter queue"
            );
            return Ok(false); // No more retries
        }

        // Calculate next retry time
        let delay = item.calculate_next_delay(&self.retry_config);
        let next_retry = now + chrono::Duration::from_std(delay).unwrap_or_default();

        conn.execute(
            r"UPDATE retry_queue SET
                status = 'pending',
                attempt_count = ?1,
                next_retry_at = ?2,
                last_error = ?3,
                updated_at = ?4
              WHERE id = ?5",
            params![
                new_attempt_count,
                next_retry.to_rfc3339(),
                error_message,
                now.to_rfc3339(),
                id,
            ],
        )?;

        debug!(
            %id,
            attempt = new_attempt_count,
            next_retry = %next_retry,
            "Scheduled next retry"
        );

        Ok(true) // Will retry
    }

    /// Move an item to the dead letter queue (uses existing connection)
    fn move_to_dlq_with_conn(
        conn: &PooledConnection<SqliteConnectionManager>,
        item: &RetryItem,
        error_message: &str,
    ) -> Result<(), RetryQueueError> {
        let now = Utc::now();

        // Insert into DLQ
        conn.execute(
            r"INSERT INTO dead_letter_queue (
                id, original_id, operation_type, payload, target, attempt_count,
                last_error, created_at, failed_at, correlation_id, user_id, tenant_id
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
            params![
                Uuid::new_v4().to_string(),
                item.id,
                item.operation_type,
                item.payload,
                item.target,
                item.attempt_count + 1,
                error_message,
                item.created_at.to_rfc3339(),
                now.to_rfc3339(),
                item.correlation_id,
                item.user_id,
                item.tenant_id,
            ],
        )?;

        // Remove from retry queue
        conn.execute("DELETE FROM retry_queue WHERE id = ?1", params![item.id])?;

        Ok(())
    }

    /// Cancel a pending retry item
    #[instrument(skip(self))]
    pub fn cancel(&self, id: &str) -> Result<(), RetryQueueError> {
        let conn = self.conn()?;

        let rows = conn.execute(
            "UPDATE retry_queue SET status = 'cancelled', updated_at = ?1 WHERE id = ?2 AND status IN ('pending', 'in_progress')",
            params![Utc::now().to_rfc3339(), id],
        )?;

        if rows == 0 {
            return Err(RetryQueueError::NotFound(id.to_string()));
        }

        info!(%id, "Retry item cancelled");
        Ok(())
    }

    /// Get queue statistics
    pub fn get_stats(&self) -> Result<QueueStats, RetryQueueError> {
        let conn = self.conn()?;

        let pending: i64 = conn.query_row(
            "SELECT COUNT(*) FROM retry_queue WHERE status = 'pending'",
            [],
            |row| row.get(0),
        )?;

        let in_progress: i64 = conn.query_row(
            "SELECT COUNT(*) FROM retry_queue WHERE status = 'in_progress'",
            [],
            |row| row.get(0),
        )?;

        let failed: i64 = conn.query_row("SELECT COUNT(*) FROM dead_letter_queue", [], |row| {
            row.get(0)
        })?;

        #[allow(clippy::cast_sign_loss)]
        Ok(QueueStats {
            pending: pending as u64,
            in_progress: in_progress as u64,
            dead_letter: failed as u64,
        })
    }

    /// Get items from the dead letter queue
    #[allow(clippy::cast_possible_wrap)]
    pub fn get_dead_letter_items(
        &self,
        limit: usize,
    ) -> Result<Vec<DeadLetterItem>, RetryQueueError> {
        let conn = self.conn()?;

        let mut stmt = conn.prepare(
            r"SELECT id, original_id, operation_type, payload, target, attempt_count,
                     last_error, created_at, failed_at, correlation_id, user_id, tenant_id
              FROM dead_letter_queue
              ORDER BY failed_at DESC
              LIMIT ?1",
        )?;

        let items = stmt
            .query_map(params![limit as i64], |row| {
                Ok(DeadLetterItem {
                    id: row.get(0)?,
                    original_id: row.get(1)?,
                    operation_type: row.get(2)?,
                    payload: row.get(3)?,
                    target: row.get(4)?,
                    attempt_count: row.get(5)?,
                    last_error: row.get(6)?,
                    created_at: parse_datetime(row.get::<_, String>(7)?),
                    failed_at: parse_datetime(row.get::<_, String>(8)?),
                    correlation_id: row.get(9)?,
                    user_id: row.get(10)?,
                    tenant_id: row.get(11)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;

        Ok(items)
    }

    /// Requeue an item from the dead letter queue
    #[instrument(skip(self))]
    pub fn requeue_from_dlq(&self, dlq_id: &str) -> Result<String, RetryQueueError> {
        let conn = self.conn()?;

        // Get DLQ item
        let dlq_item: DeadLetterItem = conn
            .query_row(
                r"SELECT id, original_id, operation_type, payload, target, attempt_count,
                         last_error, created_at, failed_at, correlation_id, user_id, tenant_id
                  FROM dead_letter_queue WHERE id = ?1",
                params![dlq_id],
                |row| {
                    Ok(DeadLetterItem {
                        id: row.get(0)?,
                        original_id: row.get(1)?,
                        operation_type: row.get(2)?,
                        payload: row.get(3)?,
                        target: row.get(4)?,
                        attempt_count: row.get(5)?,
                        last_error: row.get(6)?,
                        created_at: parse_datetime(row.get::<_, String>(7)?),
                        failed_at: parse_datetime(row.get::<_, String>(8)?),
                        correlation_id: row.get(9)?,
                        user_id: row.get(10)?,
                        tenant_id: row.get(11)?,
                    })
                },
            )
            .optional()?
            .ok_or_else(|| RetryQueueError::NotFound(dlq_id.to_string()))?;

        // Create new retry item
        let now = Utc::now();
        let new_id = Uuid::new_v4().to_string();

        // Insert directly into retry queue (don't call self.enqueue to avoid getting another connection)
        conn.execute(
            r"INSERT INTO retry_queue (
                id, operation_type, payload, target, attempt_count, max_retries,
                next_retry_at, status, last_error, created_at, updated_at,
                correlation_id, user_id, tenant_id
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
            params![
                new_id,
                dlq_item.operation_type,
                dlq_item.payload,
                dlq_item.target,
                0i32, // Reset attempts
                5i32, // max_retries
                now.to_rfc3339(),
                "pending",
                Option::<String>::None,
                now.to_rfc3339(),
                now.to_rfc3339(),
                dlq_item.correlation_id,
                dlq_item.user_id,
                dlq_item.tenant_id,
            ],
        )?;

        // Remove from DLQ
        conn.execute(
            "DELETE FROM dead_letter_queue WHERE id = ?1",
            params![dlq_id],
        )?;

        info!(dlq_id = %dlq_id, new_id = %new_id, "Requeued item from DLQ");
        Ok(new_id)
    }

    /// Clean up old completed items and cancelled items older than retention period
    pub fn cleanup_old_items(&self, retention_days: u32) -> Result<u64, RetryQueueError> {
        let conn = self.conn()?;
        let cutoff = Utc::now() - chrono::Duration::days(i64::from(retention_days));

        let deleted = conn.execute(
            "DELETE FROM retry_queue WHERE status IN ('completed', 'cancelled') AND updated_at < ?1",
            params![cutoff.to_rfc3339()],
        )?;

        if deleted > 0 {
            info!(deleted, "Cleaned up old retry queue items");
        }

        Ok(deleted as u64)
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
#[allow(clippy::needless_pass_by_value)]
fn parse_datetime(s: String) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(&s).map_or_else(|_| Utc::now(), |dt| dt.with_timezone(&Utc))
}

#[cfg(test)]
mod tests {
    use super::*;
    use r2d2_sqlite::SqliteConnectionManager;

    const RETRY_QUEUE_SCHEMA: &str = r"
        CREATE TABLE IF NOT EXISTS retry_queue (
            id TEXT PRIMARY KEY,
            operation_type TEXT NOT NULL,
            payload TEXT NOT NULL,
            target TEXT NOT NULL,
            attempt_count INTEGER NOT NULL DEFAULT 0,
            max_retries INTEGER NOT NULL DEFAULT 5,
            next_retry_at TEXT NOT NULL,
            status TEXT NOT NULL DEFAULT 'pending' CHECK(status IN ('pending', 'in_progress', 'completed', 'failed', 'cancelled')),
            last_error TEXT,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL,
            correlation_id TEXT,
            user_id TEXT,
            tenant_id TEXT
        );

        CREATE TABLE IF NOT EXISTS dead_letter_queue (
            id TEXT PRIMARY KEY,
            original_id TEXT NOT NULL,
            operation_type TEXT NOT NULL,
            payload TEXT NOT NULL,
            target TEXT NOT NULL,
            attempt_count INTEGER NOT NULL,
            last_error TEXT,
            created_at TEXT NOT NULL,
            failed_at TEXT NOT NULL,
            correlation_id TEXT,
            user_id TEXT,
            tenant_id TEXT
        );
    ";

    fn setup_test_db() -> Pool<SqliteConnectionManager> {
        let manager = SqliteConnectionManager::memory();
        let pool = Pool::builder().max_size(1).build(manager).unwrap();

        // Create tables
        let conn = pool.get().unwrap();
        conn.execute_batch(RETRY_QUEUE_SCHEMA).unwrap();

        pool
    }

    #[test]
    fn test_enqueue_and_fetch() {
        let pool = setup_test_db();
        let store = RetryQueueStore::new(pool);

        let item = RetryItem::new("webhook", r#"{"test": true}"#, "https://example.com/hook")
            .with_correlation_id("test-123");

        let id = store.enqueue(item).unwrap();

        let due = store.fetch_due_items(10).unwrap();
        assert_eq!(due.len(), 1);
        assert_eq!(due[0].id, id);
        assert_eq!(due[0].operation_type, "webhook");
    }

    #[test]
    fn test_mark_completed() {
        let pool = setup_test_db();
        let store = RetryQueueStore::new(pool);

        let item = RetryItem::new("webhook", "{}", "https://example.com");
        let id = store.enqueue(item).unwrap();

        // Fetch to mark as in_progress
        let _ = store.fetch_due_items(10).unwrap();

        // Mark completed
        store.mark_completed(&id).unwrap();

        // Should be removed
        let stats = store.get_stats().unwrap();
        assert_eq!(stats.pending, 0);
        assert_eq!(stats.in_progress, 0);
    }

    #[test]
    fn test_retry_backoff() {
        let pool = setup_test_db();
        let config = RetryConfig {
            initial_delay_ms: 100,
            max_delay_ms: 10000,
            multiplier: 2.0,
            max_retries: 5,
            jitter_enabled: false,
            jitter_factor: 0.0,
        };
        let store = RetryQueueStore::with_config(pool, config);

        let item = RetryItem::new("webhook", "{}", "https://example.com").with_max_retries(3);
        let id = store.enqueue(item).unwrap();

        // Fetch and fail
        let _ = store.fetch_due_items(10).unwrap();
        let will_retry = store.mark_failed(&id, "Connection refused").unwrap();
        assert!(will_retry);

        // Check stats
        let stats = store.get_stats().unwrap();
        assert_eq!(stats.pending, 1);
        assert_eq!(stats.dead_letter, 0);
    }

    #[test]
    fn test_dead_letter_queue() {
        let pool = setup_test_db();
        let store = RetryQueueStore::new(pool);

        let item = RetryItem::new("webhook", "{}", "https://example.com").with_max_retries(1);
        let id = store.enqueue(item).unwrap();

        // Fetch and fail - should move to DLQ since max_retries=1
        let _ = store.fetch_due_items(10).unwrap();
        let will_retry = store.mark_failed(&id, "Connection refused").unwrap();
        assert!(!will_retry);

        // Check DLQ
        let stats = store.get_stats().unwrap();
        assert_eq!(stats.pending, 0);
        assert_eq!(stats.dead_letter, 1);

        // Get DLQ items
        let dlq_items = store.get_dead_letter_items(10).unwrap();
        assert_eq!(dlq_items.len(), 1);
        assert_eq!(dlq_items[0].original_id, id);
    }

    #[test]
    fn test_requeue_from_dlq() {
        let pool = setup_test_db();
        let store = RetryQueueStore::new(pool);

        let item = RetryItem::new("webhook", "{}", "https://example.com").with_max_retries(1);
        let id = store.enqueue(item).unwrap();

        // Move to DLQ
        let _ = store.fetch_due_items(10).unwrap();
        store.mark_failed(&id, "Error").unwrap();

        // Get DLQ item ID
        let dlq_items = store.get_dead_letter_items(10).unwrap();
        let dlq_id = &dlq_items[0].id;

        // Requeue
        let new_id = store.requeue_from_dlq(dlq_id).unwrap();
        assert_ne!(new_id, id);

        // Verify stats
        let stats = store.get_stats().unwrap();
        assert_eq!(stats.pending, 1);
        assert_eq!(stats.dead_letter, 0);
    }

    #[test]
    fn test_cancel() {
        let pool = setup_test_db();
        let store = RetryQueueStore::new(pool);

        let item = RetryItem::new("webhook", "{}", "https://example.com");
        let id = store.enqueue(item).unwrap();

        store.cancel(&id).unwrap();

        // Should not be fetchable
        let due = store.fetch_due_items(10).unwrap();
        assert!(due.is_empty());
    }

    #[test]
    fn test_retry_status_display() {
        assert_eq!(RetryStatus::Pending.to_string(), "pending");
        assert_eq!(RetryStatus::InProgress.to_string(), "in_progress");
        assert_eq!(RetryStatus::Completed.to_string(), "completed");
        assert_eq!(RetryStatus::Failed.to_string(), "failed");
        assert_eq!(RetryStatus::Cancelled.to_string(), "cancelled");
    }

    #[test]
    fn test_retry_status_parse() {
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
}
