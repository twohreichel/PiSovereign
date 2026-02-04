//! Approval request entity - Tracks pending approvals for sensitive actions

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};

use crate::{
    commands::AgentCommand,
    value_objects::{ApprovalId, UserId},
};

/// Status of an approval request
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApprovalStatus {
    /// Waiting for user decision
    Pending,
    /// User approved the action
    Approved,
    /// User denied the action
    Denied,
    /// Request expired without decision
    Expired,
    /// Request was cancelled by the system
    Cancelled,
}

impl ApprovalStatus {
    /// Check if this is a terminal state (no further changes possible)
    pub const fn is_terminal(&self) -> bool {
        matches!(
            self,
            Self::Approved | Self::Denied | Self::Expired | Self::Cancelled
        )
    }

    /// Check if the action can be executed
    pub const fn allows_execution(&self) -> bool {
        matches!(self, Self::Approved)
    }
}

/// An approval request for a sensitive action
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApprovalRequest {
    /// Unique identifier for this approval request
    pub id: ApprovalId,
    /// User who initiated the request
    pub user_id: UserId,
    /// The command requiring approval
    pub command: AgentCommand,
    /// Human-readable description of what will happen
    pub description: String,
    /// Current status
    pub status: ApprovalStatus,
    /// When the request was created
    pub created_at: DateTime<Utc>,
    /// When the request expires (if pending)
    pub expires_at: DateTime<Utc>,
    /// When the status was last changed
    pub updated_at: DateTime<Utc>,
    /// Optional reason for denial or cancellation
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

impl ApprovalRequest {
    /// Default expiration time in minutes
    const DEFAULT_EXPIRATION_MINUTES: i64 = 30;

    /// Create a new pending approval request
    pub fn new(user_id: UserId, command: AgentCommand, description: impl Into<String>) -> Self {
        let now = Utc::now();
        let expires_at = now + Duration::minutes(Self::DEFAULT_EXPIRATION_MINUTES);

        Self {
            id: ApprovalId::new(),
            user_id,
            command,
            description: description.into(),
            status: ApprovalStatus::Pending,
            created_at: now,
            expires_at,
            updated_at: now,
            reason: None,
        }
    }

    /// Create a request with a custom expiration time
    #[must_use]
    pub const fn with_expiration(mut self, expires_at: DateTime<Utc>) -> Self {
        self.expires_at = expires_at;
        self
    }

    /// Check if this request has expired
    pub fn is_expired(&self) -> bool {
        self.status == ApprovalStatus::Pending && Utc::now() > self.expires_at
    }

    /// Approve the request
    ///
    /// Returns `Ok(())` if successful, `Err` if the request is not pending or has expired
    pub fn approve(&mut self) -> Result<(), ApprovalError> {
        self.check_can_change_status()?;
        self.status = ApprovalStatus::Approved;
        self.updated_at = Utc::now();
        Ok(())
    }

    /// Deny the request
    ///
    /// Returns `Ok(())` if successful, `Err` if the request is not pending or has expired
    pub fn deny(&mut self, reason: Option<String>) -> Result<(), ApprovalError> {
        self.check_can_change_status()?;
        self.status = ApprovalStatus::Denied;
        self.reason = reason;
        self.updated_at = Utc::now();
        Ok(())
    }

    /// Mark the request as expired
    pub fn mark_expired(&mut self) {
        if self.status == ApprovalStatus::Pending {
            self.status = ApprovalStatus::Expired;
            self.updated_at = Utc::now();
        }
    }

    /// Cancel the request
    pub fn cancel(&mut self, reason: impl Into<String>) {
        if self.status == ApprovalStatus::Pending {
            self.status = ApprovalStatus::Cancelled;
            self.reason = Some(reason.into());
            self.updated_at = Utc::now();
        }
    }

    /// Check if status can be changed
    fn check_can_change_status(&self) -> Result<(), ApprovalError> {
        if self.status.is_terminal() {
            return Err(ApprovalError::AlreadyProcessed(self.status));
        }
        if self.is_expired() {
            return Err(ApprovalError::Expired);
        }
        Ok(())
    }

    /// Get remaining time until expiration
    pub fn time_remaining(&self) -> Option<Duration> {
        if self.status != ApprovalStatus::Pending {
            return None;
        }
        let now = Utc::now();
        if now > self.expires_at {
            return Some(Duration::zero());
        }
        Some(self.expires_at - now)
    }
}

/// Errors that can occur when processing an approval
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ApprovalError {
    /// The request has already been processed
    AlreadyProcessed(ApprovalStatus),
    /// The request has expired
    Expired,
}

impl std::fmt::Display for ApprovalError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::AlreadyProcessed(status) => {
                write!(
                    f,
                    "Approval request already processed with status: {status:?}"
                )
            },
            Self::Expired => write!(f, "Approval request has expired"),
        }
    }
}

impl std::error::Error for ApprovalError {}

#[cfg(test)]
mod tests {
    use chrono::NaiveDate;

    use super::*;

    fn sample_command() -> AgentCommand {
        AgentCommand::SendEmail {
            draft_id: "draft-123".to_string(),
        }
    }

    fn sample_calendar_command() -> AgentCommand {
        AgentCommand::CreateCalendarEvent {
            date: NaiveDate::from_ymd_opt(2025, 1, 15).unwrap(),
            time: chrono::NaiveTime::from_hms_opt(10, 0, 0).unwrap(),
            title: "Meeting".to_string(),
            duration_minutes: Some(60),
            attendees: None,
            location: None,
        }
    }

