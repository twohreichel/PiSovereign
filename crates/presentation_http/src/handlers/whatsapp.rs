//! WhatsApp webhook handlers
//!
//! Handles WhatsApp Business API webhook verification and message processing.

use axum::{
    Json,
    body::Bytes,
    extract::{Query, State},
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
};
use integration_whatsapp::{WebhookPayload, extract_messages, verify_signature};
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
    let app_secret = match &config.whatsapp.app_secret {
        Some(secret) if config.whatsapp.signature_required => secret.clone(),
        Some(secret) => secret.clone(),
        None if config.whatsapp.signature_required => {
            warn!("WhatsApp webhook message received but app_secret not configured");
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(serde_json::json!({
                    "error": "WhatsApp app_secret not configured"
                })),
            )
                .into_response();
        }
        None => String::new(), // Skip verification if not required and not configured
    };

    // Verify signature if required or if we have a secret
    if config.whatsapp.signature_required || !app_secret.is_empty() {
        let signature = headers
            .get("x-hub-signature-256")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");

        if !verify_signature(&body, signature, &app_secret) {
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
        }
    };

    // Extract messages
    let messages = extract_messages(&payload);

    if messages.is_empty() {
        // No messages - this might be a status update or other event
        debug!("No messages in webhook payload (might be status update)");
        return (StatusCode::OK, Json(serde_json::json!({"status": "ok"}))).into_response();
    }

    info!(count = messages.len(), "Processing WhatsApp messages");

    let mut responses = Vec::new();

    for (from, message_id, text) in messages {
        debug!(
            from = %from,
            message_id = %message_id,
            text_len = text.len(),
            "Processing WhatsApp message"
        );

        // Process message through agent service
        let result = state.agent_service.handle_input(&text).await;

        match result {
            Ok(agent_result) => {
                responses.push(MessageResponse {
                    message_id: message_id.clone(),
                    from: from.clone(),
                    status: if agent_result.success {
                        "processed".to_string()
                    } else {
                        "failed".to_string()
                    },
                    response: Some(agent_result.response),
                });

                // TODO: Send response back via WhatsApp API
                // This would use the WhatsAppClient to send a message
                info!(
                    message_id = %message_id,
                    from = %from,
                    success = agent_result.success,
                    "WhatsApp message processed"
                );
            }
            Err(e) => {
                error!(
                    error = %e,
                    message_id = %message_id,
                    from = %from,
                    "Failed to process WhatsApp message"
                );
                responses.push(MessageResponse {
                    message_id: message_id.clone(),
                    from: from.clone(),
                    status: "error".to_string(),
                    response: Some(format!("Processing failed: {e}")),
                });
            }
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
        };

        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("msg123"));
        assert!(json.contains("+491234567890"));
        assert!(json.contains("processed"));
    }

    #[test]
    fn message_response_skips_none_response() {
        let response = MessageResponse {
            message_id: "msg123".to_string(),
            from: "+49123".to_string(),
            status: "ok".to_string(),
            response: None,
        };

        let json = serde_json::to_string(&response).unwrap();
        assert!(!json.contains("response"));
    }
}
