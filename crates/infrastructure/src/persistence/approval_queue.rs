//! SQLite adapter for the ApprovalQueue port
//!
//! Persists approval requests to SQLite for durability across restarts using sqlx.

use application::{error::ApplicationError, ports::ApprovalQueuePort};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use domain::{AgentCommand, ApprovalId, ApprovalRequest, ApprovalStatus, UserId};
use sqlx::SqlitePool;

use super::error::map_sqlx_error;

/// SQLite implementation of the approval queue
#[derive(Debug, Clone)]
pub struct SqliteApprovalQueue {
    pool: SqlitePool,
}

impl SqliteApprovalQueue {
    /// Create a new SQLite approval queue
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl ApprovalQueuePort for SqliteApprovalQueue {
    async fn enqueue(&self, request: &ApprovalRequest) -> Result<(), ApplicationError> {
        let command_json = serde_json::to_string(&request.command)
            .map_err(|e| ApplicationError::Internal(format!("Failed to serialize command: {e}")))?;

        let reason_json = request
            .reason
            .as_ref()
            .map(serde_json::to_string)
            .transpose()
            .map_err(|e| {
                ApplicationError::Internal(format!("Failed to serialize reason: {e}"))
            })?;

        sqlx::query(
            "INSERT INTO approval_requests
             (id, user_id, command, description, status, created_at, expires_at, updated_at, reason)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)",
        )
        .bind(request.id.to_string())
        .bind(request.user_id.to_string())
        .bind(&command_json)
        .bind(&request.description)
        .bind(status_to_str(request.status))
        .bind(request.created_at.to_rfc3339())
        .bind(request.expires_at.to_rfc3339())
        .bind(request.updated_at.to_rfc3339())
        .bind(&reason_json)
        .execute(&self.pool)
        .await
        .map_err(map_sqlx_error)?;

        Ok(())
    }

    async fn get(&self, id: &ApprovalId) -> Result<Option<ApprovalRequest>, ApplicationError> {
        let row: Option<ApprovalRow> = sqlx::query_as(
            "SELECT id, user_id, command, description, status, created_at, expires_at, updated_at, reason
             FROM approval_requests WHERE id = $1",
        )
        .bind(id.to_string())
        .fetch_optional(&self.pool)
        .await
        .map_err(map_sqlx_error)?;

        row.map(ApprovalRow::to_request).transpose()
    }

    async fn update(&self, request: &ApprovalRequest) -> Result<(), ApplicationError> {
        let reason_json = request
            .reason
            .as_ref()
            .map(serde_json::to_string)
            .transpose()
            .map_err(|e| {
                ApplicationError::Internal(format!("Failed to serialize reason: {e}"))
            })?;

        sqlx::query(
            "UPDATE approval_requests
             SET status = $1, updated_at = $2, reason = $3
             WHERE id = $4",
        )
        .bind(status_to_str(request.status))
        .bind(request.updated_at.to_rfc3339())
        .bind(&reason_json)
        .bind(request.id.to_string())
        .execute(&self.pool)
        .await
        .map_err(map_sqlx_error)?;

        Ok(())
    }

    async fn get_pending_for_user(
        &self,
        user_id: &UserId,
    ) -> Result<Vec<ApprovalRequest>, ApplicationError> {
        let rows: Vec<ApprovalRow> = sqlx::query_as(
            "SELECT id, user_id, command, description, status, created_at, expires_at, updated_at, reason
             FROM approval_requests
             WHERE user_id = $1 AND status = 'pending'
             ORDER BY created_at DESC",
        )
        .bind(user_id.to_string())
        .fetch_all(&self.pool)
        .await
        .map_err(map_sqlx_error)?;

        rows.into_iter().map(ApprovalRow::to_request).collect()
    }

    async fn get_expired(&self) -> Result<Vec<ApprovalRequest>, ApplicationError> {
        let now = Utc::now().to_rfc3339();

        let rows: Vec<ApprovalRow> = sqlx::query_as(
            "SELECT id, user_id, command, description, status, created_at, expires_at, updated_at, reason
             FROM approval_requests
             WHERE status = 'pending' AND expires_at < $1",
        )
        .bind(&now)
        .fetch_all(&self.pool)
        .await
        .map_err(map_sqlx_error)?;

        rows.into_iter().map(ApprovalRow::to_request).collect()
    }

    async fn delete(&self, id: &ApprovalId) -> Result<(), ApplicationError> {
        sqlx::query("DELETE FROM approval_requests WHERE id = $1")
            .bind(id.to_string())
            .execute(&self.pool)
            .await
            .map_err(map_sqlx_error)?;

        Ok(())
    }

