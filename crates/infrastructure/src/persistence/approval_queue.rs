//! SQLite adapter for the ApprovalQueue port
//!
//! Persists approval requests to SQLite for durability across restarts.

use std::sync::Arc;

use application::{error::ApplicationError, ports::ApprovalQueuePort};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use domain::{AgentCommand, ApprovalId, ApprovalRequest, ApprovalStatus, UserId};
use r2d2::Pool;
use r2d2_sqlite::SqliteConnectionManager;
use rusqlite::params;

/// SQLite implementation of the approval queue
#[derive(Debug, Clone)]
pub struct SqliteApprovalQueue {
    pool: Arc<Pool<SqliteConnectionManager>>,
}

impl SqliteApprovalQueue {
    /// Create a new SQLite approval queue
    pub const fn new(pool: Arc<Pool<SqliteConnectionManager>>) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl ApprovalQueuePort for SqliteApprovalQueue {
    async fn enqueue(&self, request: &ApprovalRequest) -> Result<(), ApplicationError> {
        let pool = Arc::clone(&self.pool);
        let request = request.clone();

        tokio::task::spawn_blocking(move || {
            let conn = pool.get().map_err(|e| {
                ApplicationError::Internal(format!("Failed to get database connection: {e}"))
            })?;

            let command_json = serde_json::to_string(&request.command)
                .map_err(|e| ApplicationError::Internal(format!("Failed to serialize command: {e}")))?;

            let reason_json = request
                .reason
                .as_ref()
                .map(serde_json::to_string)
                .transpose()
                .map_err(|e| ApplicationError::Internal(format!("Failed to serialize reason: {e}")))?;

            conn.execute(
                "INSERT INTO approval_requests 
                 (id, user_id, command, description, status, created_at, expires_at, updated_at, reason)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                params![
                    request.id.to_string(),
                    request.user_id.to_string(),
                    command_json,
                    request.description,
                    status_to_str(request.status),
                    request.created_at.to_rfc3339(),
                    request.expires_at.to_rfc3339(),
                    request.updated_at.to_rfc3339(),
                    reason_json,
                ],
            )
            .map_err(|e| ApplicationError::Internal(format!("Failed to insert approval request: {e}")))?;

            Ok(())
        })
        .await
        .map_err(|e| ApplicationError::Internal(format!("Task join error: {e}")))?
    }

    async fn get(&self, id: &ApprovalId) -> Result<Option<ApprovalRequest>, ApplicationError> {
        let pool = Arc::clone(&self.pool);
        let id = *id;

        tokio::task::spawn_blocking(move || {
            let conn = pool.get().map_err(|e| {
                ApplicationError::Internal(format!("Failed to get database connection: {e}"))
            })?;

            let mut stmt = conn
                .prepare(
                    "SELECT id, user_id, command, description, status, created_at, expires_at, updated_at, reason
                     FROM approval_requests WHERE id = ?1",
                )
                .map_err(|e| ApplicationError::Internal(format!("Failed to prepare statement: {e}")))?;

            let result = stmt
                .query_row([id.to_string()], row_to_approval_request)
                .map_or_else(
                    |e| {
                        if e == rusqlite::Error::QueryReturnedNoRows {
                            Ok(None)
                        } else {
                            Err(ApplicationError::Internal(format!(
                                "Failed to get approval request: {e}"
                            )))
                        }
                    },
                    |req| Ok(Some(req)),
                )?;

            Ok(result)
        })
        .await
        .map_err(|e| ApplicationError::Internal(format!("Task join error: {e}")))?
    }

    async fn update(&self, request: &ApprovalRequest) -> Result<(), ApplicationError> {
        let pool = Arc::clone(&self.pool);
        let request = request.clone();

        tokio::task::spawn_blocking(move || {
            let conn = pool.get().map_err(|e| {
                ApplicationError::Internal(format!("Failed to get database connection: {e}"))
            })?;

            let reason_json = request
                .reason
                .as_ref()
                .map(serde_json::to_string)
                .transpose()
                .map_err(|e| {
                    ApplicationError::Internal(format!("Failed to serialize reason: {e}"))
                })?;

            conn.execute(
                "UPDATE approval_requests 
                 SET status = ?1, updated_at = ?2, reason = ?3
                 WHERE id = ?4",
                params![
                    status_to_str(request.status),
                    request.updated_at.to_rfc3339(),
                    reason_json,
                    request.id.to_string(),
                ],
            )
            .map_err(|e| {
                ApplicationError::Internal(format!("Failed to update approval request: {e}"))
            })?;

            Ok(())
        })
        .await
        .map_err(|e| ApplicationError::Internal(format!("Task join error: {e}")))?
    }

    async fn get_pending_for_user(
        &self,
        user_id: &UserId,
    ) -> Result<Vec<ApprovalRequest>, ApplicationError> {
        let pool = Arc::clone(&self.pool);
        let user_id = *user_id;

        tokio::task::spawn_blocking(move || {
            let conn = pool.get().map_err(|e| {
                ApplicationError::Internal(format!("Failed to get database connection: {e}"))
            })?;

            let mut stmt = conn
                .prepare(
                    "SELECT id, user_id, command, description, status, created_at, expires_at, updated_at, reason
                     FROM approval_requests 
                     WHERE user_id = ?1 AND status = 'pending'
                     ORDER BY created_at DESC",
                )
                .map_err(|e| ApplicationError::Internal(format!("Failed to prepare statement: {e}")))?;

            let requests = stmt
                .query_map([user_id.to_string()], row_to_approval_request)
                .map_err(|e| ApplicationError::Internal(format!("Failed to query: {e}")))?
                .collect::<Result<Vec<_>, _>>()
                .map_err(|e| ApplicationError::Internal(format!("Failed to collect results: {e}")))?;

            Ok(requests)
        })
        .await
        .map_err(|e| ApplicationError::Internal(format!("Task join error: {e}")))?
    }

    async fn get_expired(&self) -> Result<Vec<ApprovalRequest>, ApplicationError> {
        let pool = Arc::clone(&self.pool);
        let now = Utc::now();

        tokio::task::spawn_blocking(move || {
            let conn = pool.get().map_err(|e| {
                ApplicationError::Internal(format!("Failed to get database connection: {e}"))
            })?;

            let mut stmt = conn
                .prepare(
                    "SELECT id, user_id, command, description, status, created_at, expires_at, updated_at, reason
                     FROM approval_requests 
                     WHERE status = 'pending' AND expires_at < ?1",
                )
                .map_err(|e| ApplicationError::Internal(format!("Failed to prepare statement: {e}")))?;

            let requests = stmt
                .query_map([now.to_rfc3339()], row_to_approval_request)
                .map_err(|e| ApplicationError::Internal(format!("Failed to query: {e}")))?
                .collect::<Result<Vec<_>, _>>()
                .map_err(|e| ApplicationError::Internal(format!("Failed to collect results: {e}")))?;

            Ok(requests)
        })
        .await
        .map_err(|e| ApplicationError::Internal(format!("Task join error: {e}")))?
    }

    async fn delete(&self, id: &ApprovalId) -> Result<(), ApplicationError> {
        let pool = Arc::clone(&self.pool);
        let id = *id;

        tokio::task::spawn_blocking(move || {
            let conn = pool.get().map_err(|e| {
                ApplicationError::Internal(format!("Failed to get database connection: {e}"))
            })?;

            conn.execute(
                "DELETE FROM approval_requests WHERE id = ?1",
                [id.to_string()],
            )
            .map_err(|e| {
                ApplicationError::Internal(format!("Failed to delete approval request: {e}"))
            })?;

            Ok(())
        })
        .await
        .map_err(|e| ApplicationError::Internal(format!("Task join error: {e}")))?
    }

    async fn get_by_status(
        &self,
        status: ApprovalStatus,
        limit: u32,
    ) -> Result<Vec<ApprovalRequest>, ApplicationError> {
        let pool = Arc::clone(&self.pool);

        tokio::task::spawn_blocking(move || {
            let conn = pool.get().map_err(|e| {
                ApplicationError::Internal(format!("Failed to get database connection: {e}"))
            })?;

            let mut stmt = conn
                .prepare(
                    "SELECT id, user_id, command, description, status, created_at, expires_at, updated_at, reason
                     FROM approval_requests 
                     WHERE status = ?1
                     ORDER BY created_at DESC
                     LIMIT ?2",
                )
                .map_err(|e| ApplicationError::Internal(format!("Failed to prepare statement: {e}")))?;

            let requests = stmt
                .query_map(params![status_to_str(status), limit], row_to_approval_request)
                .map_err(|e| ApplicationError::Internal(format!("Failed to query: {e}")))?
                .collect::<Result<Vec<_>, _>>()
                .map_err(|e| ApplicationError::Internal(format!("Failed to collect results: {e}")))?;

            Ok(requests)
        })
        .await
        .map_err(|e| ApplicationError::Internal(format!("Task join error: {e}")))?
    }

    async fn count_pending_for_user(&self, user_id: &UserId) -> Result<u32, ApplicationError> {
        let pool = Arc::clone(&self.pool);
        let user_id = *user_id;

        tokio::task::spawn_blocking(move || {
            let conn = pool.get().map_err(|e| {
                ApplicationError::Internal(format!("Failed to get database connection: {e}"))
            })?;

            let count: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM approval_requests WHERE user_id = ?1 AND status = 'pending'",
                    [user_id.to_string()],
                    |row| row.get(0),
                )
                .map_err(|e| ApplicationError::Internal(format!("Failed to count: {e}")))?;

            Ok(u32::try_from(count).unwrap_or(u32::MAX))
        })
        .await
        .map_err(|e| ApplicationError::Internal(format!("Task join error: {e}")))?
    }
}

