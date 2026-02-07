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
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChatMessage {
    /// Unique message identifier
    pub id: Uuid,
    /// Role of the sender
    pub role: MessageRole,
    /// Message content
    pub content: String,
    /// When the message was created
    pub created_at: DateTime<Utc>,
    /// Sequence number within the conversation (1-based).
    ///
    /// This provides reliable ordering independent of timestamps and enables
    /// incremental persistence by tracking which messages have been persisted.
    #[serde(default)]
    pub sequence_number: u32,
    /// Optional metadata (model used, tokens, etc.)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<MessageMetadata>,
}

/// Optional metadata about a message
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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
            sequence_number: 0,
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
            sequence_number: 0,
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
            sequence_number: 0,
            metadata: None,
        }
    }

    /// Set the sequence number for this message
    #[must_use]
    pub const fn with_sequence_number(mut self, seq: u32) -> Self {
        self.sequence_number = seq;
        self
    }

    /// Add metadata to the message
    #[must_use]
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
        assert_eq!(msg.content, "Hi there!");
    }

    #[test]
    fn system_message_has_correct_role() {
        let msg = ChatMessage::system("You are a helpful assistant.");
        assert_eq!(msg.role, MessageRole::System);
        assert_eq!(msg.content, "You are a helpful assistant.");
    }

    #[test]
    fn message_has_unique_id() {
        let msg1 = ChatMessage::user("Hello");
        let msg2 = ChatMessage::user("Hello");
        assert_ne!(msg1.id, msg2.id);
    }

    #[test]
    fn message_has_created_at_timestamp() {
        let before = Utc::now();
        let msg = ChatMessage::user("Hello");
        let after = Utc::now();
        assert!(msg.created_at >= before);
        assert!(msg.created_at <= after);
    }

    #[test]
    fn message_without_metadata_has_none() {
        let msg = ChatMessage::user("Hello");
        assert!(msg.metadata.is_none());
    }

    #[test]
    fn message_with_metadata_has_some() {
        let metadata = MessageMetadata {
            model: Some("qwen2.5".to_string()),
            tokens: Some(10),
            latency_ms: Some(100),
        };
        let msg = ChatMessage::assistant("Response").with_metadata(metadata);
        assert!(msg.metadata.is_some());
        let meta = msg.metadata.unwrap();
        assert_eq!(meta.model, Some("qwen2.5".to_string()));
        assert_eq!(meta.tokens, Some(10));
        assert_eq!(meta.latency_ms, Some(100));
    }

    #[test]
    fn message_role_serializes_correctly() {
        let msg = ChatMessage::user("Hello");
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains(r#""role":"user""#));
    }

    #[test]
    fn message_role_deserializes_correctly() {
        let json = r#""assistant""#;
        let role: MessageRole = serde_json::from_str(json).unwrap();
        assert_eq!(role, MessageRole::Assistant);
    }
}
