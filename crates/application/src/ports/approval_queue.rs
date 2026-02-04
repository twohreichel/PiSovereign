//! Port for approval queue persistence
//!
//! This port defines the interface for storing and retrieving approval requests
//! that require user confirmation before execution.

use async_trait::async_trait;
use domain::{ApprovalId, ApprovalRequest, ApprovalStatus, UserId};

use crate::error::ApplicationError;

/// Port for approval queue storage
#[async_trait]
pub trait ApprovalQueuePort: Send + Sync {
    /// Add a new approval request to the queue
    async fn enqueue(&self, request: &ApprovalRequest) -> Result<(), ApplicationError>;

    /// Get an approval request by ID
    async fn get(&self, id: &ApprovalId) -> Result<Option<ApprovalRequest>, ApplicationError>;

    /// Update an existing approval request
    async fn update(&self, request: &ApprovalRequest) -> Result<(), ApplicationError>;

    /// Get all pending requests for a user
    async fn get_pending_for_user(
        &self,
        user_id: &UserId,
    ) -> Result<Vec<ApprovalRequest>, ApplicationError>;

    /// Get all requests that have expired (past their expires_at time)
    async fn get_expired(&self) -> Result<Vec<ApprovalRequest>, ApplicationError>;

    /// Delete an approval request
    async fn delete(&self, id: &ApprovalId) -> Result<(), ApplicationError>;

    /// Get requests by status
    async fn get_by_status(
        &self,
        status: ApprovalStatus,
        limit: u32,
    ) -> Result<Vec<ApprovalRequest>, ApplicationError>;

    /// Count pending requests for a user
    async fn count_pending_for_user(&self, user_id: &UserId) -> Result<u32, ApplicationError>;
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use domain::{AgentCommand, ApprovalRequest, UserId};
    use tokio::sync::Mutex;

    use super::*;

    /// Mock implementation for testing
    #[derive(Default)]
    struct MockApprovalQueue {
        requests: Arc<Mutex<Vec<ApprovalRequest>>>,
    }

    #[async_trait]
    impl ApprovalQueuePort for MockApprovalQueue {
        async fn enqueue(&self, request: &ApprovalRequest) -> Result<(), ApplicationError> {
            self.requests.lock().await.push(request.clone());
            Ok(())
        }

        async fn get(&self, id: &ApprovalId) -> Result<Option<ApprovalRequest>, ApplicationError> {
            Ok(self
                .requests
                .lock()
                .await
                .iter()
                .find(|r| r.id == *id)
                .cloned())
        }

        async fn update(&self, request: &ApprovalRequest) -> Result<(), ApplicationError> {
            let mut requests = self.requests.lock().await;
            if let Some(existing) = requests.iter_mut().find(|r| r.id == request.id) {
                *existing = request.clone();
            }
            drop(requests);
            Ok(())
        }

        async fn get_pending_for_user(
            &self,
            user_id: &UserId,
        ) -> Result<Vec<ApprovalRequest>, ApplicationError> {
            Ok(self
                .requests
                .lock()
                .await
                .iter()
                .filter(|r| r.user_id == *user_id && r.status == ApprovalStatus::Pending)
                .cloned()
                .collect())
        }

        async fn get_expired(&self) -> Result<Vec<ApprovalRequest>, ApplicationError> {
            Ok(self
                .requests
                .lock()
                .await
                .iter()
                .filter(|r| r.is_expired())
                .cloned()
                .collect())
        }

        async fn delete(&self, id: &ApprovalId) -> Result<(), ApplicationError> {
            self.requests.lock().await.retain(|r| r.id != *id);
            Ok(())
        }

        async fn get_by_status(
            &self,
            status: ApprovalStatus,
            limit: u32,
        ) -> Result<Vec<ApprovalRequest>, ApplicationError> {
            Ok(self
                .requests
                .lock()
                .await
                .iter()
                .filter(|r| r.status == status)
                .take(limit as usize)
                .cloned()
                .collect())
        }

        async fn count_pending_for_user(&self, user_id: &UserId) -> Result<u32, ApplicationError> {
            let count = self
                .requests
                .lock()
                .await
                .iter()
                .filter(|r| r.user_id == *user_id && r.status == ApprovalStatus::Pending)
                .count();
            Ok(u32::try_from(count).unwrap_or(u32::MAX))
        }
    }

    fn sample_command() -> AgentCommand {
        AgentCommand::SendEmail {
            draft_id: "draft-123".to_string(),
        }
    }

    #[tokio::test]
    async fn enqueue_and_get() {
        let queue = MockApprovalQueue::default();
        let user_id = UserId::new();
        let request = ApprovalRequest::new(user_id, sample_command(), "Test approval");
        let id = request.id;

        queue.enqueue(&request).await.unwrap();
        let retrieved = queue.get(&id).await.unwrap();

        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().id, id);
    }

    #[tokio::test]
    async fn get_pending_for_user() {
        let queue = MockApprovalQueue::default();
        let user_id = UserId::new();

        // Add multiple requests
        for i in 0..3 {
            let req = ApprovalRequest::new(user_id, sample_command(), format!("Request {i}"));
            queue.enqueue(&req).await.unwrap();
        }

        let pending = queue.get_pending_for_user(&user_id).await.unwrap();
        assert_eq!(pending.len(), 3);
    }

    #[tokio::test]
    async fn update_changes_status() {
        let queue = MockApprovalQueue::default();
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
    async fn delete_removes_request() {
        let queue = MockApprovalQueue::default();
        let request = ApprovalRequest::new(UserId::new(), sample_command(), "Test");
        let id = request.id;

        queue.enqueue(&request).await.unwrap();
        queue.delete(&id).await.unwrap();

        assert!(queue.get(&id).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn count_pending_for_user() {
        let queue = MockApprovalQueue::default();
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
        let queue = MockApprovalQueue::default();
        let user_id = UserId::new();

        let mut req1 = ApprovalRequest::new(user_id, sample_command(), "Test 1");
        let req2 = ApprovalRequest::new(user_id, sample_command(), "Test 2");

        req1.approve().unwrap();

        queue.enqueue(&req1).await.unwrap();
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
}
