//! Conversation entity - A sequence of chat messages

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fmt;

use super::{ChatMessage, MessageRole};
use crate::value_objects::{ConversationId, PhoneNumber};

/// Source of the conversation (how the user initiated it)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ConversationSource {
    /// HTTP API or web interface
    #[default]
    Http,
    /// WhatsApp Business API
    #[serde(rename = "whatsapp")]
    WhatsApp,
    /// Signal messenger
    Signal,
}

impl ConversationSource {
    /// Convert to string representation for database storage
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Http => "http",
            Self::WhatsApp => "whatsapp",
            Self::Signal => "signal",
        }
    }

    /// Check if this is a messenger source (WhatsApp or Signal)
    #[must_use]
    pub const fn is_messenger(&self) -> bool {
        matches!(self, Self::WhatsApp | Self::Signal)
    }
}

impl std::str::FromStr for ConversationSource {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "http" => Ok(Self::Http),
            "whatsapp" => Ok(Self::WhatsApp),
            "signal" => Ok(Self::Signal),
            other => Err(format!("Unknown conversation source: {other}")),
        }
    }
}

impl fmt::Display for ConversationSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

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
    /// Number of messages that have been persisted to storage.
    ///
    /// This enables incremental persistence - only messages after this index
    /// need to be persisted during sync operations.
    #[serde(default)]
    pub persisted_message_count: usize,
    /// Source of the conversation (HTTP, WhatsApp, Signal)
    #[serde(default)]
    pub source: ConversationSource,
    /// Phone number for messenger conversations (E.164 format)
    /// Only set for WhatsApp and Signal conversations
    #[serde(skip_serializing_if = "Option::is_none")]
    pub phone_number: Option<PhoneNumber>,
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
            persisted_message_count: 0,
            source: ConversationSource::default(),
            phone_number: None,
        }
    }

    /// Create a new conversation with a system prompt
    pub fn with_system_prompt(system_prompt: impl Into<String>) -> Self {
        let mut conv = Self::new();
        conv.system_prompt = Some(system_prompt.into());
        conv
    }

    /// Create a new conversation for a messenger with phone number
    ///
    /// # Arguments
    ///
    /// * `source` - The messenger source (WhatsApp or Signal)
    /// * `phone_number` - The phone number in E.164 format (e.g., "+1234567890")
    pub fn for_messenger(source: ConversationSource, phone_number: PhoneNumber) -> Self {
        let mut conv = Self::new();
        conv.source = source;
        conv.phone_number = Some(phone_number);
        conv
    }

    /// Set the conversation source
    #[must_use]
    pub const fn with_source(mut self, source: ConversationSource) -> Self {
        self.source = source;
        self
    }

    /// Set the phone number for messenger conversations
    #[must_use]
    pub fn with_phone_number(mut self, phone_number: PhoneNumber) -> Self {
        self.phone_number = Some(phone_number);
        self
    }

    /// Add a message to the conversation.
    ///
    /// Automatically assigns the next sequence number to the message.
    pub fn add_message(&mut self, mut message: ChatMessage) {
        // Assign next sequence number (1-based)
        // Note: In practice, conversations never exceed u32::MAX messages
        #[allow(clippy::cast_possible_truncation)]
        let next_seq = (self.messages.len() as u32).saturating_add(1);
        message.sequence_number = next_seq;
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

    /// Get messages that have not yet been persisted.
    ///
    /// Returns a slice of messages starting from `persisted_message_count`.
    pub fn unpersisted_messages(&self) -> &[ChatMessage] {
        if self.persisted_message_count >= self.messages.len() {
            &[]
        } else {
            &self.messages[self.persisted_message_count..]
        }
    }

    /// Check if there are unpersisted messages
    pub fn has_unpersisted_messages(&self) -> bool {
        self.persisted_message_count < self.messages.len()
    }

    /// Get the count of unpersisted messages
    pub fn unpersisted_message_count(&self) -> usize {
        self.messages
            .len()
            .saturating_sub(self.persisted_message_count)
    }

    /// Mark all current messages as persisted.
    ///
    /// This should be called after successfully persisting messages to storage.
    pub fn mark_messages_persisted(&mut self) {
        self.persisted_message_count = self.messages.len();
    }

    /// Mark a specific number of messages as persisted.
    ///
    /// This is useful when persisting incrementally - you can mark
    /// only the messages that were successfully saved.
    pub fn mark_n_messages_persisted(&mut self, count: usize) {
        self.persisted_message_count = self
            .persisted_message_count
            .saturating_add(count)
            .min(self.messages.len());
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

    #[test]
    fn conversation_has_unique_id() {
        let conv1 = Conversation::new();
        let conv2 = Conversation::new();
        assert_ne!(conv1.id, conv2.id);
    }

    #[test]
    fn new_conversation_has_no_title() {
        let conv = Conversation::new();
        assert!(conv.title.is_none());
    }

    #[test]
    fn set_title_updates_title() {
        let mut conv = Conversation::new();
        conv.set_title("My Conversation");
        assert_eq!(conv.title, Some("My Conversation".to_string()));
    }

    #[test]
    fn set_title_updates_timestamp() {
        let mut conv = Conversation::new();
        let before = conv.updated_at;
        std::thread::sleep(std::time::Duration::from_millis(10));
        conv.set_title("My Conversation");
        assert!(conv.updated_at > before);
    }

    #[test]
    fn last_message_returns_none_for_empty_conversation() {
        let conv = Conversation::new();
        assert!(conv.last_message().is_none());
    }

    #[test]
    fn last_user_message_returns_none_when_no_user_messages() {
        let mut conv = Conversation::new();
        conv.add_assistant_message("Hi");
        assert!(conv.last_user_message().is_none());
    }

    #[test]
    fn messages_field_contains_all_messages() {
        let mut conv = Conversation::new();
        conv.add_user_message("Hello");
        conv.add_assistant_message("Hi");

        assert_eq!(conv.messages.len(), 2);
    }

    #[test]
    fn default_creates_new_conversation() {
        let conv = Conversation::default();
        assert!(conv.is_empty());
    }

    #[test]
    fn add_message_updates_timestamp() {
        let mut conv = Conversation::new();
        let before = conv.updated_at;
        std::thread::sleep(std::time::Duration::from_millis(10));
        conv.add_user_message("Hello");
        assert!(conv.updated_at > before);
    }

    #[test]
    fn with_system_prompt_sets_system_prompt() {
        let conv = Conversation::with_system_prompt("You are a helpful assistant.");
        assert_eq!(
            conv.system_prompt,
            Some("You are a helpful assistant.".to_string())
        );
    }

    #[test]
    fn new_conversation_has_no_system_prompt() {
        let conv = Conversation::new();
        assert!(conv.system_prompt.is_none());
    }

    #[test]
    fn add_message_adds_to_messages_vec() {
        let mut conv = Conversation::new();
        let msg = ChatMessage::user("Test");
        conv.add_message(msg);
        assert_eq!(conv.message_count(), 1);
    }

    // Tests for incremental persistence

    #[test]
    fn new_conversation_has_zero_persisted_count() {
        let conv = Conversation::new();
        assert_eq!(conv.persisted_message_count, 0);
    }

    #[test]
    fn unpersisted_messages_returns_all_when_none_persisted() {
        let mut conv = Conversation::new();
        conv.add_user_message("Hello");
        conv.add_assistant_message("Hi");

        let unpersisted = conv.unpersisted_messages();
        assert_eq!(unpersisted.len(), 2);
        assert_eq!(unpersisted[0].content, "Hello");
        assert_eq!(unpersisted[1].content, "Hi");
    }

    #[test]
    fn unpersisted_messages_returns_new_messages_after_persist() {
        let mut conv = Conversation::new();
        conv.add_user_message("Message 1");
        conv.mark_messages_persisted();
        conv.add_user_message("Message 2");
        conv.add_assistant_message("Response 2");

        let unpersisted = conv.unpersisted_messages();
        assert_eq!(unpersisted.len(), 2);
        assert_eq!(unpersisted[0].content, "Message 2");
        assert_eq!(unpersisted[1].content, "Response 2");
    }

    #[test]
    fn unpersisted_messages_returns_empty_when_all_persisted() {
        let mut conv = Conversation::new();
        conv.add_user_message("Hello");
        conv.mark_messages_persisted();

        assert!(conv.unpersisted_messages().is_empty());
    }

    #[test]
    fn has_unpersisted_messages_returns_true_when_present() {
        let mut conv = Conversation::new();
        conv.add_user_message("Hello");
        assert!(conv.has_unpersisted_messages());
    }

    #[test]
    fn has_unpersisted_messages_returns_false_when_persisted() {
        let mut conv = Conversation::new();
        conv.add_user_message("Hello");
        conv.mark_messages_persisted();
        assert!(!conv.has_unpersisted_messages());
    }

    #[test]
    fn unpersisted_message_count_is_correct() {
        let mut conv = Conversation::new();
        conv.add_user_message("1");
        conv.add_assistant_message("2");
        conv.mark_messages_persisted();
        conv.add_user_message("3");

        assert_eq!(conv.unpersisted_message_count(), 1);
    }

    #[test]
    fn mark_messages_persisted_updates_count() {
        let mut conv = Conversation::new();
        conv.add_user_message("Hello");
        conv.add_assistant_message("Hi");
        assert_eq!(conv.persisted_message_count, 0);

        conv.mark_messages_persisted();
        assert_eq!(conv.persisted_message_count, 2);
    }

    #[test]
    fn mark_n_messages_persisted_increments_count() {
        let mut conv = Conversation::new();
        conv.add_user_message("1");
        conv.add_assistant_message("2");
        conv.add_user_message("3");

        conv.mark_n_messages_persisted(2);
        assert_eq!(conv.persisted_message_count, 2);
        assert_eq!(conv.unpersisted_message_count(), 1);
    }

    #[test]
    fn mark_n_messages_persisted_does_not_exceed_message_count() {
        let mut conv = Conversation::new();
        conv.add_user_message("Hello");

        conv.mark_n_messages_persisted(100);
        assert_eq!(conv.persisted_message_count, 1);
    }

    #[test]
    fn persisted_count_survives_overflow_scenario() {
        let mut conv = Conversation::new();
        conv.persisted_message_count = usize::MAX - 1;
        conv.mark_n_messages_persisted(10);
        // Should saturate, not overflow
        assert_eq!(conv.persisted_message_count, 0); // min(MAX, 0) = 0
    }

    // ConversationSource tests

    #[test]
    fn conversation_source_default_is_http() {
        assert_eq!(ConversationSource::default(), ConversationSource::Http);
    }

    #[test]
    fn conversation_source_as_str() {
        assert_eq!(ConversationSource::Http.as_str(), "http");
        assert_eq!(ConversationSource::WhatsApp.as_str(), "whatsapp");
        assert_eq!(ConversationSource::Signal.as_str(), "signal");
    }

    #[test]
    fn conversation_source_from_str() {
        assert_eq!(
            "http".parse::<ConversationSource>().unwrap(),
            ConversationSource::Http
        );
        assert_eq!(
            "whatsapp".parse::<ConversationSource>().unwrap(),
            ConversationSource::WhatsApp
        );
        assert_eq!(
            "signal".parse::<ConversationSource>().unwrap(),
            ConversationSource::Signal
        );
        // Case insensitive
        assert_eq!(
            "HTTP".parse::<ConversationSource>().unwrap(),
            ConversationSource::Http
        );
        assert_eq!(
            "WhatsApp".parse::<ConversationSource>().unwrap(),
            ConversationSource::WhatsApp
        );
    }

    #[test]
    fn conversation_source_from_str_error() {
        let result = "unknown".parse::<ConversationSource>();
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Unknown conversation source"));
    }

    #[test]
    fn conversation_source_is_messenger() {
        assert!(!ConversationSource::Http.is_messenger());
        assert!(ConversationSource::WhatsApp.is_messenger());
        assert!(ConversationSource::Signal.is_messenger());
    }

    #[test]
    fn conversation_source_display() {
        assert_eq!(format!("{}", ConversationSource::Http), "http");
        assert_eq!(format!("{}", ConversationSource::WhatsApp), "whatsapp");
        assert_eq!(format!("{}", ConversationSource::Signal), "signal");
    }

    #[test]
    fn conversation_source_serialization() {
        let source = ConversationSource::WhatsApp;
        let json = serde_json::to_string(&source).unwrap();
        assert_eq!(json, r#""whatsapp""#);
        let parsed: ConversationSource = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, ConversationSource::WhatsApp);
    }

    // Conversation with source tests

    #[test]
    fn new_conversation_has_http_source() {
        let conv = Conversation::new();
        assert_eq!(conv.source, ConversationSource::Http);
        assert!(conv.phone_number.is_none());
    }

    #[test]
    fn for_messenger_creates_conversation_with_source() {
        let phone = PhoneNumber::new("+1234567890").unwrap();
        let conv = Conversation::for_messenger(ConversationSource::WhatsApp, phone.clone());
        assert_eq!(conv.source, ConversationSource::WhatsApp);
        assert_eq!(conv.phone_number, Some(phone));
    }

    #[test]
    fn with_source_sets_source() {
        let conv = Conversation::new().with_source(ConversationSource::Signal);
        assert_eq!(conv.source, ConversationSource::Signal);
    }

    #[test]
    fn with_phone_number_sets_phone() {
        let phone = PhoneNumber::new("+49123456789").unwrap();
        let conv = Conversation::new().with_phone_number(phone.clone());
        assert_eq!(conv.phone_number, Some(phone));
    }

    #[test]
    fn conversation_with_source_serialization() {
        let phone = PhoneNumber::new("+1234567890").unwrap();
        let conv = Conversation::for_messenger(ConversationSource::Signal, phone.clone());
        let json = serde_json::to_string(&conv).unwrap();
        assert!(json.contains(r#""source":"signal""#));
        assert!(json.contains(r#""phone_number":"+1234567890""#));

        let parsed: Conversation = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.source, ConversationSource::Signal);
        assert_eq!(parsed.phone_number, Some(phone));
    }
}
