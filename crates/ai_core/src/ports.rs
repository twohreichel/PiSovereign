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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inference_request_simple() {
        let req = InferenceRequest::simple("Hello");
        assert_eq!(req.messages.len(), 1);
        assert_eq!(req.messages[0].role, "user");
        assert_eq!(req.messages[0].content, "Hello");
        assert!(!req.stream);
    }

    #[test]
    fn inference_request_with_system() {
        let req = InferenceRequest::with_system("You are helpful", "Hi");
        assert_eq!(req.messages.len(), 2);
        assert_eq!(req.messages[0].role, "system");
        assert_eq!(req.messages[0].content, "You are helpful");
        assert_eq!(req.messages[1].role, "user");
        assert_eq!(req.messages[1].content, "Hi");
    }

    #[test]
    fn inference_request_streaming() {
        let req = InferenceRequest::simple("Test").streaming();
        assert!(req.stream);
    }

    #[test]
    fn inference_request_with_model() {
        let req = InferenceRequest::simple("Test").with_model("my-model");
        assert_eq!(req.model, Some("my-model".to_string()));
    }

    #[test]
    fn inference_request_with_temperature() {
        let req = InferenceRequest::simple("Test").with_temperature(0.5);
        assert_eq!(req.temperature, Some(0.5));
    }

    #[test]
    fn inference_request_chaining() {
        let req = InferenceRequest::simple("Test")
            .with_model("llama")
            .with_temperature(0.3)
            .streaming();
        assert_eq!(req.model, Some("llama".to_string()));
        assert_eq!(req.temperature, Some(0.3));
        assert!(req.stream);
    }

    #[test]
    fn inference_message_from_chat_message_user() {
        let msg = ChatMessage::user("Hello");
        let inf_msg = InferenceMessage::from(&msg);
        assert_eq!(inf_msg.role, "user");
        assert_eq!(inf_msg.content, "Hello");
    }

    #[test]
    fn inference_message_from_chat_message_assistant() {
        let msg = ChatMessage::assistant("Response");
        let inf_msg = InferenceMessage::from(&msg);
        assert_eq!(inf_msg.role, "assistant");
        assert_eq!(inf_msg.content, "Response");
    }

    #[test]
    fn inference_message_from_chat_message_system() {
        let msg = ChatMessage::system("You are helpful");
        let inf_msg = InferenceMessage::from(&msg);
        assert_eq!(inf_msg.role, "system");
        assert_eq!(inf_msg.content, "You are helpful");
    }

    #[test]
    fn inference_request_serialization() {
        let req = InferenceRequest::simple("Test");
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains("messages"));
        assert!(json.contains("user"));
        assert!(json.contains("Test"));
    }

    #[test]
    fn inference_request_skip_none_fields() {
        let req = InferenceRequest::simple("Test");
        let json = serde_json::to_string(&req).unwrap();
        assert!(!json.contains("model"));
        assert!(!json.contains("max_tokens"));
        assert!(!json.contains("temperature"));
    }

    #[test]
    fn inference_response_creation() {
        let resp = InferenceResponse {
            content: "Hello!".to_string(),
            model: "qwen".to_string(),
            usage: None,
            finish_reason: Some("stop".to_string()),
        };
        assert_eq!(resp.content, "Hello!");
        assert_eq!(resp.model, "qwen");
    }

    #[test]
    fn inference_response_with_usage() {
        let resp = InferenceResponse {
            content: "Hi".to_string(),
            model: "qwen".to_string(),
            usage: Some(TokenUsage {
                prompt_tokens: 10,
                completion_tokens: 5,
                total_tokens: 15,
            }),
            finish_reason: None,
        };
        let usage = resp.usage.unwrap();
        assert_eq!(usage.prompt_tokens, 10);
        assert_eq!(usage.completion_tokens, 5);
        assert_eq!(usage.total_tokens, 15);
    }

    #[test]
    fn token_usage_serialization() {
        let usage = TokenUsage {
            prompt_tokens: 100,
            completion_tokens: 50,
            total_tokens: 150,
        };
        let json = serde_json::to_string(&usage).unwrap();
        assert!(json.contains("prompt_tokens"));
        assert!(json.contains("100"));
    }

    #[test]
    fn streaming_chunk_creation() {
        let chunk = StreamingChunk {
            content: "Hello".to_string(),
            done: false,
            model: None,
        };
        assert_eq!(chunk.content, "Hello");
        assert!(!chunk.done);
    }

    #[test]
    fn streaming_chunk_final() {
        let chunk = StreamingChunk {
            content: String::new(),
            done: true,
            model: Some("qwen".to_string()),
        };
        assert!(chunk.done);
        assert_eq!(chunk.model, Some("qwen".to_string()));
    }

    #[test]
    fn streaming_chunk_serialization() {
        let chunk = StreamingChunk {
            content: "test".to_string(),
            done: false,
            model: None,
        };
        let json = serde_json::to_string(&chunk).unwrap();
        assert!(json.contains("content"));
        assert!(json.contains("done"));
        assert!(!json.contains("model"));
    }
}
