//! Message gateway port - Interface for messaging systems (WhatsApp, etc.)

use async_trait::async_trait;
use domain::PhoneNumber;
use serde::{Deserialize, Serialize};

use crate::error::ApplicationError;

/// An incoming message from a messaging platform
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IncomingMessage {
    /// Unique message ID from the platform
    pub message_id: String,
    /// Sender's phone number
    pub sender: PhoneNumber,
    /// Message content
    pub content: String,
    /// Timestamp (Unix milliseconds)
    pub timestamp: i64,
    /// Platform-specific metadata
    #[serde(default)]
    pub metadata: Option<serde_json::Value>,
}

/// An outgoing message to a messaging platform
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutgoingMessage {
    /// Recipient's phone number
    pub recipient: PhoneNumber,
    /// Message content
    pub content: String,
    /// Optional reply-to message ID
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reply_to: Option<String>,
}

impl OutgoingMessage {
    /// Create a simple outgoing message
    pub fn new(recipient: PhoneNumber, content: impl Into<String>) -> Self {
        Self {
            recipient,
            content: content.into(),
            reply_to: None,
        }
    }

    /// Create a reply to an incoming message
    pub fn reply_to(incoming: &IncomingMessage, content: impl Into<String>) -> Self {
        Self {
            recipient: incoming.sender.clone(),
            content: content.into(),
            reply_to: Some(incoming.message_id.clone()),
        }
    }
}

/// Port for messaging gateway operations
#[async_trait]
pub trait MessageGatewayPort: Send + Sync {
    /// Send a message to a recipient
    async fn send_message(&self, message: OutgoingMessage) -> Result<String, ApplicationError>;

    /// Check if a phone number is whitelisted
    async fn is_whitelisted(&self, phone: &PhoneNumber) -> bool;

    /// Mark a message as read/processed
    async fn mark_read(&self, message_id: &str) -> Result<(), ApplicationError>;
}
