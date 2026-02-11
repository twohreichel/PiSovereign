//! WhatsApp webhook handlers
//!
//! Handles WhatsApp Business API webhook verification and message processing.
//! Supports both text and audio (voice) messages.

use axum::{
    Json,
    body::Bytes,
    extract::{Query, State},
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
};
use domain::entities::AudioFormat;
use domain::value_objects::ConversationId;
use integration_whatsapp::{
    IncomingMessage, WebhookPayload, WhatsAppClient, WhatsAppClientConfig, extract_all_messages,
    verify_signature,
};
use secrecy::ExposeSecret;
use serde::{Deserialize, Serialize};
use tracing::{debug, error, info, instrument, warn};

use crate::state::AppState;

/// Query parameters for webhook verification
#[derive(Debug, Deserialize)]
pub struct WebhookVerifyQuery {
    /// The mode (should be "subscribe")
    #[serde(rename = "hub.mode")]
    pub hub_mode: Option<String>,
    /// The verify token to validate
    #[serde(rename = "hub.verify_token")]
    pub hub_verify_token: Option<String>,
    /// The challenge to return on success
    #[serde(rename = "hub.challenge")]
    pub hub_challenge: Option<String>,
}

/// Response for incoming messages
#[derive(Debug, Serialize)]
pub struct MessageResponse {
    /// Message ID
    pub message_id: String,
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

/// WhatsApp webhook verification (GET)
///
/// Meta sends a GET request to verify webhook ownership during setup.
/// We must verify the token and return the challenge.
#[instrument(skip(state, query))]
pub async fn verify_webhook(
    State(state): State<AppState>,
    Query(query): Query<WebhookVerifyQuery>,
) -> impl IntoResponse {
    let config = state.config.load();

    // Check if WhatsApp integration is configured
    let Some(ref verify_token) = config.whatsapp.verify_token else {
        warn!("WhatsApp webhook verification attempted but verify_token not configured");
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            "WhatsApp integration not configured",
        )
            .into_response();
    };

    // Verify the mode is "subscribe"
    let Some(mode) = query.hub_mode else {
        debug!("Missing hub.mode in webhook verification");
        return (StatusCode::BAD_REQUEST, "Missing hub.mode").into_response();
    };

    if mode != "subscribe" {
        debug!(mode = %mode, "Invalid hub.mode");
        return (StatusCode::BAD_REQUEST, "Invalid hub.mode").into_response();
    }

    // Verify the token matches
    let Some(hub_verify_token) = query.hub_verify_token else {
        debug!("Missing hub.verify_token");
        return (StatusCode::BAD_REQUEST, "Missing hub.verify_token").into_response();
    };

    if &hub_verify_token != verify_token {
        warn!("WhatsApp webhook verification failed: token mismatch");
        return (StatusCode::FORBIDDEN, "Token mismatch").into_response();
    }

    // Return the challenge
    let Some(challenge) = query.hub_challenge else {
        debug!("Missing hub.challenge");
        return (StatusCode::BAD_REQUEST, "Missing hub.challenge").into_response();
    };

    info!("WhatsApp webhook verified successfully");
    (StatusCode::OK, challenge).into_response()
}

