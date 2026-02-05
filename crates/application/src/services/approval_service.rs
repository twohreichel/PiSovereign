//! Approval Service - Manages approval workflow for sensitive actions

use std::sync::Arc;

use chrono::Utc;
use domain::{
    AgentCommand, ApprovalId, ApprovalRequest, ApprovalStatus, AuditEntry, AuditEventType, UserId,
};
use tracing::{debug, info, instrument, warn};

use crate::{
    error::ApplicationError,
    ports::{ApprovalQueuePort, AuditLogPort},
};

/// Service for managing approval workflows
pub struct ApprovalService {
    queue: Arc<dyn ApprovalQueuePort>,
    audit_log: Arc<dyn AuditLogPort>,
}

impl std::fmt::Debug for ApprovalService {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ApprovalService").finish_non_exhaustive()
    }
}

impl ApprovalService {
    /// Create a new approval service
    pub fn new(queue: Arc<dyn ApprovalQueuePort>, audit_log: Arc<dyn AuditLogPort>) -> Self {
        Self { queue, audit_log }
    }

    /// Create a new approval request for a command that requires approval
    #[instrument(skip(self, command), fields(user_id = %user_id))]
    pub async fn request_approval(
        &self,
        user_id: UserId,
        command: AgentCommand,
    ) -> Result<ApprovalRequest, ApplicationError> {
        let description = command.description();

        let request = ApprovalRequest::new(user_id, command.clone(), &description);

        info!(
            approval_id = %request.id,
            command = ?command,
            "Creating approval request"
        );

        self.queue.enqueue(&request).await?;

        // Log the approval request creation
        let audit_entry = AuditEntry::success(AuditEventType::Approval, "approval_requested")
            .with_actor(user_id.to_string())
            .with_resource("approval", request.id.to_string())
            .with_details(
                serde_json::json!({
                    "command": command.description(),
                    "expires_at": request.expires_at.to_rfc3339(),
                })
                .to_string(),
            );
        self.audit_log.log(&audit_entry).await?;

        Ok(request)
    }

    /// Get an approval request by ID
    #[instrument(skip(self))]
    pub async fn get_request(
        &self,
        id: &ApprovalId,
    ) -> Result<Option<ApprovalRequest>, ApplicationError> {
        self.queue.get(id).await
    }

    /// Get all pending requests for a user
    #[instrument(skip(self))]
    pub async fn get_pending_for_user(
        &self,
        user_id: &UserId,
    ) -> Result<Vec<ApprovalRequest>, ApplicationError> {
        self.queue.get_pending_for_user(user_id).await
    }

    /// Approve a request
    #[instrument(skip(self), fields(approval_id = %id, approver = %approver_id))]
    pub async fn approve(
        &self,
        id: &ApprovalId,
        approver_id: &UserId,
    ) -> Result<ApprovalRequest, ApplicationError> {
        let mut request =
            self.queue.get(id).await?.ok_or_else(|| {
                ApplicationError::NotFound("Approval request not found".to_string())
            })?;

        // Verify the approver is the same user who created the request
        if request.user_id != *approver_id {
            warn!(
                request_user = %request.user_id,
                approver = %approver_id,
                "Approval attempt by different user"
            );
            return Err(ApplicationError::NotAuthorized(
                "Not authorized to modify this request".to_string(),
            ));
        }

        // Check if expired
        if request.is_expired() {
            request.mark_expired();
            self.queue.update(&request).await?;
            return Err(ApplicationError::InvalidOperation(
                "Approval request has expired".to_string(),
            ));
        }

        // Approve
        request
            .approve()
            .map_err(|e| ApplicationError::InvalidOperation(format!("Cannot approve: {e}")))?;

        self.queue.update(&request).await?;

        info!(approval_id = %id, "Request approved");

        // Log the approval
        let audit_entry = AuditEntry::success(AuditEventType::Approval, "approval_granted")
            .with_actor(approver_id.to_string())
            .with_resource("approval", id.to_string())
            .with_details(
                serde_json::json!({
                    "command": request.command.description(),
                })
                .to_string(),
            );
        self.audit_log.log(&audit_entry).await?;

        Ok(request)
    }

