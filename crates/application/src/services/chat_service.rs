//! Chat service - Simple conversation handling

use std::sync::Arc;
use std::time::Instant;
use tracing::{debug, instrument};

use domain::{ChatMessage, Conversation, MessageMetadata};

use crate::error::ApplicationError;
use crate::ports::{InferencePort, InferenceResult};

/// Service for handling chat conversations
pub struct ChatService {
    inference: Arc<dyn InferencePort>,
    system_prompt: Option<String>,
}

impl ChatService {
    /// Create a new chat service
    pub fn new(inference: Arc<dyn InferencePort>) -> Self {
        Self {
            inference,
            system_prompt: None,
        }
    }

    /// Create a chat service with a system prompt
    pub fn with_system_prompt(inference: Arc<dyn InferencePort>, prompt: impl Into<String>) -> Self {
        Self {
            inference,
            system_prompt: Some(prompt.into()),
        }
    }

    /// Handle a single chat message (stateless)
    #[instrument(skip(self, message), fields(message_len = message.len()))]
    pub async fn chat(&self, message: &str) -> Result<ChatMessage, ApplicationError> {
        let start = Instant::now();

        let result = match &self.system_prompt {
            Some(system) => self.inference.generate_with_system(system, message).await?,
            None => self.inference.generate(message).await?,
        };

        let latency = start.elapsed().as_millis() as u64;

        debug!(
            model = %result.model,
            tokens = ?result.tokens_used,
            latency_ms = latency,
            "Chat response generated"
        );

        let response = ChatMessage::assistant(&result.content).with_metadata(MessageMetadata {
            model: Some(result.model),
            tokens: result.tokens_used,
            latency_ms: Some(latency),
        });

        Ok(response)
    }

    /// Continue a conversation
    #[instrument(skip(self, conversation), fields(conv_id = %conversation.id, msg_count = conversation.message_count()))]
    pub async fn continue_conversation(
        &self,
        conversation: &Conversation,
    ) -> Result<ChatMessage, ApplicationError> {
        let start = Instant::now();

        let result = self.inference.generate_with_context(conversation).await?;

        let latency = start.elapsed().as_millis() as u64;

        debug!(
            model = %result.model,
            tokens = ?result.tokens_used,
            latency_ms = latency,
            "Conversation response generated"
        );

        let response = ChatMessage::assistant(&result.content).with_metadata(MessageMetadata {
            model: Some(result.model),
            tokens: result.tokens_used,
            latency_ms: Some(latency),
        });

        Ok(response)
    }

    /// Check if the underlying inference is healthy
    pub async fn is_healthy(&self) -> bool {
        self.inference.is_healthy().await
    }

    /// Get the current model name
    pub fn current_model(&self) -> &str {
        self.inference.current_model()
    }
}