const fn status_to_str(status: ApprovalStatus) -> &'static str {
    match status {
        ApprovalStatus::Pending => "pending",
        ApprovalStatus::Approved => "approved",
        ApprovalStatus::Denied => "denied",
        ApprovalStatus::Expired => "expired",
        ApprovalStatus::Cancelled => "cancelled",
    }
}

fn str_to_status(s: &str) -> Result<ApprovalStatus, ApplicationError> {
    match s {
        "pending" => Ok(ApprovalStatus::Pending),
        "approved" => Ok(ApprovalStatus::Approved),
        "denied" => Ok(ApprovalStatus::Denied),
        "expired" => Ok(ApprovalStatus::Expired),
        "cancelled" => Ok(ApprovalStatus::Cancelled),
        _ => Err(ApplicationError::Internal(format!(
            "Unknown approval status: {s}"
        ))),
    }
}

fn row_to_approval_request(row: &rusqlite::Row) -> Result<ApprovalRequest, rusqlite::Error> {
    let id_str: String = row.get(0)?;
    let user_id_str: String = row.get(1)?;
    let command_json: String = row.get(2)?;
    let description: String = row.get(3)?;
    let status_str: String = row.get(4)?;
    let created_at_str: String = row.get(5)?;
    let expires_at_str: String = row.get(6)?;
    let updated_at_str: String = row.get(7)?;
    let reason_json: Option<String> = row.get(8)?;

    let id = ApprovalId::parse(&id_str).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(e))
    })?;

    let user_id = UserId::parse(&user_id_str).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(1, rusqlite::types::Type::Text, Box::new(e))
    })?;

    let command: AgentCommand = serde_json::from_str(&command_json).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(2, rusqlite::types::Type::Text, Box::new(e))
    })?;

    let status = str_to_status(&status_str).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(
            4,
            rusqlite::types::Type::Text,
            Box::new(std::io::Error::other(e.to_string())),
        )
    })?;

    let created_at = DateTime::parse_from_rfc3339(&created_at_str)
        .map_err(|e| {
            rusqlite::Error::FromSqlConversionFailure(5, rusqlite::types::Type::Text, Box::new(e))
        })?
        .with_timezone(&Utc);

    let expires_at = DateTime::parse_from_rfc3339(&expires_at_str)
        .map_err(|e| {
            rusqlite::Error::FromSqlConversionFailure(6, rusqlite::types::Type::Text, Box::new(e))
        })?
        .with_timezone(&Utc);

    let updated_at = DateTime::parse_from_rfc3339(&updated_at_str)
        .map_err(|e| {
            rusqlite::Error::FromSqlConversionFailure(7, rusqlite::types::Type::Text, Box::new(e))
        })?
        .with_timezone(&Utc);

    let reason = reason_json
        .map(|r| serde_json::from_str(&r))
        .transpose()
        .map_err(|e| {
            rusqlite::Error::FromSqlConversionFailure(8, rusqlite::types::Type::Text, Box::new(e))
        })?;

    Ok(ApprovalRequest {
        id,
        user_id,
        command,
        description,
        status,
        created_at,
        expires_at,
        updated_at,
        reason,
    })
}