/// WhatsApp webhook message handler (POST)
///
/// Receives incoming messages from WhatsApp Business API.
/// Must verify signature and process messages.
#[instrument(skip(state, headers, body))]
pub async fn handle_webhook(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Bytes,
) -> impl IntoResponse {
    let config = state.config.load();

    // Check if WhatsApp app_secret is configured for signature verification
    let app_secret_str: Option<String> = match &config.whatsapp.app_secret {
        Some(secret) if config.whatsapp.signature_required => {
            Some(secret.expose_secret().to_string())
        },
        Some(secret) => Some(secret.expose_secret().to_string()),
        None if config.whatsapp.signature_required => {
            warn!("WhatsApp webhook message received but app_secret not configured");
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(serde_json::json!({
                    "error": "WhatsApp app_secret not configured"
                })),
            )
                .into_response();
        },
        None => None, // Skip verification if not required and not configured
    };

    // Verify signature if required or if we have a secret
    if config.whatsapp.signature_required || app_secret_str.is_some() {
        let signature = headers
            .get("x-hub-signature-256")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");

        if !verify_signature(&body, signature, app_secret_str.as_deref().unwrap_or("")) {
            warn!("WhatsApp webhook signature verification failed");
            return (
                StatusCode::UNAUTHORIZED,
                Json(serde_json::json!({
                    "error": "Invalid signature"
                })),
            )
                .into_response();
        }
    }

    // Parse payload
    let payload: WebhookPayload = match serde_json::from_slice(&body) {
        Ok(p) => p,
        Err(e) => {
            error!(error = %e, "Failed to parse WhatsApp webhook payload");
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({
                    "error": format!("Invalid payload: {}", e)
                })),
            )
                .into_response();
        },
    };

    // Extract all messages (text and audio)
    let messages = extract_all_messages(&payload);

    if messages.is_empty() {
        // No messages - this might be a status update or other event
        debug!("No messages in webhook payload (might be status update)");
        return (StatusCode::OK, Json(serde_json::json!({"status": "ok"}))).into_response();
    }

    info!(count = messages.len(), "Processing WhatsApp messages");

    let mut responses = Vec::new();

    for message in messages {
        match message {
            IncomingMessage::Text {
                from,
                message_id,
                body,
            } => {
                let response = handle_text_message(&state, &from, &message_id, &body).await;
                responses.push(response);
            },
            IncomingMessage::Audio {
                from,
                message_id,
                media_id,
                mime_type,
                is_voice,
            } => {
                let response = handle_audio_message(
                    &state,
                    &from,
                    &message_id,
                    &media_id,
                    &mime_type,
                    is_voice,
                )
                .await;
                responses.push(response);
            },
        }
    }

    (
        StatusCode::OK,
        Json(serde_json::json!({
            "status": "ok",
            "processed": responses.len(),
            "messages": responses
        })),
    )
        .into_response()
}