    /// Deny a request
    #[instrument(skip(self), fields(approval_id = %id, denier = %denier_id))]
    pub async fn deny(
        &self,
        id: &ApprovalId,
        denier_id: &UserId,
        reason: Option<String>,
    ) -> Result<ApprovalRequest, ApplicationError> {
        let mut request =
            self.queue.get(id).await?.ok_or_else(|| {
                ApplicationError::NotFound("Approval request not found".to_string())
            })?;

        // Verify the denier is the same user who created the request
        if request.user_id != *denier_id {
            warn!(
                request_user = %request.user_id,
                denier = %denier_id,
                "Denial attempt by different user"
            );
            return Err(ApplicationError::NotAuthorized(
                "Not authorized to modify this request".to_string(),
            ));
        }

        // Check if expired
        if request.is_expired() {
            request.mark_expired();
            self.queue.update(&request).await?;
            return Err(ApplicationError::InvalidOperation(
                "Approval request has expired".to_string(),
            ));
        }

        // Deny
        request
            .deny(reason.clone())
            .map_err(|e| ApplicationError::InvalidOperation(format!("Cannot deny: {e}")))?;

        self.queue.update(&request).await?;

        info!(approval_id = %id, reason = ?reason, "Request denied");

        // Log the denial
        let audit_entry = AuditEntry::success(AuditEventType::Approval, "approval_denied")
            .with_actor(denier_id.to_string())
            .with_resource("approval", id.to_string())
            .with_details(
                serde_json::json!({
                    "command": request.command.description(),
                    "reason": reason,
                })
                .to_string(),
            );
        self.audit_log.log(&audit_entry).await?;

        Ok(request)
    }

    /// Process expired approvals
    ///
    /// This should be called periodically to clean up expired requests.
    #[instrument(skip(self))]
    pub async fn process_expired(&self) -> Result<Vec<ApprovalRequest>, ApplicationError> {
        let mut expired = self.queue.get_expired().await?;

        for request in &mut expired {
            if request.status == ApprovalStatus::Pending {
                debug!(approval_id = %request.id, "Marking request as expired");

                request.mark_expired();

                self.queue.update(request).await?;

                let audit_entry = AuditEntry::success(AuditEventType::System, "approval_expired")
                    .with_actor(request.user_id.to_string())
                    .with_resource("approval", request.id.to_string())
                    .with_details(
                        serde_json::json!({
                            "command": request.command.description(),
                        })
                        .to_string(),
                    );
                self.audit_log.log(&audit_entry).await?;
            }
        }

        Ok(expired)
    }

    /// Check if a command requires approval
    #[must_use]
    pub const fn requires_approval(command: &AgentCommand) -> bool {
        command.requires_approval()
    }

    /// Get the count of pending approvals for a user
    #[instrument(skip(self))]
    pub async fn pending_count(&self, user_id: &UserId) -> Result<u32, ApplicationError> {
        self.queue.count_pending_for_user(user_id).await
    }

    /// Cancel a pending request (e.g., user changed their mind)
    #[instrument(skip(self), fields(approval_id = %id))]
    pub async fn cancel(
        &self,
        id: &ApprovalId,
        user_id: &UserId,
    ) -> Result<ApprovalRequest, ApplicationError> {
        let mut request =
            self.queue.get(id).await?.ok_or_else(|| {
                ApplicationError::NotFound("Approval request not found".to_string())
            })?;

        // Verify ownership
        if request.user_id != *user_id {
            return Err(ApplicationError::NotAuthorized(
                "Not authorized to modify this request".to_string(),
            ));
        }

        // Check if still pending
        if request.status != ApprovalStatus::Pending {
            return Err(ApplicationError::InvalidOperation(
                "Request is not pending".to_string(),
            ));
        }

        request.cancel("User cancelled");

        self.queue.update(&request).await?;

        let audit_entry = AuditEntry::success(AuditEventType::Approval, "approval_cancelled")
            .with_actor(user_id.to_string())
            .with_resource("approval", id.to_string())
            .with_details(
                serde_json::json!({
                    "command": request.command.description(),
                })
                .to_string(),
            );
        self.audit_log.log(&audit_entry).await?;

        Ok(request)
    }

