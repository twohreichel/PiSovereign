//! WhatsApp webhook handler
//!
//! Receives and validates webhook requests from WhatsApp Business API.
//! Supports both text and audio (voice) messages.

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
    #[serde(default)]
    pub audio: Option<AudioMessage>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TextMessage {
    pub body: String,
}

/// Audio message metadata from WhatsApp
#[derive(Debug, Clone, Deserialize)]
pub struct AudioMessage {
    /// Media ID for downloading the audio file
    pub id: String,
    /// MIME type (e.g., "audio/ogg; codecs=opus")
    pub mime_type: String,
    /// Whether this is a voice message (vs. uploaded audio file)
    #[serde(default)]
    pub voice: bool,
}

#[derive(Debug, Deserialize)]
pub struct WebhookStatus {
    pub id: String,
    pub status: String,
    pub timestamp: String,
    pub recipient_id: String,
}

/// Extracted message types from webhook
#[derive(Debug, Clone)]
pub enum IncomingMessage {
    /// Text message
    Text {
        from: String,
        message_id: String,
        body: String,
    },
    /// Audio/voice message
    Audio {
        from: String,
        message_id: String,
        media_id: String,
        mime_type: String,
        is_voice: bool,
    },
}

impl IncomingMessage {
    /// Get the sender phone number
    #[must_use]
    pub fn from(&self) -> &str {
        match self {
            Self::Text { from, .. } | Self::Audio { from, .. } => from,
        }
    }

    /// Get the message ID
    #[must_use]
    pub fn message_id(&self) -> &str {
        match self {
            Self::Text { message_id, .. } | Self::Audio { message_id, .. } => message_id,
        }
    }

    /// Check if this is a text message
    #[must_use]
    pub const fn is_text(&self) -> bool {
        matches!(self, Self::Text { .. })
    }

    /// Check if this is an audio message
    #[must_use]
    pub const fn is_audio(&self) -> bool {
        matches!(self, Self::Audio { .. })
    }

