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

#[cfg(test)]
mod tests {
    use super::*;

    fn test_phone() -> PhoneNumber {
        PhoneNumber::new("+491234567890").unwrap()
    }

    #[test]
    fn incoming_message_creation() {
        let msg = IncomingMessage {
            message_id: "msg123".to_string(),
            sender: test_phone(),
            content: "Hello".to_string(),
            timestamp: 1_234_567_890,
            metadata: None,
        };
        assert_eq!(msg.message_id, "msg123");
        assert_eq!(msg.content, "Hello");
    }

    #[test]
    fn incoming_message_with_metadata() {
        let msg = IncomingMessage {
            message_id: "msg123".to_string(),
            sender: test_phone(),
            content: "Hello".to_string(),
            timestamp: 1_234_567_890,
            metadata: Some(serde_json::json!({"type": "text"})),
        };
        assert!(msg.metadata.is_some());
    }

    #[test]
    fn outgoing_message_new() {
        let msg = OutgoingMessage::new(test_phone(), "Hi");
        assert_eq!(msg.content, "Hi");
        assert!(msg.reply_to.is_none());
    }

    #[test]
    fn outgoing_message_reply_to() {
        let incoming = IncomingMessage {
            message_id: "orig123".to_string(),
            sender: test_phone(),
            content: "Original".to_string(),
            timestamp: 1_234_567_890,
            metadata: None,
        };

        let reply = OutgoingMessage::reply_to(&incoming, "Reply");
        assert_eq!(reply.content, "Reply");
        assert_eq!(reply.reply_to, Some("orig123".to_string()));
    }

    #[test]
    fn incoming_message_serialization() {
        let msg = IncomingMessage {
            message_id: "msg123".to_string(),
            sender: test_phone(),
            content: "Hello".to_string(),
            timestamp: 1_234_567_890,
            metadata: None,
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("message_id"));
        assert!(json.contains("sender"));
    }

    #[test]
    fn outgoing_message_serialization() {
        let msg = OutgoingMessage::new(test_phone(), "Hi");
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("recipient"));
        assert!(json.contains("content"));
        assert!(!json.contains("reply_to")); // None is skipped
    }

    #[test]
    fn outgoing_message_with_reply_serialization() {
        let incoming = IncomingMessage {
            message_id: "orig123".to_string(),
            sender: test_phone(),
            content: "Original".to_string(),
            timestamp: 1_234_567_890,
            metadata: None,
        };
        let reply = OutgoingMessage::reply_to(&incoming, "Reply");
        let json = serde_json::to_string(&reply).unwrap();
        assert!(json.contains("reply_to"));
    }

    #[test]
    fn incoming_message_clone() {
        let msg = IncomingMessage {
            message_id: "msg123".to_string(),
            sender: test_phone(),
            content: "Hello".to_string(),
            timestamp: 1_234_567_890,
            metadata: None,
        };
        #[allow(clippy::redundant_clone)]
        let cloned = msg.clone();
        assert_eq!(msg.message_id, cloned.message_id);
    }

    #[test]
    fn outgoing_message_clone() {
        let msg = OutgoingMessage::new(test_phone(), "Hi");
        #[allow(clippy::redundant_clone)]
        let cloned = msg.clone();
        assert_eq!(msg.content, cloned.content);
    }
}
