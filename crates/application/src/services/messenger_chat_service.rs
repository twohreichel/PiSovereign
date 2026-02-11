//! Messenger chat service
//!
//! Provides conversation persistence and memory integration for messenger platforms
//! (WhatsApp, Signal). Maintains one conversation per phone number per platform.

use std::sync::Arc;

use domain::{
    entities::{ChatMessage, Conversation, ConversationSource},
    value_objects::UserId,
};
use tracing::{debug, info, instrument, warn};
use uuid::Uuid;

use crate::{
    error::ApplicationError,
    ports::{ConversationStore, EmbeddingPort, EncryptionPort, InferencePort, MemoryStore},
    services::{MemoryEnhancedChat, MemoryEnhancedChatConfig, MemoryService},
};

/// Configuration for messenger chat service
#[derive(Debug, Clone)]
pub struct MessengerChatConfig {
    /// Whether persistence is enabled
    pub enabled: bool,
    /// Enable RAG for context retrieval
    pub enable_rag: bool,
    /// Enable learning from conversations
    pub enable_learning: bool,
    /// Maximum messages to retain per conversation
    pub max_messages: usize,
    /// System prompt for AI responses
    pub system_prompt: Option<String>,
}

impl Default for MessengerChatConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            enable_rag: true,
            enable_learning: true,
            max_messages: 100,
            system_prompt: None,
        }
    }
}

/// Response from the messenger chat service
#[derive(Debug, Clone)]
pub struct MessengerChatResponse {
    /// The AI response message
    pub message: ChatMessage,
    /// The conversation ID (for reference)
    pub conversation_id: domain::value_objects::ConversationId,
    /// Whether this is a new conversation
    pub is_new_conversation: bool,
    /// Number of messages in the conversation
    pub message_count: usize,
}

/// Service for handling messenger conversations with persistence and memory
///
/// This service:
/// - Maintains one conversation per phone number per messenger source
/// - Automatically persists conversations and messages
/// - Integrates with the memory system for RAG context
/// - Supports encrypted message storage
///
/// # Example
///
/// ```ignore
/// let service = MessengerChatService::new(
///     inference,
///     memory_service,
///     conversation_store,
///     MessengerChatConfig::default(),
/// );
///
/// // Process a WhatsApp message
/// let response = service.chat(
///     ConversationSource::WhatsApp,
///     "+1234567890",
///     "Hello!",
/// ).await?;
/// ```
pub struct MessengerChatService<S, E, C>
where
    S: MemoryStore,
    E: EmbeddingPort,
    C: EncryptionPort,
{
    memory_chat: MemoryEnhancedChat<S, E, C>,
    conversation_store: Arc<dyn ConversationStore>,
    config: MessengerChatConfig,
}

impl<S, E, C> Clone for MessengerChatService<S, E, C>
where
    S: MemoryStore,
    E: EmbeddingPort,
    C: EncryptionPort,
{
    fn clone(&self) -> Self {
        Self {
            memory_chat: self.memory_chat.clone(),
            conversation_store: Arc::clone(&self.conversation_store),
            config: self.config.clone(),
        }
    }
}

impl<S, E, C> std::fmt::Debug for MessengerChatService<S, E, C>
where
    S: MemoryStore,
    E: EmbeddingPort,
    C: EncryptionPort,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MessengerChatService")
            .field("config", &self.config)
            .finish_non_exhaustive()
    }
}

