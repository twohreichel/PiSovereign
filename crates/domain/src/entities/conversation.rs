//! Conversation entity - A sequence of chat messages

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::{ChatMessage, MessageRole};
use crate::value_objects::ConversationId;

/// A conversation containing a sequence of messages
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Conversation {
    /// Unique conversation identifier
    pub id: ConversationId,
    /// Messages in the conversation (oldest first)
    pub messages: Vec<ChatMessage>,
    /// When the conversation started
    pub created_at: DateTime<Utc>,
    /// When the conversation was last updated
    pub updated_at: DateTime<Utc>,
    /// Optional title for the conversation
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// System prompt for this conversation
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system_prompt: Option<String>,
}

impl Conversation {
    /// Create a new empty conversation
    pub fn new() -> Self {
        let now = Utc::now();
        Self {
            id: ConversationId::new(),
            messages: Vec::new(),
            created_at: now,
            updated_at: now,
            title: None,
            system_prompt: None,
        }
    }

    /// Create a new conversation with a system prompt
    pub fn with_system_prompt(system_prompt: impl Into<String>) -> Self {
        let mut conv = Self::new();
        conv.system_prompt = Some(system_prompt.into());
        conv
    }

    /// Add a message to the conversation
    pub fn add_message(&mut self, message: ChatMessage) {
        self.messages.push(message);
        self.updated_at = Utc::now();
    }

    /// Add a user message
    pub fn add_user_message(&mut self, content: impl Into<String>) {
        self.add_message(ChatMessage::user(content));
    }

    /// Add an assistant message
    pub fn add_assistant_message(&mut self, content: impl Into<String>) {
        self.add_message(ChatMessage::assistant(content));
    }

    /// Get the last message in the conversation
    pub fn last_message(&self) -> Option<&ChatMessage> {
        self.messages.last()
    }

    /// Get the last user message
    pub fn last_user_message(&self) -> Option<&ChatMessage> {
        self.messages
            .iter()
            .rev()
            .find(|m| m.role == MessageRole::User)
    }

    /// Get the number of messages
    pub fn message_count(&self) -> usize {
        self.messages.len()
    }

    /// Check if the conversation is empty
    pub fn is_empty(&self) -> bool {
        self.messages.is_empty()
    }

    /// Set the conversation title
    pub fn set_title(&mut self, title: impl Into<String>) {
        self.title = Some(title.into());
        self.updated_at = Utc::now();
    }
}

impl Default for Conversation {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_conversation_is_empty() {
        let conv = Conversation::new();
        assert!(conv.is_empty());
        assert_eq!(conv.message_count(), 0);
    }

    #[test]
    fn messages_can_be_added() {
        let mut conv = Conversation::new();
        conv.add_user_message("Hello");
        conv.add_assistant_message("Hi there!");

        assert_eq!(conv.message_count(), 2);
        assert_eq!(conv.last_message().unwrap().content, "Hi there!");
    }

    #[test]
    fn last_user_message_is_found() {
        let mut conv = Conversation::new();
        conv.add_user_message("First question");
        conv.add_assistant_message("First answer");
        conv.add_user_message("Second question");
        conv.add_assistant_message("Second answer");

        let last_user = conv.last_user_message().unwrap();
        assert_eq!(last_user.content, "Second question");
    }
}
