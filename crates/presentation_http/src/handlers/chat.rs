//! Chat handlers

use std::{convert::Infallible, time::Duration};

use axum::{
    Json,
    extract::State,
    response::sse::{Event, Sse},
};
use futures::stream::{self, Stream};
use serde::{Deserialize, Serialize};
use tracing::instrument;
use validator::Validate;

use crate::{error::ApiError, middleware::ValidatedJson, state::AppState};

/// Maximum allowed message length (10KB)
pub const MAX_MESSAGE_LENGTH: u64 = 10_000;

/// Validate that a string is not empty after trimming
fn validate_not_empty_trimmed(value: &str) -> Result<(), validator::ValidationError> {
    if value.trim().is_empty() {
        return Err(validator::ValidationError::new(
            "Message cannot be empty or whitespace only",
        ));
    }
    Ok(())
}

/// Chat request body
#[derive(Debug, Deserialize, Validate)]
pub struct ChatRequest {
    /// User message
    #[validate(length(
        min = 1,
        max = 10000,
        message = "Message must be between 1 and 10000 characters"
    ))]
    #[validate(custom(function = "validate_not_empty_trimmed"))]
    pub message: String,
    /// Optional conversation ID for context
    #[serde(default)]
    #[allow(dead_code)]
    pub conversation_id: Option<String>,
}

/// Chat response body
#[derive(Debug, Serialize)]
pub struct ChatResponse {
    /// Assistant response
    pub message: String,
    /// Model used
    pub model: String,
    /// Tokens used (if available)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tokens: Option<u32>,
    /// Latency in milliseconds
    pub latency_ms: u64,
}

/// Handle a chat request
#[instrument(skip(state, request), fields(message_len = request.message.len()))]
pub async fn chat(
    State(state): State<AppState>,
    ValidatedJson(request): ValidatedJson<ChatRequest>,
) -> Result<Json<ChatResponse>, ApiError> {
    let response = state.chat_service.chat(&request.message).await?;

    let metadata = response.metadata.as_ref();

    Ok(Json(ChatResponse {
        message: response.content,
        model: metadata.and_then(|m| m.model.clone()).unwrap_or_default(),
        tokens: metadata.and_then(|m| m.tokens),
        latency_ms: metadata.and_then(|m| m.latency_ms).unwrap_or(0),
    }))
}

/// Streaming chat request
#[derive(Debug, Deserialize, Validate)]
pub struct StreamChatRequest {
    #[validate(length(
        min = 1,
        max = 10000,
        message = "Message must be between 1 and 10000 characters"
    ))]
    #[validate(custom(function = "validate_not_empty_trimmed"))]
    pub message: String,
}

/// Handle a streaming chat request via SSE
#[instrument(skip(state, request), fields(message_len = request.message.len()))]
pub async fn chat_stream(
    State(state): State<AppState>,
    ValidatedJson(request): ValidatedJson<StreamChatRequest>,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>>>, ApiError> {
    // For now, we simulate streaming by sending the full response in one event
    // TODO: Implement true streaming when ai_core streaming is connected
    let response = state.chat_service.chat(&request.message).await?;

    let stream = stream::once(async move {
        Ok::<_, Infallible>(
            Event::default().data(
                serde_json::json!({
                    "content": response.content,
                    "done": true
                })
                .to_string(),
            ),
        )
    });

    Ok(Sse::new(stream).keep_alive(
        axum::response::sse::KeepAlive::new()
            .interval(Duration::from_secs(15))
            .text("keep-alive"),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chat_request_deserialize() {
        let json = r#"{"message": "Hello"}"#;
        let request: ChatRequest = serde_json::from_str(json).unwrap();
        assert_eq!(request.message, "Hello");
        assert!(request.conversation_id.is_none());
    }

    #[test]
    fn chat_request_with_conversation_id() {
        let json = r#"{"message": "Hi", "conversation_id": "abc123"}"#;
        let request: ChatRequest = serde_json::from_str(json).unwrap();
        assert_eq!(request.message, "Hi");
        assert_eq!(request.conversation_id, Some("abc123".to_string()));
    }

    #[test]
    fn chat_request_debug() {
        let request = ChatRequest {
            message: "Test".to_string(),
            conversation_id: None,
        };
        let debug = format!("{request:?}");
        assert!(debug.contains("ChatRequest"));
    }

    #[test]
    fn chat_response_serialize() {
        let response = ChatResponse {
            message: "Hello there".to_string(),
            model: "qwen".to_string(),
            tokens: Some(42),
            latency_ms: 100,
        };
        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("Hello there"));
        assert!(json.contains("qwen"));
        assert!(json.contains("42"));
    }

    #[test]
    fn chat_response_without_tokens() {
        let response = ChatResponse {
            message: "Response".to_string(),
            model: "llama".to_string(),
            tokens: None,
            latency_ms: 50,
        };
        let json = serde_json::to_string(&response).unwrap();
        assert!(!json.contains("tokens"));
    }

    #[test]
    fn chat_response_debug() {
        let response = ChatResponse {
            message: "Test".to_string(),
            model: "model".to_string(),
            tokens: None,
            latency_ms: 10,
        };
        let debug = format!("{response:?}");
        assert!(debug.contains("ChatResponse"));
    }

    #[test]
    fn stream_chat_request_deserialize() {
        let json = r#"{"message": "Stream this"}"#;
        let request: StreamChatRequest = serde_json::from_str(json).unwrap();
        assert_eq!(request.message, "Stream this");
    }

    #[test]
    fn stream_chat_request_debug() {
        let request = StreamChatRequest {
            message: "Test".to_string(),
        };
        let debug = format!("{request:?}");
        assert!(debug.contains("StreamChatRequest"));
    }

    #[test]
    fn empty_message_validation() {
        let request = ChatRequest {
            message: "   ".to_string(),
            conversation_id: None,
        };
        assert!(request.message.trim().is_empty());
    }

    #[test]
    fn non_empty_message_validation() {
        let request = ChatRequest {
            message: "  Hello  ".to_string(),
            conversation_id: None,
        };
        assert!(!request.message.trim().is_empty());
    }
}
