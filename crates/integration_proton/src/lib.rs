//! Proton Mail integration
//!
//! Sidecar interface for Proton Mail Bridge.

// Placeholder for future implementation

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Proton integration errors
#[derive(Debug, Error)]
pub enum ProtonError {
    #[error("Bridge not available: {0}")]
    BridgeUnavailable(String),

    #[error("Authentication failed")]
    AuthenticationFailed,

    #[error("Mailbox not found: {0}")]
    MailboxNotFound(String),

    #[error("Request failed: {0}")]
    RequestFailed(String),
}

/// Email summary for display
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmailSummary {
    pub id: String,
    pub from: String,
    pub subject: String,
    pub snippet: String,
    pub received_at: String,
    pub is_read: bool,
    pub is_important: bool,
}

/// Proton Mail client trait
#[async_trait]
pub trait ProtonClient: Send + Sync {
    /// Get recent emails from inbox
    async fn get_inbox(&self, count: u32) -> Result<Vec<EmailSummary>, ProtonError>;

    /// Get unread count
    async fn get_unread_count(&self) -> Result<u32, ProtonError>;

    /// Mark email as read
    async fn mark_read(&self, email_id: &str) -> Result<(), ProtonError>;

    /// Send an email (draft must exist)
    async fn send_draft(&self, draft_id: &str) -> Result<(), ProtonError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn proton_error_bridge_unavailable() {
        let err = ProtonError::BridgeUnavailable("not running".to_string());
        assert_eq!(err.to_string(), "Bridge not available: not running");
    }

    #[test]
    fn proton_error_authentication_failed() {
        let err = ProtonError::AuthenticationFailed;
        assert_eq!(err.to_string(), "Authentication failed");
    }

    #[test]
    fn proton_error_mailbox_not_found() {
        let err = ProtonError::MailboxNotFound("Archive".to_string());
        assert_eq!(err.to_string(), "Mailbox not found: Archive");
    }

    #[test]
    fn proton_error_request_failed() {
        let err = ProtonError::RequestFailed("timeout".to_string());
        assert_eq!(err.to_string(), "Request failed: timeout");
    }

    #[test]
    fn email_summary_creation() {
        let email = EmailSummary {
            id: "mail123".to_string(),
            from: "sender@example.com".to_string(),
            subject: "Hello".to_string(),
            snippet: "This is a test...".to_string(),
            received_at: "2025-02-01T10:00:00Z".to_string(),
            is_read: false,
            is_important: true,
        };
        assert_eq!(email.id, "mail123");
        assert!(!email.is_read);
        assert!(email.is_important);
    }

    #[test]
    fn email_summary_serialization() {
        let email = EmailSummary {
            id: "1".to_string(),
            from: "a@b.com".to_string(),
            subject: "Test".to_string(),
            snippet: "...".to_string(),
            received_at: "2025-01-01T00:00:00Z".to_string(),
            is_read: true,
            is_important: false,
        };
        let json = serde_json::to_string(&email).unwrap();
        assert!(json.contains("subject"));
        assert!(json.contains("is_read"));
    }

    #[test]
    fn email_summary_deserialization() {
        let json = r#"{"id":"1","from":"a@b.com","subject":"Hi","snippet":"...","received_at":"2025-01-01T00:00:00Z","is_read":false,"is_important":false}"#;
        let email: EmailSummary = serde_json::from_str(json).unwrap();
        assert_eq!(email.subject, "Hi");
        assert!(!email.is_read);
    }

    #[test]
    fn email_summary_has_debug() {
        let email = EmailSummary {
            id: "1".to_string(),
            from: "a@b.com".to_string(),
            subject: "Test".to_string(),
            snippet: "...".to_string(),
            received_at: "2025-01-01T00:00:00Z".to_string(),
            is_read: false,
            is_important: false,
        };
        let debug = format!("{email:?}");
        assert!(debug.contains("EmailSummary"));
        assert!(debug.contains("subject"));
    }

    #[test]
    fn email_summary_clone() {
        let email = EmailSummary {
            id: "1".to_string(),
            from: "a@b.com".to_string(),
            subject: "Test".to_string(),
            snippet: "...".to_string(),
            received_at: "2025-01-01T00:00:00Z".to_string(),
            is_read: false,
            is_important: true,
        };
        #[allow(clippy::redundant_clone)]
        let cloned = email.clone();
        assert_eq!(email.id, cloned.id);
        assert_eq!(email.is_important, cloned.is_important);
    }

    #[test]
    fn proton_error_has_debug() {
        let err = ProtonError::AuthenticationFailed;
        let debug = format!("{err:?}");
        assert!(debug.contains("AuthenticationFailed"));
    }
}