    /// Get the time remaining before an approval expires (in seconds)
    #[must_use]
    pub fn time_remaining(request: &ApprovalRequest) -> i64 {
        let remaining = request.expires_at - Utc::now();
        remaining.num_seconds().max(0)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use domain::{AgentCommand, UserId};
    use tokio::sync::Mutex;

    use super::*;

    /// Mock approval queue for testing
    #[derive(Default)]
    struct MockApprovalQueue {
        requests: Arc<Mutex<Vec<ApprovalRequest>>>,
    }

    #[async_trait::async_trait]
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
            Ok(count as u32)
        }
    }

    /// Mock audit log for testing
    #[derive(Default)]
    struct MockAuditLog {
        entries: Arc<Mutex<Vec<AuditEntry>>>,
    }

    #[async_trait::async_trait]
    impl AuditLogPort for MockAuditLog {
        async fn log(&self, entry: &AuditEntry) -> Result<(), ApplicationError> {
            self.entries.lock().await.push(entry.clone());
            Ok(())
        }

        async fn query(
            &self,
            _query: &crate::ports::AuditQuery,
        ) -> Result<Vec<AuditEntry>, ApplicationError> {
            Ok(vec![])
        }

        async fn get_recent(&self, limit: u32) -> Result<Vec<AuditEntry>, ApplicationError> {
            Ok(self
                .entries
                .lock()
                .await
                .iter()
                .take(limit as usize)
                .cloned()
                .collect())
        }

        async fn get_for_resource(
            &self,
            _resource_type: &str,
            _resource_id: &str,
        ) -> Result<Vec<AuditEntry>, ApplicationError> {
            Ok(vec![])
        }

        async fn get_for_actor(
            &self,
            _actor: &str,
            _limit: u32,
        ) -> Result<Vec<AuditEntry>, ApplicationError> {
            Ok(vec![])
        }

        async fn count(&self, _query: &crate::ports::AuditQuery) -> Result<u64, ApplicationError> {
            Ok(self.entries.lock().await.len() as u64)
        }
    }

    fn create_test_service() -> (ApprovalService, Arc<MockApprovalQueue>, Arc<MockAuditLog>) {
        let queue = Arc::new(MockApprovalQueue::default());
        let audit_log = Arc::new(MockAuditLog::default());
        let service = ApprovalService::new(queue.clone(), audit_log.clone());
        (service, queue, audit_log)
    }

    #[tokio::test]
    async fn request_approval_creates_pending_request() {
        let (service, queue, _) = create_test_service();
        let user_id = UserId::new();
        let command = AgentCommand::SendEmail {
            draft_id: "draft-123".to_string(),
        };

        let request = service.request_approval(user_id, command).await.unwrap();

        assert_eq!(request.status, ApprovalStatus::Pending);
        assert_eq!(request.user_id, user_id);
        assert_eq!(queue.requests.lock().await.len(), 1);
    }

    #[tokio::test]
    async fn approve_request_changes_status() {
        let (service, _, _) = create_test_service();
        let user_id = UserId::new();
        let command = AgentCommand::SendEmail {
            draft_id: "draft-123".to_string(),
        };

        let request = service.request_approval(user_id, command).await.unwrap();

        let approved = service.approve(&request.id, &user_id).await.unwrap();

        assert_eq!(approved.status, ApprovalStatus::Approved);
    }

    #[tokio::test]
    async fn deny_request_changes_status() {
        let (service, _, _) = create_test_service();
        let user_id = UserId::new();
        let command = AgentCommand::SendEmail {
            draft_id: "draft-123".to_string(),
        };

        let request = service.request_approval(user_id, command).await.unwrap();

        let denied = service
            .deny(&request.id, &user_id, Some("Not now".to_string()))
            .await
            .unwrap();

        assert_eq!(denied.status, ApprovalStatus::Denied);
        assert_eq!(denied.reason, Some("Not now".to_string()));
    }

    #[tokio::test]
    async fn cannot_approve_other_users_request() {
        let (service, _, _) = create_test_service();
        let user_id = UserId::new();
        let other_user = UserId::new();
        let command = AgentCommand::SendEmail {
            draft_id: "draft-123".to_string(),
        };

        let request = service.request_approval(user_id, command).await.unwrap();

        let result = service.approve(&request.id, &other_user).await;

        assert!(matches!(result, Err(ApplicationError::NotAuthorized(_))));
    }

    #[tokio::test]
    async fn cannot_deny_other_users_request() {
        let (service, _, _) = create_test_service();
        let user_id = UserId::new();
        let other_user = UserId::new();
        let command = AgentCommand::SendEmail {
            draft_id: "draft-123".to_string(),
        };

        let request = service.request_approval(user_id, command).await.unwrap();

        let result = service.deny(&request.id, &other_user, None).await;

        assert!(matches!(result, Err(ApplicationError::NotAuthorized(_))));
    }

    #[tokio::test]
    async fn get_pending_for_user_returns_only_pending() {
        let (service, _, _) = create_test_service();
        let user_id = UserId::new();

        // Create two requests
        let command1 = AgentCommand::SendEmail {
            draft_id: "draft-1".to_string(),
        };
        let command2 = AgentCommand::SendEmail {
            draft_id: "draft-2".to_string(),
        };

        let request1 = service.request_approval(user_id, command1).await.unwrap();
        service.request_approval(user_id, command2).await.unwrap();

        // Approve one
        service.approve(&request1.id, &user_id).await.unwrap();

        // Should only have one pending
        let pending = service.get_pending_for_user(&user_id).await.unwrap();
        assert_eq!(pending.len(), 1);
    }

    #[tokio::test]
    async fn pending_count_returns_correct_count() {
        let (service, _, _) = create_test_service();
        let user_id = UserId::new();

        assert_eq!(service.pending_count(&user_id).await.unwrap(), 0);

        let command = AgentCommand::SendEmail {
            draft_id: "draft-1".to_string(),
        };
        service.request_approval(user_id, command).await.unwrap();

        assert_eq!(service.pending_count(&user_id).await.unwrap(), 1);
    }

    #[tokio::test]
    async fn cancel_request_changes_status() {
        let (service, _, _) = create_test_service();
        let user_id = UserId::new();
        let command = AgentCommand::SendEmail {
            draft_id: "draft-123".to_string(),
        };

        let request = service.request_approval(user_id, command).await.unwrap();

        let cancelled = service.cancel(&request.id, &user_id).await.unwrap();

        assert_eq!(cancelled.status, ApprovalStatus::Cancelled);
    }

    #[tokio::test]
    async fn cannot_cancel_other_users_request() {
        let (service, _, _) = create_test_service();
        let user_id = UserId::new();
        let other_user = UserId::new();
        let command = AgentCommand::SendEmail {
            draft_id: "draft-123".to_string(),
        };

        let request = service.request_approval(user_id, command).await.unwrap();

        let result = service.cancel(&request.id, &other_user).await;

        assert!(matches!(result, Err(ApplicationError::NotAuthorized(_))));
    }

    #[test]
    fn requires_approval_checks_command() {
        let send_email = AgentCommand::SendEmail {
            draft_id: "draft-123".to_string(),
        };
        let echo = AgentCommand::Echo {
            message: "hello".to_string(),
        };

        assert!(ApprovalService::requires_approval(&send_email));
        assert!(!ApprovalService::requires_approval(&echo));
    }

    #[test]
    fn time_remaining_returns_positive_for_valid_request() {
        let user_id = UserId::new();
        let command = AgentCommand::SendEmail {
            draft_id: "draft-123".to_string(),
        };
        let request = ApprovalRequest::new(user_id, command, "Test");

        let remaining = ApprovalService::time_remaining(&request);

        // Should be close to 30 minutes (1800 seconds) for a fresh request
        assert!(remaining > 1700);
        assert!(remaining <= 1800);
    }

    #[tokio::test]
    async fn audit_log_records_approval_request() {
        let (service, _, audit_log) = create_test_service();
        let user_id = UserId::new();
        let command = AgentCommand::SendEmail {
            draft_id: "draft-123".to_string(),
        };

        service.request_approval(user_id, command).await.unwrap();

        let entries = audit_log.entries.lock().await;
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].action, "approval_requested");
    }

    #[tokio::test]
    async fn audit_log_records_approval_granted() {
        let (service, _, audit_log) = create_test_service();
        let user_id = UserId::new();
        let command = AgentCommand::SendEmail {
            draft_id: "draft-123".to_string(),
        };

        let request = service.request_approval(user_id, command).await.unwrap();
        service.approve(&request.id, &user_id).await.unwrap();

        let entries = audit_log.entries.lock().await;
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[1].action, "approval_granted");
    }

    #[tokio::test]
    async fn audit_log_records_approval_denied() {
        let (service, _, audit_log) = create_test_service();
        let user_id = UserId::new();
        let command = AgentCommand::SendEmail {
            draft_id: "draft-123".to_string(),
        };

        let request = service.request_approval(user_id, command).await.unwrap();
        service.deny(&request.id, &user_id, None).await.unwrap();

        let entries = audit_log.entries.lock().await;
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[1].action, "approval_denied");
    }

    #[tokio::test]
    async fn get_request_returns_existing_request() {
        let (service, _, _) = create_test_service();
        let user_id = UserId::new();
        let command = AgentCommand::SendEmail {
            draft_id: "draft-123".to_string(),
        };

        let created = service.request_approval(user_id, command).await.unwrap();

        let retrieved = service.get_request(&created.id).await.unwrap();

        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().id, created.id);
    }

    #[tokio::test]
    async fn get_request_returns_none_for_missing() {
        let (service, _, _) = create_test_service();
        let fake_id = ApprovalId::new();

        let result = service.get_request(&fake_id).await.unwrap();

        assert!(result.is_none());
    }

    #[test]
    fn service_has_debug() {
        let queue = Arc::new(MockApprovalQueue::default());
        let audit_log = Arc::new(MockAuditLog::default());
        let service = ApprovalService::new(queue, audit_log);

        let debug = format!("{service:?}");
        assert!(debug.contains("ApprovalService"));
    }
}