    async fn get_by_status(
        &self,
        status: ApprovalStatus,
        limit: u32,
    ) -> Result<Vec<ApprovalRequest>, ApplicationError> {
        let rows: Vec<ApprovalRow> = sqlx::query_as(
            "SELECT id, user_id, command, description, status, created_at, expires_at, updated_at, reason
             FROM approval_requests
             WHERE status = $1
             ORDER BY created_at DESC
             LIMIT $2",
        )
        .bind(status_to_str(status))
        .bind(limit)
        .fetch_all(&self.pool)
        .await
        .map_err(map_sqlx_error)?;

        rows.into_iter().map(ApprovalRow::to_request).collect()
    }

    async fn count_pending_for_user(&self, user_id: &UserId) -> Result<u32, ApplicationError> {
        let (count,): (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM approval_requests WHERE user_id = $1 AND status = 'pending'",
        )
        .bind(user_id.to_string())
        .fetch_one(&self.pool)
        .await
        .map_err(map_sqlx_error)?;

        Ok(u32::try_from(count).unwrap_or(u32::MAX))
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

/// Row type for approval query
#[derive(sqlx::FromRow)]
struct ApprovalRow {
    id: String,
    user_id: String,
    command: String,
    description: String,
    status: String,
    created_at: String,
    expires_at: String,
    updated_at: String,
    reason: Option<String>,
}

impl ApprovalRow {
    fn to_request(self) -> Result<ApprovalRequest, ApplicationError> {
        let id = ApprovalId::parse(&self.id)
            .map_err(|e| ApplicationError::Internal(format!("Invalid approval_id: {e}")))?;
        let user_id = UserId::parse(&self.user_id)
            .map_err(|e| ApplicationError::Internal(format!("Invalid user_id: {e}")))?;
        let command: AgentCommand = serde_json::from_str(&self.command)
            .map_err(|e| ApplicationError::Internal(format!("Invalid command JSON: {e}")))?;
        let status = str_to_status(&self.status)?;

        let created_at = DateTime::parse_from_rfc3339(&self.created_at)
            .map_err(|e| ApplicationError::Internal(format!("Invalid created_at: {e}")))?
            .with_timezone(&Utc);
        let expires_at = DateTime::parse_from_rfc3339(&self.expires_at)
            .map_err(|e| ApplicationError::Internal(format!("Invalid expires_at: {e}")))?
            .with_timezone(&Utc);
        let updated_at = DateTime::parse_from_rfc3339(&self.updated_at)
            .map_err(|e| ApplicationError::Internal(format!("Invalid updated_at: {e}")))?
            .with_timezone(&Utc);

        let reason = self
            .reason
            .map(|r| serde_json::from_str(&r))
            .transpose()
            .map_err(|e| ApplicationError::Internal(format!("Invalid reason JSON: {e}")))?;

        Ok(ApprovalRequest {
            id,
            user_id,
            command,
            description: self.description,
            status,
            created_at,
            expires_at,
            updated_at,
            reason,
        })
    }
}

#[cfg(test)]
mod tests {
    use domain::AgentCommand;

    use super::*;
    use crate::persistence::async_connection::AsyncDatabase;

    async fn setup() -> (AsyncDatabase, SqliteApprovalQueue) {
        let db = AsyncDatabase::in_memory().await.unwrap();
        db.migrate().await.unwrap();
        let queue = SqliteApprovalQueue::new(db.pool().clone());
        (db, queue)
    }

    fn sample_command() -> AgentCommand {
        AgentCommand::SendEmail {
            draft_id: "draft-123".to_string(),
        }
    }

    #[tokio::test]
    async fn enqueue_and_get() {
        let (_db, queue) = setup().await;
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
        let (_db, queue) = setup().await;
        let result = queue.get(&ApprovalId::new()).await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn update_status() {
        let (_db, queue) = setup().await;
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
        let (_db, queue) = setup().await;
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
        let (_db, queue) = setup().await;
        let request = ApprovalRequest::new(UserId::new(), sample_command(), "Test");
        let id = request.id;

        queue.enqueue(&request).await.unwrap();
        queue.delete(&id).await.unwrap();
        assert!(queue.get(&id).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn count_pending() {
        let (_db, queue) = setup().await;
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
        let (_db, queue) = setup().await;
        let user_id = UserId::new();

        let mut req1 = ApprovalRequest::new(user_id, sample_command(), "Test 1");
        req1.approve().unwrap();
        queue.enqueue(&req1).await.unwrap();

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
        let (_db, queue) = setup().await;
        let mut request = ApprovalRequest::new(UserId::new(), sample_command(), "Test");
        let id = request.id;

        request.deny(Some("Not allowed".to_string())).unwrap();
        queue.enqueue(&request).await.unwrap();

        let retrieved = queue.get(&id).await.unwrap().unwrap();
        assert_eq!(retrieved.reason, Some("Not allowed".to_string()));
    }
}
