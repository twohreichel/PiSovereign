//! Chat message entity

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Role of the message sender
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MessageRole {
    /// Message from the user
    User,
    /// Message from the assistant
    Assistant,
    /// System prompt or instruction
    System,
}

/// A single message in a conversation
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChatMessage {
    /// Unique message identifier
    pub id: Uuid,
    /// Role of the sender
    pub role: MessageRole,
    /// Message content
    pub content: String,
    /// When the message was created
    pub created_at: DateTime<Utc>,
    /// Optional metadata (model used, tokens, etc.)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<MessageMetadata>,
}

/// Optional metadata about a message
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MessageMetadata {
    /// Model that generated this response
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    /// Number of tokens in the message
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tokens: Option<u32>,
    /// Generation latency in milliseconds
    #[serde(skip_serializing_if = "Option::is_none")]
    pub latency_ms: Option<u64>,
}

impl ChatMessage {
    /// Create a new user message
    pub fn user(content: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4(),
            role: MessageRole::User,
            content: content.into(),
            created_at: Utc::now(),
            metadata: None,
        }
    }

    /// Create a new assistant message
    pub fn assistant(content: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4(),
            role: MessageRole::Assistant,
            content: content.into(),
            created_at: Utc::now(),
            metadata: None,
        }
    }

    /// Create a new system message
    pub fn system(content: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4(),
            role: MessageRole::System,
            content: content.into(),
            created_at: Utc::now(),
            metadata: None,
        }
    }

    /// Add metadata to the message
    pub fn with_metadata(mut self, metadata: MessageMetadata) -> Self {
        self.metadata = Some(metadata);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn user_message_has_correct_role() {
        let msg = ChatMessage::user("Hello");
        assert_eq!(msg.role, MessageRole::User);
        assert_eq!(msg.content, "Hello");
    }

    #[test]
    fn assistant_message_has_correct_role() {
        let msg = ChatMessage::assistant("Hi there!");
        assert_eq!(msg.role, MessageRole::Assistant);
    }
}