/// Handle a text message
async fn handle_text_message(
    state: &AppState,
    from: &str,
    message_id: &str,
    text: &str,
) -> MessageResponse {
    debug!(
        from = %from,
        message_id = %message_id,
        text_len = text.len(),
        "Processing WhatsApp text message"
    );

    // Process message through agent service
    let result = state.agent_service.handle_input(text).await;
    let config = state.config.load();

    match result {
        Ok(agent_result) => {
            let response_text = agent_result.response.clone();

            // Send response back via WhatsApp API
            if let Err(e) = send_whatsapp_response(&config.whatsapp, from, &response_text).await {
                error!(
                    error = %e,
                    message_id = %message_id,
                    from = %from,
                    "Failed to send WhatsApp response"
                );
            } else {
                info!(
                    message_id = %message_id,
                    from = %from,
                    success = agent_result.success,
                    "WhatsApp text message processed and response sent"
                );
            }

            MessageResponse {
                message_id: message_id.to_string(),
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
                message_id = %message_id,
                from = %from,
                "Failed to process WhatsApp text message"
            );
            MessageResponse {
                message_id: message_id.to_string(),
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
    from: &str,
    message_id: &str,
    media_id: &str,
    mime_type: &str,
    is_voice: bool,
) -> MessageResponse {
    debug!(
        from = %from,
        message_id = %message_id,
        media_id = %media_id,
        mime_type = %mime_type,
        is_voice = is_voice,
        "Processing WhatsApp audio message"
    );

    let config = state.config.load();

    // Check if voice message service is available
    let Some(voice_service) = &state.voice_message_service else {
        warn!("Voice message service not configured, falling back to text response");
        return MessageResponse {
            message_id: message_id.to_string(),
            from: from.to_string(),
            status: "unsupported".to_string(),
            response: Some(
                "Voice messages are not supported yet. Please send a text message.".to_string(),
            ),
            response_type: Some("text".to_string()),
        };
    };

    // Create WhatsApp client for media operations
    let client = match create_whatsapp_client(&config.whatsapp) {
        Ok(c) => c,
        Err(e) => {
            error!(error = %e, "Failed to create WhatsApp client");
            return MessageResponse {
                message_id: message_id.to_string(),
                from: from.to_string(),
                status: "error".to_string(),
                response: Some(format!("Configuration error: {e}")),
                response_type: None,
            };
        },
    };

    // Download audio from WhatsApp
    let downloaded = match client.download_media(media_id).await {
        Ok(data) => data,
        Err(e) => {
            error!(error = %e, media_id = %media_id, "Failed to download audio");
            return MessageResponse {
                message_id: message_id.to_string(),
                from: from.to_string(),
                status: "error".to_string(),
                response: Some(format!("Failed to download audio: {e}")),
                response_type: None,
            };
        },
    };

    info!(
        audio_size = downloaded.data.len(),
        media_id = %media_id,
        mime_type = %downloaded.mime_type,
        "Downloaded audio from WhatsApp"
    );

    // Parse audio format from MIME type (prefer downloaded MIME type)
    let format = parse_audio_format(&downloaded.mime_type);

    // Create a deterministic conversation ID from phone number
    // This allows continuing conversations
    let conversation_id = conversation_id_from_phone(from);

    // Process through voice message service
    let result = voice_service
        .process_voice_message(
            downloaded.data,
            format,
            conversation_id,
            Some(message_id.to_string()),
        )
        .await;

    match result {
        Ok(voice_result) => {
            info!(
                message_id = %message_id,
                transcription_len = voice_result.transcription.len(),
                response_len = voice_result.response_text.len(),
                has_audio = voice_result.response_audio.is_some(),
                processing_time_ms = voice_result.processing_time_ms,
                "Voice message processed successfully"
            );

            // Send response (audio if available, otherwise text)
            let response_type = if let Some(ref audio_response) = voice_result.response_audio {
                // Upload audio and send as audio message
                match upload_and_send_audio(
                    &client,
                    from,
                    &audio_response.audio_data,
                    &audio_response.format,
                )
                .await
                {
                    Ok(()) => "audio".to_string(),
                    Err(e) => {
                        warn!(error = %e, "Failed to send audio response, falling back to text");
                        // Fallback to text
                        if let Err(e) = send_whatsapp_response(
                            &config.whatsapp,
                            from,
                            &voice_result.response_text,
                        )
                        .await
                        {
                            error!(error = %e, "Failed to send text fallback");
                        }
                        "text".to_string()
                    },
                }
            } else {
                // Send text response
                if let Err(e) =
                    send_whatsapp_response(&config.whatsapp, from, &voice_result.response_text)
                        .await
                {
                    error!(error = %e, "Failed to send text response");
                }
                "text".to_string()
            };

            MessageResponse {
                message_id: message_id.to_string(),
                from: from.to_string(),
                status: "processed".to_string(),
                response: Some(voice_result.response_text),
                response_type: Some(response_type),
            }
        },
        Err(e) => {
            error!(
                error = %e,
                message_id = %message_id,
                from = %from,
                "Failed to process voice message"
            );

            // Send error message to user
            let error_msg = "Sorry, I couldn't process your voice message. Please try again or send a text message.";
            let _ = send_whatsapp_response(&config.whatsapp, from, error_msg).await;

            MessageResponse {
                message_id: message_id.to_string(),
                from: from.to_string(),
                status: "error".to_string(),
                response: Some(format!("Voice processing failed: {e}")),
                response_type: None,
            }
        },
    }
}

/// Create a deterministic conversation ID from a phone number
fn conversation_id_from_phone(phone: &str) -> ConversationId {
    use std::hash::{DefaultHasher, Hash, Hasher};

    // Create a deterministic UUID from phone number hash
    let mut hasher = DefaultHasher::new();
    "whatsapp".hash(&mut hasher);
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

/// Upload audio to WhatsApp and send it as an audio message
async fn upload_and_send_audio(
    client: &WhatsAppClient,
    to: &str,
    audio_data: &[u8],
    format: &AudioFormat,
) -> Result<(), String> {
    let mime_type = format_to_mime(*format);
    let filename = format!("response.{}", format_extension(*format));

    // Upload the audio
    let upload_result = client
        .upload_media(audio_data.to_vec(), &mime_type, &filename)
        .await
        .map_err(|e| format!("Failed to upload audio: {e}"))?;

    // Send the audio message
    client
        .send_audio_message(to, &upload_result.id)
        .await
        .map_err(|e| format!("Failed to send audio message: {e}"))?;

    Ok(())
}

/// Get file extension for audio format
const fn format_extension(format: AudioFormat) -> &'static str {
    match format {
        AudioFormat::Opus => "opus",
        AudioFormat::Ogg => "ogg",
        AudioFormat::Mp3 => "mp3",
        AudioFormat::Wav => "wav",
    }
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
        // Default to Ogg for WhatsApp voice messages
        AudioFormat::Ogg
    }
}

/// Convert audio format to MIME type
fn format_to_mime(format: AudioFormat) -> String {
    match format {
        AudioFormat::Opus => "audio/ogg; codecs=opus".to_string(),
        AudioFormat::Ogg => "audio/ogg".to_string(),
        AudioFormat::Mp3 => "audio/mpeg".to_string(),
        AudioFormat::Wav => "audio/wav".to_string(),
    }
}

/// Create a WhatsApp client from configuration
fn create_whatsapp_client(
    config: &infrastructure::WhatsAppConfig,
) -> Result<WhatsAppClient, String> {
    let access_token = config
        .access_token
        .as_ref()
        .ok_or("WhatsApp access_token not configured")?;
    let phone_number_id = config
        .phone_number_id
        .as_ref()
        .ok_or("WhatsApp phone_number_id not configured")?;

    let client_config = WhatsAppClientConfig {
        access_token: access_token.expose_secret().to_string(),
        phone_number_id: phone_number_id.clone(),
        app_secret: config
            .app_secret
            .as_ref()
            .map(|s| s.expose_secret().to_string())
            .unwrap_or_default(),
        verify_token: config.verify_token.clone().unwrap_or_default(),
        signature_required: config.signature_required,
        api_version: config.api_version.clone(),
    };

    WhatsAppClient::new(client_config).map_err(|e| e.to_string())
}

/// Send a response message via WhatsApp Cloud API
///
/// Creates a WhatsApp client from configuration and sends the message.
/// Returns an error if WhatsApp is not properly configured or if sending fails.
async fn send_whatsapp_response(
    config: &infrastructure::WhatsAppConfig,
    to: &str,
    message: &str,
) -> Result<(), String> {
    // Validate required configuration
    let access_token = config
        .access_token
        .as_ref()
        .ok_or("WhatsApp access_token not configured")?;
    let phone_number_id = config
        .phone_number_id
        .as_ref()
        .ok_or("WhatsApp phone_number_id not configured")?;

    let client_config = WhatsAppClientConfig {
        access_token: access_token.expose_secret().to_string(),
        phone_number_id: phone_number_id.clone(),
        app_secret: config
            .app_secret
            .as_ref()
            .map(|s| s.expose_secret().to_string())
            .unwrap_or_default(),
        verify_token: config.verify_token.clone().unwrap_or_default(),
        signature_required: config.signature_required,
        api_version: config.api_version.clone(),
    };

    let client = WhatsAppClient::new(client_config).map_err(|e| e.to_string())?;

    client
        .send_message(to, message)
        .await
        .map_err(|e| e.to_string())?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn verify_query_fields() {
        // Test that WebhookVerifyQuery has the right structure
        let params = WebhookVerifyQuery {
            hub_mode: Some("subscribe".to_string()),
            hub_verify_token: Some("my_token".to_string()),
            hub_challenge: Some("challenge123".to_string()),
        };

        assert_eq!(params.hub_mode, Some("subscribe".to_string()));
        assert_eq!(params.hub_verify_token, Some("my_token".to_string()));
        assert_eq!(params.hub_challenge, Some("challenge123".to_string()));
    }

    #[test]
    fn verify_query_optional_fields() {
        let params = WebhookVerifyQuery {
            hub_mode: None,
            hub_verify_token: None,
            hub_challenge: None,
        };

        assert!(params.hub_mode.is_none());
        assert!(params.hub_verify_token.is_none());
        assert!(params.hub_challenge.is_none());
    }

    #[test]
    fn message_response_serializes() {
        let response = MessageResponse {
            message_id: "msg123".to_string(),
            from: "+491234567890".to_string(),
            status: "processed".to_string(),
            response: Some("Hello!".to_string()),
            response_type: Some("text".to_string()),
        };

        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("msg123"));
        assert!(json.contains("+491234567890"));
        assert!(json.contains("processed"));
        assert!(json.contains("text"));
    }

    #[test]
    fn message_response_skips_none_response() {
        let response = MessageResponse {
            message_id: "msg123".to_string(),
            from: "+49123".to_string(),
            status: "ok".to_string(),
            response: None,
            response_type: None,
        };

        let json = serde_json::to_string(&response).unwrap();
        assert!(!json.contains("response"));
        assert!(!json.contains("response_type"));
    }

    #[test]
    fn message_response_with_audio_type() {
        let response = MessageResponse {
            message_id: "msg123".to_string(),
            from: "+49123".to_string(),
            status: "processed".to_string(),
            response: Some("Transcribed text".to_string()),
            response_type: Some("audio".to_string()),
        };

        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("audio"));
        assert!(json.contains("Transcribed text"));
    }

    #[test]
    fn parse_audio_format_opus() {
        assert!(matches!(
            parse_audio_format("audio/ogg; codecs=opus"),
            AudioFormat::Opus
        ));
        assert!(matches!(
            parse_audio_format("audio/opus"),
            AudioFormat::Opus
        ));
    }

    #[test]
    fn parse_audio_format_ogg() {
        assert!(matches!(parse_audio_format("audio/ogg"), AudioFormat::Ogg));
    }

    #[test]
    fn parse_audio_format_mp3() {
        assert!(matches!(parse_audio_format("audio/mp3"), AudioFormat::Mp3));
        assert!(matches!(parse_audio_format("audio/mpeg"), AudioFormat::Mp3));
    }

    #[test]
    fn parse_audio_format_wav() {
        assert!(matches!(parse_audio_format("audio/wav"), AudioFormat::Wav));
    }

    #[test]
    fn parse_audio_format_unknown_defaults_to_ogg() {
        assert!(matches!(
            parse_audio_format("audio/unknown"),
            AudioFormat::Ogg
        ));
        assert!(matches!(parse_audio_format("video/mp4"), AudioFormat::Ogg));
    }

    #[test]
    fn format_to_mime_conversions() {
        assert_eq!(format_to_mime(AudioFormat::Opus), "audio/ogg; codecs=opus");
        assert_eq!(format_to_mime(AudioFormat::Ogg), "audio/ogg");
        assert_eq!(format_to_mime(AudioFormat::Mp3), "audio/mpeg");
        assert_eq!(format_to_mime(AudioFormat::Wav), "audio/wav");
    }

    #[test]
    fn create_whatsapp_client_fails_without_access_token() {
        let config = infrastructure::WhatsAppConfig::default();
        let result = create_whatsapp_client(&config);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("access_token"));
    }

    #[test]
    fn create_whatsapp_client_fails_without_phone_number_id() {
        use secrecy::SecretString;

        let config = infrastructure::WhatsAppConfig {
            access_token: Some(SecretString::from("test_token")),
            phone_number_id: None,
            ..Default::default()
        };
        let result = create_whatsapp_client(&config);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("phone_number_id"));
    }

    #[tokio::test]
    async fn send_whatsapp_response_fails_without_access_token() {
        let config = infrastructure::WhatsAppConfig::default();
        let result = send_whatsapp_response(&config, "+491234567890", "Hello").await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("access_token"));
    }

    #[tokio::test]
    async fn send_whatsapp_response_fails_without_phone_number_id() {
        use secrecy::SecretString;

        let config = infrastructure::WhatsAppConfig {
            access_token: Some(SecretString::from("test_token")),
            phone_number_id: None,
            ..Default::default()
        };
        let result = send_whatsapp_response(&config, "+491234567890", "Hello").await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("phone_number_id"));
    }

    #[test]
    fn whatsapp_client_config_conversion() {
        use secrecy::SecretString;

        let config = infrastructure::WhatsAppConfig {
            access_token: Some(SecretString::from("token123")),
            phone_number_id: Some("phone123".to_string()),
            app_secret: Some(SecretString::from("secret")),
            verify_token: Some("verify".to_string()),
            signature_required: true,
            api_version: "v18.0".to_string(),
            whitelist: Vec::new(),
            persistence: infrastructure::MessengerPersistenceConfig::default(),
        };

        let client_config = WhatsAppClientConfig {
            access_token: config
                .access_token
                .as_ref()
                .map(|s| s.expose_secret().to_string())
                .unwrap(),
            phone_number_id: config.phone_number_id.clone().unwrap(),
            app_secret: config
                .app_secret
                .as_ref()
                .map(|s| s.expose_secret().to_string())
                .unwrap_or_default(),
            verify_token: config.verify_token.clone().unwrap_or_default(),
            signature_required: config.signature_required,
            api_version: config.api_version,
        };

        assert_eq!(client_config.access_token, "token123");
        assert_eq!(client_config.phone_number_id, "phone123");
        assert_eq!(client_config.app_secret, "secret");
        assert_eq!(client_config.verify_token, "verify");
        assert!(client_config.signature_required);
        assert_eq!(client_config.api_version, "v18.0");
    }
}
