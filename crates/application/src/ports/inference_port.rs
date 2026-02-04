//! Inference port - Interface for LLM inference

use std::pin::Pin;

use async_trait::async_trait;
use domain::Conversation;
use futures::Stream;
use serde::{Deserialize, Serialize};

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

/// A chunk of a streaming response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamingChunk {
    /// Content delta (new text since last chunk)
    pub content: String,
    /// Whether this is the final chunk
    pub done: bool,
    /// Model name (typically present in final chunk)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
}

/// Type alias for streaming response
pub type InferenceStream =
    Pin<Box<dyn Stream<Item = Result<StreamingChunk, ApplicationError>> + Send>>;

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

    /// Generate a streaming response for a single message
    async fn generate_stream(&self, message: &str) -> Result<InferenceStream, ApplicationError>;

    /// Generate a streaming response with a specific system prompt
    async fn generate_stream_with_system(
        &self,
        system_prompt: &str,
        message: &str,
    ) -> Result<InferenceStream, ApplicationError>;

    /// Check if the inference backend is healthy
    async fn is_healthy(&self) -> bool;

    /// Get the name of the current model
    fn current_model(&self) -> &str;

    /// List available models on the system
    ///
    /// # Returns
    /// Vector of available model names
    async fn list_available_models(&self) -> Result<Vec<String>, ApplicationError>;
}
