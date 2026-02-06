//! Test fixtures for conversation and message testing.
//!
//! Provides convenient builders for creating test data.

use chrono::{DateTime, Utc};
use domain::entities::{ChatMessage, Conversation, MessageRole};
use domain::value_objects::ConversationId;
use uuid::Uuid;

/// Builder for creating test conversations.
#[derive(Debug, Clone)]
pub struct TestConversation {
    id: Option<ConversationId>,
    title: Option<String>,
    system_prompt: Option<String>,
    messages: Vec<ChatMessage>,
    created_at: Option<DateTime<Utc>>,
    updated_at: Option<DateTime<Utc>>,
}

impl TestConversation {
    /// Create a new test conversation builder.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            id: None,
            title: None,
            system_prompt: None,
            messages: Vec::new(),
            created_at: None,
            updated_at: None,
        }
    }

    /// Set the conversation ID.
    #[must_use]
    pub fn with_id(mut self, id: ConversationId) -> Self {
        self.id = Some(id);
        self
    }

    /// Set the conversation title.
    #[must_use]
    pub fn with_title(mut self, title: impl Into<String>) -> Self {
        self.title = Some(title.into());
        self
    }

    /// Set the system prompt.
    #[must_use]
    pub fn with_system_prompt(mut self, prompt: impl Into<String>) -> Self {
        self.system_prompt = Some(prompt.into());
        self
    }

    /// Add a user message.
    #[must_use]
    pub fn with_user_message(mut self, content: impl Into<String>) -> Self {
        self.messages.push(ChatMessage::user(content));
        self
    }

    /// Add an assistant message.
    #[must_use]
    pub fn with_assistant_message(mut self, content: impl Into<String>) -> Self {
        self.messages.push(ChatMessage::assistant(content));
        self
    }

    /// Add a custom message.
    #[must_use]
    pub fn with_message(mut self, message: ChatMessage) -> Self {
        self.messages.push(message);
        self
    }

    /// Set the creation timestamp.
    #[must_use]
    pub const fn with_created_at(mut self, created_at: DateTime<Utc>) -> Self {
        self.created_at = Some(created_at);
        self
    }

    /// Set the updated timestamp.
    #[must_use]
    pub const fn with_updated_at(mut self, updated_at: DateTime<Utc>) -> Self {
        self.updated_at = Some(updated_at);
        self
    }

    /// Build the conversation.
    #[must_use]
    pub fn build(self) -> Conversation {
        let now = Utc::now();
        let mut conv = Conversation {
            id: self.id.unwrap_or_default(),
            messages: self.messages,
            created_at: self.created_at.unwrap_or(now),
            updated_at: self.updated_at.unwrap_or(now),
            title: self.title,
            system_prompt: self.system_prompt,
            persisted_message_count: 0,
        };
        // If we have messages, mark them as not yet persisted
        // (caller can call mark_messages_persisted() if needed)
        conv.persisted_message_count = 0;
        conv
    }
}

impl Default for TestConversation {
    fn default() -> Self {
        Self::new()
    }
}

/// Builder for creating test messages.
#[derive(Debug, Clone)]
pub struct TestMessage {
    id: Option<Uuid>,
    role: MessageRole,
    content: String,
    created_at: Option<DateTime<Utc>>,
}

impl TestMessage {
    /// Create a new user message builder.
    #[must_use]
    pub fn user(content: impl Into<String>) -> Self {
        Self {
            id: None,
            role: MessageRole::User,
            content: content.into(),
            created_at: None,
        }
    }

    /// Create a new assistant message builder.
    #[must_use]
    pub fn assistant(content: impl Into<String>) -> Self {
        Self {
            id: None,
            role: MessageRole::Assistant,
            content: content.into(),
            created_at: None,
        }
    }

    /// Create a new system message builder.
    #[must_use]
    pub fn system(content: impl Into<String>) -> Self {
        Self {
            id: None,
            role: MessageRole::System,
            content: content.into(),
            created_at: None,
        }
    }

