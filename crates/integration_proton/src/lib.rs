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
