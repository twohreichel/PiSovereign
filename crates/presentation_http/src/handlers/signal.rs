//! Signal messenger handlers
//!
//! Handles message polling from signal-cli daemon and message processing.
//! Signal uses a polling model rather than webhooks.

use application::ports::SynthesisResult;
use axum::{Json, extract::State, http::StatusCode, response::IntoResponse};
use domain::PhoneNumber;
use domain::entities::{Conversation, ConversationSource};
use integration_signal::Attachment;
use serde::{Deserialize, Serialize};
use tracing::{debug, error, info, instrument, warn};
use utoipa::ToSchema;

use crate::state::AppState;

/// Query parameters for message polling
#[derive(Debug, Deserialize, ToSchema)]
pub struct PollQuery {
    /// Timeout in seconds (0 for non-blocking, default: 1)
    #[serde(default = "default_timeout")]
    pub timeout: u64,
}

const fn default_timeout() -> u64 {
    1
}

/// Response for polling messages
#[derive(Debug, Serialize, ToSchema)]
pub struct PollResponse {
    /// Number of messages processed
    pub processed: usize,
    /// Individual message responses
    pub messages: Vec<MessageResponse>,
    /// Whether Signal is available
    pub available: bool,
}

/// Response for a single processed message
#[derive(Debug, Serialize, ToSchema)]
pub struct MessageResponse {
    /// Message timestamp (Signal's unique ID)
    pub timestamp: i64,
    /// Sender phone number
    pub from: String,
    /// Processing status
    pub status: String,
    /// Optional response text
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response: Option<String>,
    /// Response type (text or audio)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response_type: Option<String>,
}

/// Health check for Signal integration
#[derive(Debug, Serialize, ToSchema)]
pub struct SignalHealthResponse {
    /// Whether Signal is available
    pub available: bool,
    /// Status message
    pub status: String,
    /// Configured phone number (redacted)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub phone_number: Option<String>,
}

/// Check Signal service availability
///
/// Returns the status of the signal-cli daemon connection.
#[utoipa::path(
    get,
    path = "/health/signal",
    tag = "signal",
    responses(
        (status = 200, description = "Signal daemon is available", body = SignalHealthResponse),
        (status = 503, description = "Signal daemon is unavailable", body = SignalHealthResponse)
    )
)]
#[instrument(skip(state))]
pub async fn health_check(State(state): State<AppState>) -> impl IntoResponse {
    let config = state.config.load();

    // Check if Signal integration is configured
    if config.signal.phone_number.is_empty() {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(SignalHealthResponse {
                available: false,
                status: "Signal integration not configured".to_string(),
                phone_number: None,
            }),
        )
            .into_response();
    }

    // Check if messenger is set to Signal
    if !matches!(config.messenger, infrastructure::MessengerSelection::Signal) {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(SignalHealthResponse {
                available: false,
                status: "Signal is not the selected messenger".to_string(),
                phone_number: None,
            }),
        )
            .into_response();
    }

    // Check if signal adapter is available in state
    let Some(ref messenger_adapter) = state.messenger_adapter else {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(SignalHealthResponse {
                available: false,
                status: "Messenger adapter not initialized".to_string(),
                phone_number: None,
            }),
        )
            .into_response();
    };

    // Check daemon availability
    let available = messenger_adapter.is_available().await;

    // Redact phone number (show last 4 digits)
    let phone_redacted = {
        let phone = &config.signal.phone_number;
        if phone.len() > 4 {
            format!("***{}", &phone[phone.len() - 4..])
        } else {
            "****".to_string()
        }
    };

    let status = if available {
        "Signal daemon is running"
    } else {
        "Signal daemon is not available"
    };

    let status_code = if available {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    };

    (
        status_code,
        Json(SignalHealthResponse {
            available,
            status: status.to_string(),
            phone_number: Some(phone_redacted),
        }),
    )
        .into_response()
}

