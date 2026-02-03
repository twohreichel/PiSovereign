//! Port definitions for inference engine
//!
//! Defines the traits (ports) that inference adapters must implement.

use std::pin::Pin;

use async_trait::async_trait;
use domain::{ChatMessage, MessageRole};
use futures::Stream;
use serde::{Deserialize, Serialize};

use crate::error::InferenceError;

/// Request for inference
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InferenceRequest {
    /// Messages in the conversation
    pub messages: Vec<InferenceMessage>,
    /// Model to use (overrides config default)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    /// Maximum tokens to generate
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u32>,
    /// Temperature for sampling
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
    /// Whether to stream the response
    #[serde(default)]
    pub stream: bool,
}

/// A message in the inference request (OpenAI-compatible format)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InferenceMessage {
    pub role: String,
    pub content: String,
}

impl From<&ChatMessage> for InferenceMessage {
    fn from(msg: &ChatMessage) -> Self {
        Self {
            role: match msg.role {
                MessageRole::User => "user".to_string(),
                MessageRole::Assistant => "assistant".to_string(),
                MessageRole::System => "system".to_string(),
            },
            content: msg.content.clone(),
        }
    }
}

impl InferenceRequest {
    /// Create a simple single-turn request
    pub fn simple(user_message: impl Into<String>) -> Self {
        Self {
            messages: vec![InferenceMessage {
                role: "user".to_string(),
                content: user_message.into(),
            }],
            model: None,
            max_tokens: None,
            temperature: None,
            stream: false,
        }
    }

    /// Create a request with system prompt
    pub fn with_system(system: impl Into<String>, user: impl Into<String>) -> Self {
        Self {
            messages: vec![
                InferenceMessage {
                    role: "system".to_string(),
                    content: system.into(),
                },
                InferenceMessage {
                    role: "user".to_string(),
                    content: user.into(),
                },
            ],
            model: None,
            max_tokens: None,
            temperature: None,
            stream: false,
        }
    }

    /// Enable streaming for this request
    pub const fn streaming(mut self) -> Self {
        self.stream = true;
        self
    }

    /// Set the model for this request
    pub fn with_model(mut self, model: impl Into<String>) -> Self {
        self.model = Some(model.into());
        self
    }

    /// Set temperature
    pub const fn with_temperature(mut self, temp: f32) -> Self {
        self.temperature = Some(temp);
        self
    }
}

/// Response from inference
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InferenceResponse {
    /// Generated content
    pub content: String,
    /// Model that generated the response
    pub model: String,
    /// Token usage statistics
    pub usage: Option<TokenUsage>,
    /// Finish reason
    pub finish_reason: Option<String>,
}

/// Token usage statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenUsage {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub total_tokens: u32,
}

/// A chunk of a streaming response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamingChunk {
    /// Content delta
    pub content: String,
    /// Whether this is the final chunk
    pub done: bool,
    /// Model name (usually in first/last chunk)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
}

/// Type alias for streaming response
pub type StreamingResponse =
    Pin<Box<dyn Stream<Item = Result<StreamingChunk, InferenceError>> + Send>>;

/// Port for inference engine implementations
#[async_trait]
pub trait InferenceEngine: Send + Sync {
    /// Generate a complete response (non-streaming)
    async fn generate(
        &self,
        request: InferenceRequest,
    ) -> Result<InferenceResponse, InferenceError>;

    /// Generate a streaming response
    async fn generate_stream(
        &self,
        request: InferenceRequest,
    ) -> Result<StreamingResponse, InferenceError>;

    /// Check if the inference server is healthy
    async fn health_check(&self) -> Result<bool, InferenceError>;

    /// List available models
    async fn list_models(&self) -> Result<Vec<String>, InferenceError>;

    /// Get the current default model
    fn default_model(&self) -> &str;
}