    /// Set a specific message ID.
    #[must_use]
    pub const fn with_id(mut self, id: Uuid) -> Self {
        self.id = Some(id);
        self
    }

    /// Set the creation timestamp.
    #[must_use]
    pub const fn with_created_at(mut self, created_at: DateTime<Utc>) -> Self {
        self.created_at = Some(created_at);
        self
    }

    /// Build the message.
    #[must_use]
    pub fn build(self) -> ChatMessage {
        ChatMessage {
            id: self.id.unwrap_or_else(Uuid::new_v4),
            role: self.role,
            content: self.content,
            created_at: self.created_at.unwrap_or_else(Utc::now),
            sequence_number: 0,
            metadata: None,
        }
    }
}

/// Collection of common test fixtures.
#[derive(Debug, Clone, Copy, Default)]
pub struct TestFixtures;

impl TestFixtures {
    /// Create a simple conversation with one exchange.
    #[must_use]
    pub fn simple_conversation() -> Conversation {
        TestConversation::new()
            .with_title("Test Conversation")
            .with_user_message("Hello!")
            .with_assistant_message("Hi there! How can I help you?")
            .build()
    }

    /// Create a conversation with a system prompt.
    #[must_use]
    pub fn conversation_with_system_prompt() -> Conversation {
        TestConversation::new()
            .with_title("System Prompt Test")
            .with_system_prompt("You are a helpful assistant.")
            .with_user_message("What's 2+2?")
            .with_assistant_message("2+2 equals 4.")
            .build()
    }

    /// Create a long conversation with multiple exchanges.
    #[must_use]
    pub fn long_conversation(exchanges: usize) -> Conversation {
        let mut builder = TestConversation::new().with_title("Long Conversation");

        for i in 0..exchanges {
            builder = builder
                .with_user_message(format!("Question {}", i + 1))
                .with_assistant_message(format!("Answer {}", i + 1));
        }

        builder.build()
    }

    /// Create a conversation with specific ID.
    #[must_use]
    pub fn conversation_with_id(id: ConversationId) -> Conversation {
        TestConversation::new()
            .with_id(id)
            .with_user_message("Test message")
            .build()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_conversation_builder() {
        let conv = TestConversation::new()
            .with_title("Test")
            .with_user_message("Hello")
            .with_assistant_message("Hi")
            .build();

        assert_eq!(conv.title, Some("Test".to_string()));
        assert_eq!(conv.messages.len(), 2);
    }

    #[test]
    fn test_message_builder_user() {
        let msg = TestMessage::user("Hello").build();
        assert_eq!(msg.role, MessageRole::User);
        assert_eq!(msg.content, "Hello");
    }

    #[test]
    fn test_message_builder_assistant() {
        let msg = TestMessage::assistant("Hi").build();
        assert_eq!(msg.role, MessageRole::Assistant);
    }

    #[test]
    fn test_message_builder_system() {
        let msg = TestMessage::system("You are helpful").build();
        assert_eq!(msg.role, MessageRole::System);
    }

    #[test]
    fn test_simple_conversation_fixture() {
        let conv = TestFixtures::simple_conversation();
        assert_eq!(conv.messages.len(), 2);
        assert!(conv.title.is_some());
    }

    #[test]
    fn test_long_conversation_fixture() {
        let conv = TestFixtures::long_conversation(5);
        assert_eq!(conv.messages.len(), 10); // 5 exchanges = 10 messages
    }

    #[test]
    fn test_conversation_with_system_prompt() {
        let conv = TestFixtures::conversation_with_system_prompt();
        assert!(conv.system_prompt.is_some());
    }

    #[test]
    fn test_conversation_with_id() {
        let id = ConversationId::new();
        let conv = TestFixtures::conversation_with_id(id);
        assert_eq!(conv.id, id);
    }
}