/// Poll for new Signal messages and process them
///
/// This endpoint polls the signal-cli daemon for new messages and processes them.
/// Use this endpoint for periodic polling (e.g., every few seconds).
#[utoipa::path(
    post,
    path = "/v1/signal/poll",
    tag = "signal",
    params(
        ("timeout" = u64, Query, description = "Timeout in seconds (0 for non-blocking, default: 1)")
    ),
    responses(
        (status = 200, description = "Messages polled and processed", body = PollResponse),
        (status = 503, description = "Signal not available", body = PollResponse)
    ),
    security(
        ("api_key" = [])
    )
)]
#[instrument(skip(state))]
pub async fn poll_messages(
    State(state): State<AppState>,
    axum::extract::Query(query): axum::extract::Query<PollQuery>,
) -> impl IntoResponse {
    let config = state.config.load();

    // Check if Signal integration is configured
    if config.signal.phone_number.is_empty() {
        warn!("Signal poll attempted but not configured");
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(PollResponse {
                processed: 0,
                messages: vec![],
                available: false,
            }),
        )
            .into_response();
    }

    // Check if messenger is set to Signal
    if !matches!(config.messenger, infrastructure::MessengerSelection::Signal) {
        debug!("Signal poll attempted but messenger is not Signal");
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(PollResponse {
                processed: 0,
                messages: vec![],
                available: false,
            }),
        )
            .into_response();
    }

    // Get the messenger adapter
    let Some(ref messenger_adapter) = state.messenger_adapter else {
        warn!("Signal poll called but messenger adapter not initialized");
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(PollResponse {
                processed: 0,
                messages: vec![],
                available: false,
            }),
        )
            .into_response();
    };

    // Check availability
    if !messenger_adapter.is_available().await {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(PollResponse {
                processed: 0,
                messages: vec![],
                available: false,
            }),
        )
            .into_response();
    }

    // We need access to the underlying SignalClient for receiving messages
    // This is done through the SignalMessengerAdapter
    let Some(signal_adapter) = state.signal_client.as_ref() else {
        error!("Signal client not available in state");
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(PollResponse {
                processed: 0,
                messages: vec![],
                available: true,
            }),
        )
            .into_response();
    };

    // Poll for messages
    let envelopes = match signal_adapter.receive(query.timeout).await {
        Ok(envs) => envs,
        Err(e) => {
            error!(error = %e, "Failed to receive Signal messages");
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({
                    "error": format!("Failed to receive messages: {e}"),
                    "available": true
                })),
            )
                .into_response();
        },
    };

    if envelopes.is_empty() {
        debug!("No new Signal messages");
        return (
            StatusCode::OK,
            Json(PollResponse {
                processed: 0,
                messages: vec![],
                available: true,
            }),
        )
            .into_response();
    }

    info!(count = envelopes.len(), "Processing Signal messages");

    let mut responses = Vec::new();

    for envelope in envelopes {
        // Extract sender info
        let sender = envelope
            .source
            .as_deref()
            .or(envelope.source_uuid.as_deref())
            .unwrap_or("unknown");

        // Check whitelist
        if !signal_adapter.is_whitelisted(sender) {
            debug!(sender = %sender, "Ignoring message from non-whitelisted sender");
            continue;
        }

        // Process based on message type
        if let Some(data_message) = envelope.data_message {
            let timestamp = data_message.timestamp;

            // Handle text messages
            if let Some(ref body) = data_message.body {
                let response =
                    handle_text_message(&state, signal_adapter, sender, timestamp, body).await;
                responses.push(response);
            }

            // Handle audio attachments
            for attachment in &data_message.attachments {
                if attachment.content_type.starts_with("audio/") {
                    let response =
                        handle_audio_message(&state, signal_adapter, sender, timestamp, attachment)
                            .await;
                    responses.push(response);
                }
            }

            // Send read receipt
            if let Err(e) = signal_adapter
                .send_read_receipt(sender, vec![timestamp])
                .await
            {
                warn!(error = %e, "Failed to send read receipt");
            }
        }
    }

    (
        StatusCode::OK,
        Json(PollResponse {
            processed: responses.len(),
            messages: responses,
            available: true,
        }),
    )
        .into_response()
}

