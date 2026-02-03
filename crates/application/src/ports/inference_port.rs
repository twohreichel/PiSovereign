//! Inference port - Interface for LLM inference

use async_trait::async_trait;
use domain::{ChatMessage, Conversation};

use crate::error::ApplicationError;

/// Result of an inference call
#[derive(Debug, Clone)]
pub struct InferenceResult {
    /// Generated response content
    pub content: String,
    /// Model used for generation
    pub model: String,
    /// Number of tokens used (if available)
    pub tokens_used: Option<u32>,
    /// Latency in milliseconds
    pub latency_ms: u64,
}

/// Port for inference operations
#[async_trait]
pub trait InferencePort: Send + Sync {
    /// Generate a response for a single message
    async fn generate(&self, message: &str) -> Result<InferenceResult, ApplicationError>;

    /// Generate a response within a conversation context
    async fn generate_with_context(
        &self,
        conversation: &Conversation,
    ) -> Result<InferenceResult, ApplicationError>;

    /// Generate a response with a specific system prompt
    async fn generate_with_system(
        &self,
        system_prompt: &str,
        message: &str,
    ) -> Result<InferenceResult, ApplicationError>;

    /// Check if the inference backend is healthy
    async fn is_healthy(&self) -> bool;

    /// Get the name of the current model
    fn current_model(&self) -> &str;
}
