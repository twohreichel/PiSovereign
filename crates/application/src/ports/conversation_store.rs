//! Conversation storage port
//!
//! Defines the interface for persisting and retrieving conversations.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use domain::{
    entities::{ChatMessage, Conversation, ConversationSource},
    value_objects::ConversationId,
};

use crate::error::ApplicationError;

/// Port for conversation persistence
#[async_trait]
pub trait ConversationStore: Send + Sync {
    /// Save a new conversation
    async fn save(&self, conversation: &Conversation) -> Result<(), ApplicationError>;

    /// Get a conversation by ID
    async fn get(&self, id: &ConversationId) -> Result<Option<Conversation>, ApplicationError>;

    /// Get a conversation by phone number and source
    ///
    /// This is used to maintain one conversation per contact per messenger.
    async fn get_by_phone_number(
        &self,
        source: ConversationSource,
        phone_number: &str,
    ) -> Result<Option<Conversation>, ApplicationError>;

    /// Update an existing conversation
    async fn update(&self, conversation: &Conversation) -> Result<(), ApplicationError>;

    /// Delete a conversation
    async fn delete(&self, id: &ConversationId) -> Result<(), ApplicationError>;

    /// Add a message to a conversation
    async fn add_message(
        &self,
        conversation_id: &ConversationId,
        message: &ChatMessage,
    ) -> Result<(), ApplicationError>;

    /// Add multiple messages to a conversation in a single transaction.
    ///
    /// This is more efficient than calling `add_message` multiple times
    /// as it can batch the operations. Returns the number of messages added.
    async fn add_messages(
        &self,
        conversation_id: &ConversationId,
        messages: &[ChatMessage],
    ) -> Result<usize, ApplicationError> {
        // Default implementation calls add_message for each message
        for message in messages {
            self.add_message(conversation_id, message).await?;
        }
        Ok(messages.len())
    }

    /// Get recent conversations (most recently updated first)
    async fn list_recent(&self, limit: usize) -> Result<Vec<Conversation>, ApplicationError>;

    /// Search conversations by content
    async fn search(
        &self,
        query: &str,
        limit: usize,
    ) -> Result<Vec<Conversation>, ApplicationError>;

    /// Delete conversations older than the given date
    ///
    /// Returns the number of deleted conversations.
    async fn cleanup_older_than(&self, cutoff: DateTime<Utc>) -> Result<usize, ApplicationError>;
}