/// Handle a text message
async fn handle_text_message(
    state: &AppState,
    signal_client: &integration_signal::SignalClient,
    from: &str,
    timestamp: i64,
    text: &str,
) -> MessageResponse {
    debug!(
        from = %from,
        timestamp = timestamp,
        text_len = text.len(),
        "Processing Signal text message"
    );

    // Parse phone number for domain type
    let phone = match PhoneNumber::new(from) {
        Ok(p) => p,
        Err(e) => {
            warn!(error = %e, from = %from, "Invalid phone number from Signal");
            return MessageResponse {
                timestamp,
                from: from.to_string(),
                status: "error".to_string(),
                response: Some("Invalid sender phone number".to_string()),
                response_type: None,
            };
        },
    };

    // Get or create conversation for this phone number
    let mut conversation = if let Some(store) = &state.conversation_store {
        match store
            .get_by_phone_number(ConversationSource::Signal, from)
            .await
        {
            Ok(Some(conv)) => {
                debug!(
                    conversation_id = %conv.id,
                    message_count = conv.messages.len(),
                    "Found existing Signal conversation for phone number"
                );
                conv
            },
            Ok(None) => {
                debug!("Creating new Signal conversation for phone number");
                Conversation::for_messenger(ConversationSource::Signal, phone.clone())
            },
            Err(e) => {
                warn!(error = %e, "Failed to load Signal conversation, creating new one");
                Conversation::for_messenger(ConversationSource::Signal, phone.clone())
            },
        }
    } else {
        // No persistence configured, create transient conversation
        Conversation::for_messenger(ConversationSource::Signal, phone)
    };

    // Add user message to conversation
    conversation.add_user_message(text);

    // Process message through agent service
    let result = state.agent_service.handle_input(text).await;

    match result {
        Ok(agent_result) => {
            let response_text = agent_result.response.clone();

            // Add assistant response to conversation
            conversation.add_assistant_message(&response_text);

            // Persist conversation
            if let Some(store) = &state.conversation_store {
                if let Err(e) = store.save(&conversation).await {
                    error!(
                        error = %e,
                        conversation_id = %conversation.id,
                        "Failed to persist Signal conversation"
                    );
                } else {
                    debug!(
                        conversation_id = %conversation.id,
                        message_count = conversation.messages.len(),
                        "Signal conversation persisted"
                    );
                }
            }

            // Send response back via Signal
            if let Err(e) = signal_client.send_text(from, &response_text).await {
                error!(
                    error = %e,
                    timestamp = timestamp,
                    from = %from,
                    "Failed to send Signal response"
                );
            } else {
                info!(
                    timestamp = timestamp,
                    from = %from,
                    conversation_id = %conversation.id,
                    success = agent_result.success,
                    "Signal text message processed and response sent"
                );
            }

            MessageResponse {
                timestamp,
                from: from.to_string(),
                status: if agent_result.success {
                    "processed".to_string()
                } else {
                    "failed".to_string()
                },
                response: Some(response_text),
                response_type: Some("text".to_string()),
            }
        },
        Err(e) => {
            error!(
                error = %e,
                timestamp = timestamp,
                from = %from,
                "Failed to process Signal text message"
            );

            // Still persist the conversation with just the user message
            if let Some(store) = &state.conversation_store {
                if let Err(persist_err) = store.save(&conversation).await {
                    warn!(
                        error = %persist_err,
                        "Failed to persist Signal conversation after processing error"
                    );
                }
            }

            MessageResponse {
                timestamp,
                from: from.to_string(),
                status: "error".to_string(),
                response: Some(format!("Processing failed: {e}")),
                response_type: Some("text".to_string()),
            }
        },
    }
}

