//! WhatsApp webhook handler
//!
//! Receives and validates webhook requests from WhatsApp Business API.

use hmac::{Hmac, Mac};
use serde::Deserialize;
use sha2::Sha256;
use tracing::warn;

type HmacSha256 = Hmac<Sha256>;

/// Webhook configuration
#[derive(Debug, Clone)]
pub struct WebhookConfig {
    /// Verify token for webhook setup
    pub verify_token: String,
    /// App secret for signature verification
    pub app_secret: String,
}

/// WhatsApp webhook entry
#[derive(Debug, Deserialize)]
pub struct WebhookPayload {
    pub object: String,
    pub entry: Vec<WebhookEntry>,
}

#[derive(Debug, Deserialize)]
pub struct WebhookEntry {
    pub id: String,
    pub changes: Vec<WebhookChange>,
}

#[derive(Debug, Deserialize)]
pub struct WebhookChange {
    pub value: WebhookValue,
    pub field: String,
}

#[derive(Debug, Deserialize)]
pub struct WebhookValue {
    pub messaging_product: String,
    pub metadata: WebhookMetadata,
    #[serde(default)]
    pub messages: Vec<WebhookMessage>,
    #[serde(default)]
    pub statuses: Vec<WebhookStatus>,
}

#[derive(Debug, Deserialize)]
pub struct WebhookMetadata {
    pub display_phone_number: String,
    pub phone_number_id: String,
}

#[derive(Debug, Deserialize)]
pub struct WebhookMessage {
    pub from: String,
    pub id: String,
    pub timestamp: String,
    #[serde(rename = "type")]
    pub msg_type: String,
    #[serde(default)]
    pub text: Option<TextMessage>,
}

#[derive(Debug, Deserialize)]
pub struct TextMessage {
    pub body: String,
}

#[derive(Debug, Deserialize)]
pub struct WebhookStatus {
    pub id: String,
    pub status: String,
    pub timestamp: String,
    pub recipient_id: String,
}

/// Verify webhook signature
pub fn verify_signature(payload: &[u8], signature: &str, secret: &str) -> bool {
    // Signature format: sha256=<hex>
    let expected_prefix = "sha256=";
    if !signature.starts_with(expected_prefix) {
        warn!("Invalid signature format");
        return false;
    }

    let signature_hex = &signature[expected_prefix.len()..];

    let Ok(mut mac) = HmacSha256::new_from_slice(secret.as_bytes()) else {
        warn!("Failed to create HMAC");
        return false;
    };

    mac.update(payload);

    let Ok(expected) = hex::decode(signature_hex) else {
        warn!("Failed to decode signature hex");
        return false;
    };

    mac.verify_slice(&expected).is_ok()
}

/// Extract messages from a webhook payload
pub fn extract_messages(payload: &WebhookPayload) -> Vec<(String, String, String)> {
    let mut messages = Vec::new();

    for entry in &payload.entry {
        for change in &entry.changes {
            if change.field == "messages" {
                for message in &change.value.messages {
                    if message.msg_type == "text" {
                        if let Some(text) = &message.text {
                            messages.push((
                                message.from.clone(),
                                message.id.clone(),
                                text.body.clone(),
                            ));
                        }
                    }
                }
            }
        }
    }

    messages
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_text_messages() {
        let payload = WebhookPayload {
            object: "whatsapp_business_account".to_string(),
            entry: vec![WebhookEntry {
                id: "123".to_string(),
                changes: vec![WebhookChange {
                    field: "messages".to_string(),
                    value: WebhookValue {
                        messaging_product: "whatsapp".to_string(),
                        metadata: WebhookMetadata {
                            display_phone_number: "+1234567890".to_string(),
                            phone_number_id: "123".to_string(),
                        },
                        messages: vec![WebhookMessage {
                            from: "+491234567890".to_string(),
                            id: "msg123".to_string(),
                            timestamp: "1234567890".to_string(),
                            msg_type: "text".to_string(),
                            text: Some(TextMessage {
                                body: "Hello!".to_string(),
                            }),
                        }],
                        statuses: vec![],
                    },
                }],
            }],
        };

        let messages = extract_messages(&payload);
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].0, "+491234567890");
        assert_eq!(messages[0].2, "Hello!");
    }
}
