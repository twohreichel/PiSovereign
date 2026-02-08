//! Signal messenger handlers
//!
//! Handles message polling from signal-cli daemon and message processing.
//! Signal uses a polling model rather than webhooks.

use application::ports::SynthesisResult;
use axum::{
    Json,
    extract::State,
    http::StatusCode,
    response::IntoResponse,
};
use domain::entities::AudioFormat;
use domain::value_objects::ConversationId;
use integration_signal::Attachment;
use serde::{Deserialize, Serialize};
use tracing::{debug, error, info, instrument, warn};

use crate::state::AppState;

/// Query parameters for message polling
#[derive(Debug, Deserialize)]
pub struct PollQuery {
    /// Timeout in seconds (0 for non-blocking, default: 1)
    #[serde(default = "default_timeout")]
    pub timeout: u64,
}

fn default_timeout() -> u64 {
    1
}

/// Response for polling messages
#[derive(Debug, Serialize)]
pub struct PollResponse {
    /// Number of messages processed
    pub processed: usize,
    /// Individual message responses
    pub messages: Vec<MessageResponse>,
    /// Whether Signal is available
    pub available: bool,
}

/// Response for a single processed message
#[derive(Debug, Serialize)]
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
#[derive(Debug, Serialize)]
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
    let signal_adapter = match state
        .signal_client
        .as_ref()
    {
        Some(client) => client,
        None => {
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
        }
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
        }
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
                let response = handle_text_message(
                    &state,
                    signal_adapter,
                    sender,
                    timestamp,
                    body,
                )
                .await;
                responses.push(response);
            }

            // Handle audio attachments
            for attachment in &data_message.attachments {
                if attachment.content_type.starts_with("audio/") {
                    let response = handle_audio_message(
                        &state,
                        signal_adapter,
                        sender,
                        timestamp,
                        attachment,
                    )
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

    // Process message through agent service
    let result = state.agent_service.handle_input(text).await;

    match result {
        Ok(agent_result) => {
            let response_text = agent_result.response.clone();

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
        }
        Err(e) => {
            error!(
                error = %e,
                timestamp = timestamp,
                from = %from,
                "Failed to process Signal text message"
            );
            MessageResponse {
                timestamp,
                from: from.to_string(),
                status: "error".to_string(),
                response: Some(format!("Processing failed: {e}")),
                response_type: Some("text".to_string()),
            }
        }
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
        }
    };

    info!(
        audio_size = audio_data.len(),
        path = %file_path,
        content_type = %attachment.content_type,
        "Read audio from Signal attachment"
    );

    // Parse audio format
    let format = parse_audio_format(&attachment.content_type);

    // Create a deterministic conversation ID from phone number
    let conversation_id = conversation_id_from_phone(from);

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
                    }
                }
            } else {
                // Send text response
                if let Err(e) = signal_client.send_text(from, &voice_result.response_text).await {
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
        }
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
        }
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

    let ext = format_extension(&audio_response.format);
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

/// Create a deterministic conversation ID from a phone number
fn conversation_id_from_phone(phone: &str) -> ConversationId {
    use std::hash::{DefaultHasher, Hash, Hasher};

    // Create a deterministic UUID from phone number hash
    let mut hasher = DefaultHasher::new();
    "signal".hash(&mut hasher);
    phone.hash(&mut hasher);
    let hash = hasher.finish();

    // Create UUID bytes from hash
    let bytes: [u8; 16] = {
        let mut b = [0u8; 16];
        b[0..8].copy_from_slice(&hash.to_be_bytes());
        b[8..16].copy_from_slice(&hash.wrapping_mul(31).to_be_bytes());
        // Set version 4 (random) and variant bits
        b[6] = (b[6] & 0x0f) | 0x40;
        b[8] = (b[8] & 0x3f) | 0x80;
        b
    };

    ConversationId::from_uuid(uuid::Uuid::from_bytes(bytes))
}

