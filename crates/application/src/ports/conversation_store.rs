//! Conversation storage port
//!
//! Defines the interface for persisting and retrieving conversations.

use async_trait::async_trait;
use domain::{
    entities::{ChatMessage, Conversation},
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

    /// Get recent conversations (most recently updated first)
    async fn list_recent(&self, limit: usize) -> Result<Vec<Conversation>, ApplicationError>;

    /// Search conversations by content
    async fn search(
        &self,
        query: &str,
        limit: usize,
    ) -> Result<Vec<Conversation>, ApplicationError>;
}