/// Handle an audio/voice message
async fn handle_audio_message(
    state: &AppState,
    signal_client: &integration_signal::SignalClient,
    from: &str,
    timestamp: i64,
    attachment: &Attachment,
) -> MessageResponse {
    debug!(
        from = %from,
        timestamp = timestamp,
        content_type = ?attachment.content_type,
        "Processing Signal audio message"
    );

    // Check if voice message service is available
    let Some(voice_service) = &state.voice_message_service else {
        warn!("Voice message service not configured, falling back to text response");

        // Send error response
        let _ = signal_client
            .send_text(
                from,
                "Voice messages are not supported yet. Please send a text message.",
            )
            .await;

        return MessageResponse {
            timestamp,
            from: from.to_string(),
            status: "unsupported".to_string(),
            response: Some(
                "Voice messages are not supported yet. Please send a text message.".to_string(),
            ),
            response_type: Some("text".to_string()),
        };
    };

    // Get attachment file path
    let Some(file_path) = signal_client.get_attachment_path(attachment) else {
        error!(timestamp = timestamp, "No file path for attachment");
        return MessageResponse {
            timestamp,
            from: from.to_string(),
            status: "error".to_string(),
            response: Some("Failed to get audio file".to_string()),
            response_type: None,
        };
    };

    // Read audio data
    let audio_data = match tokio::fs::read(&file_path).await {
        Ok(data) => data,
        Err(e) => {
            error!(error = %e, path = %file_path, "Failed to read audio file");
            return MessageResponse {
                timestamp,
                from: from.to_string(),
                status: "error".to_string(),
                response: Some(format!("Failed to read audio: {e}")),
                response_type: None,
            };
        },
    };

    info!(
        audio_size = audio_data.len(),
        path = %file_path,
        content_type = %attachment.content_type,
        "Read audio from Signal attachment"
    );

    // Parse audio format
    let format = super::common::parse_audio_format(&attachment.content_type);

    // Create a deterministic conversation ID from phone number
    let conversation_id = super::common::conversation_id_from_phone("signal", from);

    // Process through voice message service
    let result = voice_service
        .process_voice_message(
            audio_data,
            format,
            conversation_id,
            Some(timestamp.to_string()),
        )
        .await;

    match result {
        Ok(voice_result) => {
            info!(
                timestamp = timestamp,
                transcription_len = voice_result.transcription.len(),
                response_len = voice_result.response_text.len(),
                has_audio = voice_result.response_audio.is_some(),
                processing_time_ms = voice_result.processing_time_ms,
                "Voice message processed successfully"
            );

            // Send response (audio if available, otherwise text)
            let response_type = if let Some(ref audio_response) = voice_result.response_audio {
                // Write audio to temp file and send
                match send_audio_response(signal_client, from, audio_response).await {
                    Ok(()) => "audio".to_string(),
                    Err(e) => {
                        warn!(error = %e, "Failed to send audio response, falling back to text");
                        // Fallback to text
                        let _ = signal_client
                            .send_text(from, &voice_result.response_text)
                            .await;
                        "text".to_string()
                    },
                }
            } else {
                // Send text response
                if let Err(e) = signal_client
                    .send_text(from, &voice_result.response_text)
                    .await
                {
                    error!(error = %e, "Failed to send text response");
                }
                "text".to_string()
            };

            MessageResponse {
                timestamp,
                from: from.to_string(),
                status: "processed".to_string(),
                response: Some(voice_result.response_text),
                response_type: Some(response_type),
            }
        },
        Err(e) => {
            error!(
                error = %e,
                timestamp = timestamp,
                from = %from,
                "Failed to process voice message"
            );

            // Send error message to user
            let _ = signal_client
                .send_text(
                    from,
                    "Sorry, I couldn't process your voice message. Please try again or send a text message.",
                )
                .await;

            MessageResponse {
                timestamp,
                from: from.to_string(),
                status: "error".to_string(),
                response: Some(format!("Voice processing failed: {e}")),
                response_type: None,
            }
        },
    }
}

/// Send an audio response via Signal
async fn send_audio_response(
    signal_client: &integration_signal::SignalClient,
    to: &str,
    audio_response: &SynthesisResult,
) -> Result<(), String> {
    // Write audio to temp file
    let temp_dir = std::env::temp_dir().join("pisovereign-signal");
    tokio::fs::create_dir_all(&temp_dir)
        .await
        .map_err(|e| format!("Failed to create temp dir: {e}"))?;

    let ext = super::common::format_extension(audio_response.format);
    let filename = format!("{}.{}", uuid::Uuid::new_v4(), ext);
    let temp_path = temp_dir.join(&filename);

    tokio::fs::write(&temp_path, &audio_response.audio_data)
        .await
        .map_err(|e| format!("Failed to write temp audio: {e}"))?;

    // Send audio message
    let result = signal_client
        .send_audio(to, &temp_path.to_string_lossy())
        .await
        .map_err(|e| format!("Failed to send audio: {e}"));

    // Clean up temp file
    let _ = tokio::fs::remove_file(&temp_path).await;

    result.map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn poll_query_default_timeout() {
        assert_eq!(default_timeout(), 1);
    }
}