    /// Check if this is a voice message (audio recorded in app)
    #[must_use]
    pub const fn is_voice(&self) -> bool {
        matches!(self, Self::Audio { is_voice: true, .. })
    }
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

/// Extract text messages from a webhook payload (legacy function for backwards compatibility)
///
/// Returns tuples of (from, message_id, body)
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

/// Extract all messages (text and audio) from a webhook payload
///
/// Returns a list of `IncomingMessage` variants for both text and audio messages.
pub fn extract_all_messages(payload: &WebhookPayload) -> Vec<IncomingMessage> {
    let mut messages = Vec::new();

    for entry in &payload.entry {
        for change in &entry.changes {
            if change.field == "messages" {
                for message in &change.value.messages {
                    match message.msg_type.as_str() {
                        "text" => {
                            if let Some(text) = &message.text {
                                messages.push(IncomingMessage::Text {
                                    from: message.from.clone(),
                                    message_id: message.id.clone(),
                                    body: text.body.clone(),
                                });
                            }
                        },
                        "audio" => {
                            if let Some(audio) = &message.audio {
                                messages.push(IncomingMessage::Audio {
                                    from: message.from.clone(),
                                    message_id: message.id.clone(),
                                    media_id: audio.id.clone(),
                                    mime_type: audio.mime_type.clone(),
                                    is_voice: audio.voice,
                                });
                            }
                        },
                        _ => {
                            // Ignore other message types (image, video, etc.)
                        },
                    }
                }
            }
        }
    }

    messages
}

/// Extract only audio messages from a webhook payload
pub fn extract_audio_messages(payload: &WebhookPayload) -> Vec<IncomingMessage> {
    extract_all_messages(payload)
        .into_iter()
        .filter(IncomingMessage::is_audio)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_payload(messages: Vec<WebhookMessage>) -> WebhookPayload {
        WebhookPayload {
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
                        messages,
                        statuses: vec![],
                    },
                }],
            }],
        }
    }

    fn create_text_message(from: &str, id: &str, body: &str) -> WebhookMessage {
        WebhookMessage {
            from: from.to_string(),
            id: id.to_string(),
            timestamp: "1234567890".to_string(),
            msg_type: "text".to_string(),
            text: Some(TextMessage {
                body: body.to_string(),
            }),
            audio: None,
        }
    }

    fn create_audio_message(
        from: &str,
        id: &str,
        media_id: &str,
        is_voice: bool,
    ) -> WebhookMessage {
        WebhookMessage {
            from: from.to_string(),
            id: id.to_string(),
            timestamp: "1234567890".to_string(),
            msg_type: "audio".to_string(),
            text: None,
            audio: Some(AudioMessage {
                id: media_id.to_string(),
                mime_type: "audio/ogg; codecs=opus".to_string(),
                voice: is_voice,
            }),
        }
    }

    mod text_message_tests {
        use super::*;

        #[test]
        fn extracts_text_messages() {
            let payload = create_test_payload(vec![create_text_message(
                "+491234567890",
                "msg123",
                "Hello!",
            )]);

            let messages = extract_messages(&payload);
            assert_eq!(messages.len(), 1);
            assert_eq!(messages[0].0, "+491234567890");
            assert_eq!(messages[0].2, "Hello!");
        }

        #[test]
        fn extracts_message_id() {
            let payload =
                create_test_payload(vec![create_text_message("+49123", "unique-msg-id", "Test")]);

            let messages = extract_messages(&payload);
            assert_eq!(messages[0].1, "unique-msg-id");
        }

        #[test]
        fn extracts_multiple_messages() {
            let payload = create_test_payload(vec![
                create_text_message("+491111", "msg1", "First"),
                create_text_message("+492222", "msg2", "Second"),
            ]);

            let messages = extract_messages(&payload);
            assert_eq!(messages.len(), 2);
            assert_eq!(messages[0].2, "First");
            assert_eq!(messages[1].2, "Second");
        }

        #[test]
        fn ignores_non_text_messages() {
            let payload = create_test_payload(vec![WebhookMessage {
                from: "+49123".to_string(),
                id: "msg1".to_string(),
                timestamp: "1234567890".to_string(),
                msg_type: "image".to_string(),
                text: None,
                audio: None,
            }]);

            let messages = extract_messages(&payload);
            assert!(messages.is_empty());
        }

        #[test]
        fn handles_empty_messages() {
            let payload = create_test_payload(vec![]);
            let messages = extract_messages(&payload);
            assert!(messages.is_empty());
        }
    }

    mod audio_message_tests {
        use super::*;

        #[test]
        fn extracts_voice_messages() {
            let payload = create_test_payload(vec![create_audio_message(
                "+491234567890",
                "msg123",
                "media-id-456",
                true,
            )]);

            let messages = extract_all_messages(&payload);
            assert_eq!(messages.len(), 1);

            match &messages[0] {
                IncomingMessage::Audio {
                    from,
                    message_id,
                    media_id,
                    is_voice,
                    ..
                } => {
                    assert_eq!(from, "+491234567890");
                    assert_eq!(message_id, "msg123");
                    assert_eq!(media_id, "media-id-456");
                    assert!(is_voice);
                },
                IncomingMessage::Text { .. } => unreachable!("Expected Audio message"),
            }
        }

        #[test]
        fn extracts_uploaded_audio() {
            let payload = create_test_payload(vec![create_audio_message(
                "+49123", "msg1", "media-1", false,
            )]);

            let messages = extract_all_messages(&payload);
            assert_eq!(messages.len(), 1);

            match &messages[0] {
                IncomingMessage::Audio { is_voice, .. } => {
                    assert!(!is_voice);
                },
                IncomingMessage::Text { .. } => unreachable!("Expected Audio message"),
            }
        }

        #[test]
        fn extract_audio_messages_filters_correctly() {
            let payload = create_test_payload(vec![
                create_text_message("+491111", "msg1", "Text message"),
                create_audio_message("+492222", "msg2", "media-1", true),
                create_text_message("+493333", "msg3", "Another text"),
            ]);

            let audio_messages = extract_audio_messages(&payload);
            assert_eq!(audio_messages.len(), 1);
            assert!(audio_messages[0].is_audio());
        }

        #[test]
        fn audio_message_mime_type() {
            let payload = create_test_payload(vec![create_audio_message(
                "+49123", "msg1", "media-1", true,
            )]);

            let messages = extract_all_messages(&payload);
            match &messages[0] {
                IncomingMessage::Audio { mime_type, .. } => {
                    assert_eq!(mime_type, "audio/ogg; codecs=opus");
                },
                IncomingMessage::Text { .. } => unreachable!("Expected Audio message"),
            }
        }
    }

    mod mixed_message_tests {
        use super::*;

        #[test]
        fn extracts_both_text_and_audio() {
            let payload = create_test_payload(vec![
                create_text_message("+491111", "msg1", "Hello"),
                create_audio_message("+492222", "msg2", "media-1", true),
            ]);

            let messages = extract_all_messages(&payload);
            assert_eq!(messages.len(), 2);
            assert!(messages[0].is_text());
            assert!(messages[1].is_audio());
        }

        #[test]
        fn incoming_message_from_method() {
            let text = IncomingMessage::Text {
                from: "+491111".to_string(),
                message_id: "msg1".to_string(),
                body: "Test".to_string(),
            };

            let audio = IncomingMessage::Audio {
                from: "+492222".to_string(),
                message_id: "msg2".to_string(),
                media_id: "media-1".to_string(),
                mime_type: "audio/opus".to_string(),
                is_voice: true,
            };

            assert_eq!(text.from(), "+491111");
            assert_eq!(audio.from(), "+492222");
        }

        #[test]
        fn incoming_message_message_id_method() {
            let text = IncomingMessage::Text {
                from: "+491111".to_string(),
                message_id: "text-msg-id".to_string(),
                body: "Test".to_string(),
            };

            let audio = IncomingMessage::Audio {
                from: "+492222".to_string(),
                message_id: "audio-msg-id".to_string(),
                media_id: "media-1".to_string(),
                mime_type: "audio/opus".to_string(),
                is_voice: true,
            };

            assert_eq!(text.message_id(), "text-msg-id");
            assert_eq!(audio.message_id(), "audio-msg-id");
        }

        #[test]
        fn incoming_message_type_checks() {
            let text = IncomingMessage::Text {
                from: "+49".to_string(),
                message_id: "1".to_string(),
                body: "Test".to_string(),
            };

            let voice = IncomingMessage::Audio {
                from: "+49".to_string(),
                message_id: "2".to_string(),
                media_id: "m".to_string(),
                mime_type: "audio/opus".to_string(),
                is_voice: true,
            };

            let audio_file = IncomingMessage::Audio {
                from: "+49".to_string(),
                message_id: "3".to_string(),
                media_id: "m".to_string(),
                mime_type: "audio/mp3".to_string(),
                is_voice: false,
            };

            assert!(text.is_text());
            assert!(!text.is_audio());
            assert!(!text.is_voice());

            assert!(!voice.is_text());
            assert!(voice.is_audio());
            assert!(voice.is_voice());

            assert!(!audio_file.is_text());
            assert!(audio_file.is_audio());
            assert!(!audio_file.is_voice());
        }
    }

    mod signature_tests {
        use super::*;

        #[test]
        fn verify_signature_valid() {
            let secret = "test_secret";
            let payload = b"test payload";
            let mut mac = HmacSha256::new_from_slice(secret.as_bytes()).unwrap();
            mac.update(payload);
            let signature = format!("sha256={}", hex::encode(mac.finalize().into_bytes()));

            assert!(verify_signature(payload, &signature, secret));
        }

        #[test]
        fn verify_signature_invalid() {
            let secret = "test_secret";
            let payload = b"test payload";
            let wrong_signature =
                "sha256=0000000000000000000000000000000000000000000000000000000000000000";

            assert!(!verify_signature(payload, wrong_signature, secret));
        }

        #[test]
        fn verify_signature_wrong_format() {
            let payload = b"test";
            assert!(!verify_signature(payload, "invalid", "secret"));
            assert!(!verify_signature(payload, "md5=abc", "secret"));
        }

        #[test]
        fn verify_signature_invalid_hex() {
            let payload = b"test";
            assert!(!verify_signature(payload, "sha256=notahex", "secret"));
        }
    }

    mod config_tests {
        use super::*;

        #[test]
        fn webhook_config_creation() {
            let config = WebhookConfig {
                verify_token: "token".to_string(),
                app_secret: "secret".to_string(),
            };
            assert_eq!(config.verify_token, "token");
            assert_eq!(config.app_secret, "secret");
        }

        #[test]
        fn webhook_config_has_debug() {
            let config = WebhookConfig {
                verify_token: "token".to_string(),
                app_secret: "secret".to_string(),
            };
            let debug = format!("{config:?}");
            assert!(debug.contains("WebhookConfig"));
        }
    }

    mod deserialization_tests {
        use super::*;

        #[test]
        fn webhook_payload_deserialization() {
            let json = r#"{
                "object": "whatsapp_business_account",
                "entry": [{
                    "id": "123",
                    "changes": [{
                        "field": "messages",
                        "value": {
                            "messaging_product": "whatsapp",
                            "metadata": {
                                "display_phone_number": "+1234567890",
                                "phone_number_id": "123"
                            },
                            "messages": [{
                                "from": "+491234567890",
                                "id": "msg123",
                                "timestamp": "1234567890",
                                "type": "text",
                                "text": {"body": "Hello!"}
                            }]
                        }
                    }]
                }]
            }"#;

            let payload: WebhookPayload = serde_json::from_str(json).unwrap();
            assert_eq!(payload.object, "whatsapp_business_account");
            assert_eq!(payload.entry.len(), 1);
        }

        #[test]
        fn webhook_audio_message_deserialization() {
            let json = r#"{
                "object": "whatsapp_business_account",
                "entry": [{
                    "id": "123",
                    "changes": [{
                        "field": "messages",
                        "value": {
                            "messaging_product": "whatsapp",
                            "metadata": {
                                "display_phone_number": "+1234567890",
                                "phone_number_id": "123"
                            },
                            "messages": [{
                                "from": "+491234567890",
                                "id": "msg123",
                                "timestamp": "1234567890",
                                "type": "audio",
                                "audio": {
                                    "id": "media-123",
                                    "mime_type": "audio/ogg; codecs=opus",
                                    "voice": true
                                }
                            }]
                        }
                    }]
                }]
            }"#;

            let payload: WebhookPayload = serde_json::from_str(json).unwrap();
            let messages = extract_all_messages(&payload);
            assert_eq!(messages.len(), 1);
            assert!(messages[0].is_voice());
        }

        #[test]
        fn webhook_status_deserialization() {
            let json = r#"{
                "object": "whatsapp_business_account",
                "entry": [{
                    "id": "123",
                    "changes": [{
                        "field": "messages",
                        "value": {
                            "messaging_product": "whatsapp",
                            "metadata": {
                                "display_phone_number": "+1234567890",
                                "phone_number_id": "123"
                            },
                            "statuses": [{
                                "id": "msg123",
                                "status": "delivered",
                                "timestamp": "1234567890",
                                "recipient_id": "+49123"
                            }]
                        }
                    }]
                }]
            }"#;

            let payload: WebhookPayload = serde_json::from_str(json).unwrap();
            let statuses = &payload.entry[0].changes[0].value.statuses;
            assert_eq!(statuses.len(), 1);
            assert_eq!(statuses[0].status, "delivered");
        }
    }
}