#[cfg(test)]
mod tests {
    use domain::AgentCommand;

    use super::*;
    use crate::{
        config::DatabaseConfig,
        persistence::{connection::create_pool, migrations::run_migrations},
    };

    fn create_test_store() -> SqliteApprovalQueue {
        let config = DatabaseConfig {
            path: ":memory:".to_string(),
            max_connections: 1,
            run_migrations: true,
        };
        let pool = create_pool(&config).unwrap();
        let conn = pool.get().unwrap();
        run_migrations(&conn).unwrap();
        SqliteApprovalQueue::new(Arc::new(pool))
    }

    fn sample_command() -> AgentCommand {
        AgentCommand::SendEmail {
            draft_id: "draft-123".to_string(),
        }
    }

    #[tokio::test]
    async fn enqueue_and_get() {
        let queue = create_test_store();
        let user_id = UserId::new();
        let request = ApprovalRequest::new(user_id, sample_command(), "Test approval");
        let id = request.id;

        queue.enqueue(&request).await.unwrap();
        let retrieved = queue.get(&id).await.unwrap();

        assert!(retrieved.is_some());
        let retrieved = retrieved.unwrap();
        assert_eq!(retrieved.id, id);
        assert_eq!(retrieved.description, "Test approval");
    }

    #[tokio::test]
    async fn get_nonexistent() {
        let queue = create_test_store();
        let id = ApprovalId::new();

        let result = queue.get(&id).await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn update_status() {
        let queue = create_test_store();
        let user_id = UserId::new();
        let mut request = ApprovalRequest::new(user_id, sample_command(), "Test");
        let id = request.id;

        queue.enqueue(&request).await.unwrap();
        request.approve().unwrap();
        queue.update(&request).await.unwrap();

        let retrieved = queue.get(&id).await.unwrap().unwrap();
        assert_eq!(retrieved.status, ApprovalStatus::Approved);
    }

    #[tokio::test]
    async fn get_pending_for_user() {
        let queue = create_test_store();
        let user_id = UserId::new();

        for i in 0..3 {
            let req = ApprovalRequest::new(user_id, sample_command(), format!("Request {i}"));
            queue.enqueue(&req).await.unwrap();
        }

        let pending = queue.get_pending_for_user(&user_id).await.unwrap();
        assert_eq!(pending.len(), 3);
    }

    #[tokio::test]
    async fn delete_request() {
        let queue = create_test_store();
        let request = ApprovalRequest::new(UserId::new(), sample_command(), "Test");
        let id = request.id;

        queue.enqueue(&request).await.unwrap();
        queue.delete(&id).await.unwrap();

        assert!(queue.get(&id).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn count_pending() {
        let queue = create_test_store();
        let user_id = UserId::new();

        for _ in 0..5 {
            let req = ApprovalRequest::new(user_id, sample_command(), "Test");
            queue.enqueue(&req).await.unwrap();
        }

        let count = queue.count_pending_for_user(&user_id).await.unwrap();
        assert_eq!(count, 5);
    }

    #[tokio::test]
    async fn get_by_status() {
        let queue = create_test_store();
        let user_id = UserId::new();

        // Create and approve one request
        let mut req1 = ApprovalRequest::new(user_id, sample_command(), "Test 1");
        req1.approve().unwrap();
        queue.enqueue(&req1).await.unwrap();

        // Create a pending request
        let req2 = ApprovalRequest::new(user_id, sample_command(), "Test 2");
        queue.enqueue(&req2).await.unwrap();

        let approved = queue
            .get_by_status(ApprovalStatus::Approved, 10)
            .await
            .unwrap();
        assert_eq!(approved.len(), 1);

        let pending = queue
            .get_by_status(ApprovalStatus::Pending, 10)
            .await
            .unwrap();
        assert_eq!(pending.len(), 1);
    }

    #[tokio::test]
    async fn stores_and_retrieves_reason() {
        let queue = create_test_store();
        let mut request = ApprovalRequest::new(UserId::new(), sample_command(), "Test");
        let id = request.id;

        request.deny(Some("Not allowed".to_string())).unwrap();
        queue.enqueue(&request).await.unwrap();

        let retrieved = queue.get(&id).await.unwrap().unwrap();
        assert_eq!(retrieved.reason, Some("Not allowed".to_string()));
    }
}