    #[test]
    fn new_approval_request_is_pending() {
        let req = ApprovalRequest::new(
            UserId::new(),
            sample_command(),
            "Send email to test@example.com",
        );

        assert_eq!(req.status, ApprovalStatus::Pending);
        assert!(!req.status.is_terminal());
        assert!(!req.is_expired());
    }

    #[test]
    fn approval_request_has_unique_id() {
        let req1 = ApprovalRequest::new(UserId::new(), sample_command(), "Description 1");
        let req2 = ApprovalRequest::new(UserId::new(), sample_command(), "Description 2");

        assert_ne!(req1.id, req2.id);
    }

    #[test]
    fn approve_changes_status() {
        let mut req = ApprovalRequest::new(UserId::new(), sample_command(), "Test");

        req.approve().unwrap();

        assert_eq!(req.status, ApprovalStatus::Approved);
        assert!(req.status.is_terminal());
        assert!(req.status.allows_execution());
    }

    #[test]
    fn deny_changes_status_and_stores_reason() {
        let mut req = ApprovalRequest::new(UserId::new(), sample_command(), "Test");

        req.deny(Some("Not now".to_string())).unwrap();

        assert_eq!(req.status, ApprovalStatus::Denied);
        assert!(req.status.is_terminal());
        assert!(!req.status.allows_execution());
        assert_eq!(req.reason, Some("Not now".to_string()));
    }

    #[test]
    fn cannot_approve_already_processed() {
        let mut req = ApprovalRequest::new(UserId::new(), sample_command(), "Test");
        req.approve().unwrap();

        let result = req.approve();

        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err(),
            ApprovalError::AlreadyProcessed(ApprovalStatus::Approved)
        );
    }

    #[test]
    fn cannot_deny_already_approved() {
        let mut req = ApprovalRequest::new(UserId::new(), sample_command(), "Test");
        req.approve().unwrap();

        let result = req.deny(None);

        assert!(result.is_err());
    }

    #[test]
    fn mark_expired_changes_pending_to_expired() {
        let mut req = ApprovalRequest::new(UserId::new(), sample_command(), "Test");

        req.mark_expired();

        assert_eq!(req.status, ApprovalStatus::Expired);
    }

    #[test]
    fn mark_expired_does_not_change_approved() {
        let mut req = ApprovalRequest::new(UserId::new(), sample_command(), "Test");
        req.approve().unwrap();

        req.mark_expired();

        assert_eq!(req.status, ApprovalStatus::Approved);
    }

    #[test]
    fn cancel_changes_pending_to_cancelled() {
        let mut req = ApprovalRequest::new(UserId::new(), sample_command(), "Test");

        req.cancel("System shutdown");

        assert_eq!(req.status, ApprovalStatus::Cancelled);
        assert_eq!(req.reason, Some("System shutdown".to_string()));
    }

    #[test]
    fn custom_expiration() {
        let user_id = UserId::new();
        let custom_expiry = Utc::now() + Duration::hours(2);

        let req =
            ApprovalRequest::new(user_id, sample_command(), "Test").with_expiration(custom_expiry);

        // Allow 1 second tolerance for test execution time
        assert!((req.expires_at - custom_expiry).num_seconds().abs() < 1);
    }

    #[test]
    fn time_remaining_returns_some_for_pending() {
        let req = ApprovalRequest::new(UserId::new(), sample_command(), "Test");

        let remaining = req.time_remaining();

        assert!(remaining.is_some());
        assert!(remaining.unwrap().num_minutes() > 0);
    }

    #[test]
    fn time_remaining_returns_none_for_approved() {
        let mut req = ApprovalRequest::new(UserId::new(), sample_command(), "Test");
        req.approve().unwrap();

        assert!(req.time_remaining().is_none());
    }

    #[test]
    fn approval_status_serialization() {
        let status = ApprovalStatus::Pending;
        let json = serde_json::to_string(&status).unwrap();
        assert_eq!(json, "\"pending\"");

        let status = ApprovalStatus::Approved;
        let json = serde_json::to_string(&status).unwrap();
        assert_eq!(json, "\"approved\"");
    }

    #[test]
    fn approval_request_serialization() {
        let req = ApprovalRequest::new(UserId::new(), sample_command(), "Test description");

        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains("\"status\":\"pending\""));
        assert!(json.contains("\"description\":\"Test description\""));
    }

    #[test]
    fn approval_error_display() {
        let err = ApprovalError::Expired;
        assert_eq!(err.to_string(), "Approval request has expired");

        let err = ApprovalError::AlreadyProcessed(ApprovalStatus::Denied);
        assert!(err.to_string().contains("Denied"));
    }

    #[test]
    fn stores_calendar_command() {
        let req = ApprovalRequest::new(
            UserId::new(),
            sample_calendar_command(),
            "Create meeting at 10:00",
        );

        assert!(matches!(
            &req.command,
            AgentCommand::CreateCalendarEvent { title, .. } if title == "Meeting"
        ));
    }

    #[test]
    fn approval_request_clone() {
        let req = ApprovalRequest::new(UserId::new(), sample_command(), "Test");
        #[allow(clippy::redundant_clone)]
        let cloned = req.clone();

        assert_eq!(req.id, cloned.id);
        assert_eq!(req.status, cloned.status);
    }

    #[test]
    fn approval_request_debug() {
        let req = ApprovalRequest::new(UserId::new(), sample_command(), "Test");
        let debug = format!("{req:?}");

        assert!(debug.contains("ApprovalRequest"));
        assert!(debug.contains("Pending"));
    }
}