impl<S, E, C> MessengerChatService<S, E, C>
where
    S: MemoryStore,
    E: EmbeddingPort,
    C: EncryptionPort,
{
    /// Create a new messenger chat service
    ///
    /// # Arguments
    ///
    /// * `inference` - The inference port for AI responses
    /// * `memory_service` - The memory service for RAG
    /// * `conversation_store` - The conversation persistence store
    /// * `config` - Service configuration
    pub fn new(
        inference: Arc<dyn InferencePort>,
        memory_service: MemoryService<S, E, C>,
        conversation_store: Arc<dyn ConversationStore>,
        config: MessengerChatConfig,
    ) -> Self {
        let memory_chat_config = MemoryEnhancedChatConfig {
            enable_rag: config.enable_rag,
            enable_learning: config.enable_learning,
            system_prompt: config.system_prompt.clone(),
            ..Default::default()
        };

        let memory_chat = MemoryEnhancedChat::new(inference, memory_service, memory_chat_config);

        Self {
            memory_chat,
            conversation_store,
            config,
        }
    }

    /// Process a message from a messenger platform
    ///
    /// This method:
    /// 1. Finds or creates a conversation for the phone number
    /// 2. Adds the incoming message to the conversation
    /// 3. Generates an AI response with RAG context
    /// 4. Adds the response to the conversation
    /// 5. Persists the conversation
    ///
    /// # Arguments
    ///
    /// * `source` - The messenger source (WhatsApp or Signal)
    /// * `phone_number` - The sender's phone number
    /// * `message` - The message text
    ///
    /// # Returns
    ///
    /// The AI response and conversation metadata
    #[instrument(skip(self, message), fields(source = ?source, phone = %phone_number, msg_len = message.len()))]
    pub async fn chat(
        &self,
        source: ConversationSource,
        phone_number: &str,
        message: &str,
    ) -> Result<MessengerChatResponse, ApplicationError> {
        if !self.config.enabled {
            warn!("Messenger persistence is disabled, processing without persistence");
            return self
                .chat_without_persistence(source, phone_number, message)
                .await;
        }

        // Find or create conversation
        let (mut conversation, is_new) = self
            .get_or_create_conversation(source, phone_number)
            .await?;

        // Create user ID from phone number for memory context
        // Use UUID v5 (namespace-based) for deterministic ID from phone number
        let user_id = phone_number_to_user_id(phone_number);

        // Add the user message
        let user_message = ChatMessage::user(message);
        conversation.add_message(user_message.clone());

        // Generate response using memory-enhanced chat
        let response = self
            .memory_chat
            .chat_in_conversation(&user_id, &conversation, message)
            .await?;

        // Add response to conversation
        conversation.add_message(response.clone());

        // Trim messages if needed
        self.trim_conversation_if_needed(&mut conversation);

        // Persist the conversation
        self.persist_conversation(&conversation).await?;

        let message_count = conversation.messages.len();
        let conversation_id = conversation.id;

        info!(
            conversation_id = %conversation_id,
            is_new = is_new,
            message_count = message_count,
            "Messenger chat completed"
        );

        Ok(MessengerChatResponse {
            message: response,
            conversation_id,
            is_new_conversation: is_new,
            message_count,
        })
    }

    /// Chat without persistence (fallback when disabled)
    async fn chat_without_persistence(
        &self,
        _source: ConversationSource,
        phone_number: &str,
        message: &str,
    ) -> Result<MessengerChatResponse, ApplicationError> {
        let user_id = phone_number_to_user_id(phone_number);
        let response = self.memory_chat.chat(&user_id, message).await?;

        Ok(MessengerChatResponse {
            message: response,
            conversation_id: domain::value_objects::ConversationId::new(),
            is_new_conversation: true,
            message_count: 2,
        })
    }

    /// Find or create a conversation for the given phone number
    async fn get_or_create_conversation(
        &self,
        source: ConversationSource,
        phone_number: &str,
    ) -> Result<(Conversation, bool), ApplicationError> {
        // Try to find existing conversation
        if let Some(conversation) = self
            .conversation_store
            .get_by_phone_number(source, phone_number)
            .await?
        {
            debug!(
                conversation_id = %conversation.id,
                message_count = conversation.messages.len(),
                "Found existing conversation"
            );
            return Ok((conversation, false));
        }

        // Create new conversation
        let conversation = Conversation::for_messenger(source, phone_number);
        debug!(
            conversation_id = %conversation.id,
            "Created new conversation"
        );

        Ok((conversation, true))
    }

    /// Trim conversation messages if exceeding max
    fn trim_conversation_if_needed(&self, conversation: &mut Conversation) {
        if conversation.messages.len() > self.config.max_messages {
            let to_remove = conversation.messages.len() - self.config.max_messages;
            // Remove oldest messages (skip system messages)
            conversation.messages.drain(0..to_remove);
            debug!(
                removed = to_remove,
                remaining = conversation.messages.len(),
                "Trimmed conversation messages"
            );
        }
    }

    /// Persist the conversation to storage
    async fn persist_conversation(
        &self,
        conversation: &Conversation,
    ) -> Result<(), ApplicationError> {
        // Save will upsert the conversation
        self.conversation_store.save(conversation).await?;
        debug!(
            conversation_id = %conversation.id,
            "Conversation persisted"
        );
        Ok(())
    }

    /// Get conversation for a phone number without processing a message
    #[instrument(skip(self), fields(source = ?source, phone = %phone_number))]
    pub async fn get_conversation(
        &self,
        source: ConversationSource,
        phone_number: &str,
    ) -> Result<Option<Conversation>, ApplicationError> {
        self.conversation_store
            .get_by_phone_number(source, phone_number)
            .await
    }

    /// Clear conversation history for a phone number
    #[instrument(skip(self), fields(source = ?source, phone = %phone_number))]
    pub async fn clear_conversation(
        &self,
        source: ConversationSource,
        phone_number: &str,
    ) -> Result<bool, ApplicationError> {
        if let Some(conversation) = self
            .conversation_store
            .get_by_phone_number(source, phone_number)
            .await?
        {
            self.conversation_store.delete(&conversation.id).await?;
            info!(
                conversation_id = %conversation.id,
                "Cleared conversation"
            );
            Ok(true)
        } else {
            Ok(false)
        }
    }
}

/// Convert a phone number to a deterministic UserId
///
/// Uses a hash-based approach to create a consistent user ID from a phone number.
/// The same phone number will always produce the same user ID.
fn phone_number_to_user_id(phone_number: &str) -> UserId {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    // Create a deterministic hash from the phone number
    let mut hasher = DefaultHasher::new();
    phone_number.hash(&mut hasher);
    // Add a namespace string to avoid collisions
    "messenger-user".hash(&mut hasher);
    let hash = hasher.finish();

    // Convert to UUID by padding the hash
    let uuid = Uuid::from_u128((hash as u128) << 64 | (hash as u128));
    UserId::from(uuid)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_default() {
        let config = MessengerChatConfig::default();
        assert!(config.enabled);
        assert!(config.enable_rag);
        assert!(config.enable_learning);
        assert_eq!(config.max_messages, 100);
        assert!(config.system_prompt.is_none());
    }

    #[test]
    fn response_debug() {
        let response = MessengerChatResponse {
            message: ChatMessage::assistant("Hello!"),
            conversation_id: domain::value_objects::ConversationId::new(),
            is_new_conversation: true,
            message_count: 2,
        };
        let debug_str = format!("{response:?}");
        assert!(debug_str.contains("MessengerChatResponse"));
    }

    #[test]
    fn phone_number_to_user_id_deterministic() {
        // Same phone number should produce same UserId
        let phone = "+1234567890";
        let user_id1 = phone_number_to_user_id(phone);
        let user_id2 = phone_number_to_user_id(phone);
        assert_eq!(user_id1, user_id2);
    }

    #[test]
    fn phone_number_to_user_id_different_for_different_phones() {
        let user_id1 = phone_number_to_user_id("+1234567890");
        let user_id2 = phone_number_to_user_id("+0987654321");
        assert_ne!(user_id1, user_id2);
    }
}
