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

use crate::{error::ApiError, state::AppState};

/// Chat request body
#[derive(Debug, Deserialize)]
pub struct ChatRequest {
    /// User message
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
    Json(request): Json<ChatRequest>,
) -> Result<Json<ChatResponse>, ApiError> {
    if request.message.trim().is_empty() {
        return Err(ApiError::BadRequest("Message cannot be empty".to_string()));
    }

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
#[derive(Debug, Deserialize)]
pub struct StreamChatRequest {
    pub message: String,
}

/// Handle a streaming chat request via SSE
#[instrument(skip(state, request), fields(message_len = request.message.len()))]
pub async fn chat_stream(
    State(state): State<AppState>,
    Json(request): Json<StreamChatRequest>,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>>>, ApiError> {
    if request.message.trim().is_empty() {
        return Err(ApiError::BadRequest("Message cannot be empty".to_string()));
    }

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