/// Parse audio format from MIME type
fn parse_audio_format(mime_type: &str) -> AudioFormat {
    let mime_lower = mime_type.to_lowercase();

    if mime_lower.contains("opus") {
        AudioFormat::Opus
    } else if mime_lower.contains("ogg") {
        AudioFormat::Ogg
    } else if mime_lower.contains("mp3") || mime_lower.contains("mpeg") {
        AudioFormat::Mp3
    } else if mime_lower.contains("wav") {
        AudioFormat::Wav
    } else {
        // Default to Ogg for Signal voice messages
        AudioFormat::Ogg
    }
}

/// Get file extension for audio format
fn format_extension(format: &AudioFormat) -> &'static str {
    match format {
        AudioFormat::Opus => "opus",
        AudioFormat::Ogg => "ogg",
        AudioFormat::Mp3 => "mp3",
        AudioFormat::Wav => "wav",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn conversation_id_deterministic() {
        let id1 = conversation_id_from_phone("+491234567890");
        let id2 = conversation_id_from_phone("+491234567890");
        assert_eq!(id1, id2);
    }

    #[test]
    fn conversation_id_differs_for_different_phones() {
        let id1 = conversation_id_from_phone("+491234567890");
        let id2 = conversation_id_from_phone("+491234567891");
        assert_ne!(id1, id2);
    }

    #[test]
    fn conversation_id_differs_from_whatsapp() {
        // Signal uses "signal" hash prefix, WhatsApp uses "whatsapp"
        // So same phone number should produce different IDs
        let signal_id = conversation_id_from_phone("+491234567890");

        // Simulate WhatsApp's conversation ID generation
        use std::hash::{DefaultHasher, Hash, Hasher};
        let mut hasher = DefaultHasher::new();
        "whatsapp".hash(&mut hasher);
        "+491234567890".hash(&mut hasher);
        let hash = hasher.finish();
        let bytes: [u8; 16] = {
            let mut b = [0u8; 16];
            b[0..8].copy_from_slice(&hash.to_be_bytes());
            b[8..16].copy_from_slice(&hash.wrapping_mul(31).to_be_bytes());
            b[6] = (b[6] & 0x0f) | 0x40;
            b[8] = (b[8] & 0x3f) | 0x80;
            b
        };
        let whatsapp_id = ConversationId::from_uuid(uuid::Uuid::from_bytes(bytes));

        assert_ne!(signal_id, whatsapp_id);
    }

    #[test]
    fn parse_audio_format_opus() {
        assert_eq!(parse_audio_format("audio/ogg; codecs=opus"), AudioFormat::Opus);
        assert_eq!(parse_audio_format("audio/opus"), AudioFormat::Opus);
    }

    #[test]
    fn parse_audio_format_ogg() {
        assert_eq!(parse_audio_format("audio/ogg"), AudioFormat::Ogg);
    }

    #[test]
    fn parse_audio_format_mp3() {
        assert_eq!(parse_audio_format("audio/mp3"), AudioFormat::Mp3);
        assert_eq!(parse_audio_format("audio/mpeg"), AudioFormat::Mp3);
    }

    #[test]
    fn parse_audio_format_wav() {
        assert_eq!(parse_audio_format("audio/wav"), AudioFormat::Wav);
    }

    #[test]
    fn parse_audio_format_unknown_defaults_to_ogg() {
        assert_eq!(parse_audio_format("audio/unknown"), AudioFormat::Ogg);
        assert_eq!(parse_audio_format("application/octet-stream"), AudioFormat::Ogg);
    }

    #[test]
    fn format_extension_returns_correct_extensions() {
        assert_eq!(format_extension(&AudioFormat::Opus), "opus");
        assert_eq!(format_extension(&AudioFormat::Ogg), "ogg");
        assert_eq!(format_extension(&AudioFormat::Mp3), "mp3");
        assert_eq!(format_extension(&AudioFormat::Wav), "wav");
    }

    #[test]
    fn poll_query_default_timeout() {
        assert_eq!(default_timeout(), 1);
    }
}
