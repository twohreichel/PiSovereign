//! Email port for application layer
//!
//! Defines the interface for email operations (read/send).
//! Implemented by adapters in the infrastructure layer.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Email port errors
#[derive(Debug, Error)]
pub enum EmailError {
    #[error("Email service unavailable")]
    ServiceUnavailable,

    #[error("Authentication failed")]
    AuthenticationFailed,

    #[error("Email not found: {0}")]
    NotFound(String),

    #[error("Operation failed: {0}")]
    OperationFailed(String),

    #[error("Invalid address: {0}")]
    InvalidAddress(String),
}

/// Summary of an email message
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EmailSummary {
    /// Unique identifier (UID)
    pub id: String,
    /// Sender email address
    pub from: String,
    /// Subject line
    pub subject: String,
    /// Short preview of the body
    pub snippet: String,
    /// When the email was received (RFC 3339)
    pub received_at: String,
    /// Whether the email has been read
    pub is_read: bool,
    /// Whether the email is flagged/starred
    pub is_starred: bool,
}

impl EmailSummary {
    /// Create a new email summary
    pub fn new(id: impl Into<String>, from: impl Into<String>, subject: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            from: from.into(),
            subject: subject.into(),
            snippet: String::new(),
            received_at: chrono::Utc::now().to_rfc3339(),
            is_read: false,
            is_starred: false,
        }
    }

    /// Set the snippet/preview text
    #[must_use]
    pub fn with_snippet(mut self, snippet: impl Into<String>) -> Self {
        self.snippet = snippet.into();
        self
    }

    /// Set the received timestamp
    #[must_use]
    pub fn with_received_at(mut self, received_at: impl Into<String>) -> Self {
        self.received_at = received_at.into();
        self
    }

    /// Set the read status
    #[must_use]
    pub const fn with_is_read(mut self, is_read: bool) -> Self {
        self.is_read = is_read;
        self
    }

    /// Set the starred status
    #[must_use]
    pub const fn with_is_starred(mut self, is_starred: bool) -> Self {
        self.is_starred = is_starred;
        self
    }
}

/// Email composition for sending
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmailDraft {
    /// Recipient email address
    pub to: String,
    /// CC recipients
    pub cc: Vec<String>,
    /// Email subject
    pub subject: String,
    /// Email body (plain text)
    pub body: String,
}

impl EmailDraft {
    /// Create a new email draft
    pub fn new(to: impl Into<String>, subject: impl Into<String>, body: impl Into<String>) -> Self {
        Self {
            to: to.into(),
            cc: Vec::new(),
            subject: subject.into(),
            body: body.into(),
        }
    }

    /// Add a CC recipient
    #[must_use]
    pub fn with_cc(mut self, cc: impl Into<String>) -> Self {
        self.cc.push(cc.into());
        self
    }
}

/// Email port trait
///
/// Defines operations for reading and sending emails.
/// Implemented by adapters that connect to email services.
#[async_trait]
pub trait EmailPort: Send + Sync {
    /// Get recent emails from inbox
    ///
    /// # Arguments
    /// * `count` - Maximum number of emails to retrieve
    ///
    /// # Returns
    /// Vector of email summaries, newest first
    async fn get_inbox(&self, count: u32) -> Result<Vec<EmailSummary>, EmailError>;

    /// Get emails from a specific mailbox/folder
    ///
    /// # Arguments
    /// * `mailbox` - Mailbox name (e.g., "INBOX", "Sent", "Archive")
    /// * `count` - Maximum number to retrieve
    async fn get_mailbox(&self, mailbox: &str, count: u32)
    -> Result<Vec<EmailSummary>, EmailError>;

    /// Get unread email count
    async fn get_unread_count(&self) -> Result<u32, EmailError>;

    /// Mark an email as read
    async fn mark_read(&self, email_id: &str) -> Result<(), EmailError>;

    /// Mark an email as unread
    async fn mark_unread(&self, email_id: &str) -> Result<(), EmailError>;

    /// Delete an email (move to trash)
    async fn delete(&self, email_id: &str) -> Result<(), EmailError>;

    /// Send an email
    ///
    /// # Returns
    /// Message ID of the sent email
    async fn send_email(&self, draft: &EmailDraft) -> Result<String, EmailError>;

    /// Check if the email service is available
    async fn is_available(&self) -> bool;

    /// List available mailboxes/folders
    async fn list_mailboxes(&self) -> Result<Vec<String>, EmailError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn email_summary_creation() {
        let summary = EmailSummary::new("123", "sender@example.com", "Test Subject");
        assert_eq!(summary.id, "123");
        assert_eq!(summary.from, "sender@example.com");
        assert_eq!(summary.subject, "Test Subject");
        assert!(!summary.is_read);
    }

    #[test]
    fn email_summary_builder_pattern() {
        let summary = EmailSummary::new("123", "sender@example.com", "Test")
            .with_snippet("Hello world...")
            .with_is_read(true)
            .with_is_starred(true);

        assert_eq!(summary.snippet, "Hello world...");
        assert!(summary.is_read);
        assert!(summary.is_starred);
    }

    #[test]
    fn email_draft_creation() {
        let draft = EmailDraft::new("to@example.com", "Subject", "Body");
        assert_eq!(draft.to, "to@example.com");
        assert_eq!(draft.subject, "Subject");
        assert_eq!(draft.body, "Body");
        assert!(draft.cc.is_empty());
    }

    #[test]
    fn email_draft_with_cc() {
        let draft = EmailDraft::new("to@example.com", "Subject", "Body")
            .with_cc("cc1@example.com")
            .with_cc("cc2@example.com");

        assert_eq!(draft.cc.len(), 2);
        assert_eq!(draft.cc[0], "cc1@example.com");
    }

    #[test]
    fn email_error_display() {
        let error = EmailError::ServiceUnavailable;
        assert_eq!(error.to_string(), "Email service unavailable");

        let error = EmailError::NotFound("123".to_string());
        assert_eq!(error.to_string(), "Email not found: 123");
    }

    #[test]
    fn email_summary_serialization() {
        let summary = EmailSummary::new("123", "test@example.com", "Subject");
        let json = serde_json::to_string(&summary).unwrap();
        assert!(json.contains("\"id\":\"123\""));
    }

    #[test]
    fn email_summary_deserialization() {
        let json = r#"{
            "id": "123",
            "from": "test@example.com",
            "subject": "Test",
            "snippet": "",
            "received_at": "2024-01-01T00:00:00Z",
            "is_read": false,
            "is_starred": false
        }"#;
        let summary: EmailSummary = serde_json::from_str(json).unwrap();
        assert_eq!(summary.id, "123");
    }
}
