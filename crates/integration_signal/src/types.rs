//! Signal-specific types and JSON-RPC protocol structures

use serde::{Deserialize, Serialize};

/// Configuration for the Signal client
#[derive(Debug, Clone)]
pub struct SignalClientConfig {
    /// Phone number registered with Signal (E.164 format, e.g., "+1234567890")
    pub phone_number: String,
    /// Path to the signal-cli JSON-RPC socket
    pub socket_path: String,
    /// Path to signal-cli data directory (optional)
    pub data_path: Option<String>,
    /// Connection timeout in milliseconds
    pub timeout_ms: u64,
}

impl SignalClientConfig {
    /// Default socket path for signal-cli JSON-RPC daemon
    pub const DEFAULT_SOCKET_PATH: &'static str = "/var/run/signal-cli/socket";

    /// Default connection timeout (30 seconds)
    pub const DEFAULT_TIMEOUT_MS: u64 = 30_000;

    /// Create a new config with required phone number
    #[must_use]
    pub fn new(phone_number: impl Into<String>) -> Self {
        Self {
            phone_number: phone_number.into(),
            socket_path: Self::DEFAULT_SOCKET_PATH.to_string(),
            data_path: None,
            timeout_ms: Self::DEFAULT_TIMEOUT_MS,
        }
    }

    /// Set the socket path
    #[must_use]
    pub fn with_socket_path(mut self, path: impl Into<String>) -> Self {
        self.socket_path = path.into();
        self
    }

    /// Set the data directory path
    #[must_use]
    pub fn with_data_path(mut self, path: impl Into<String>) -> Self {
        self.data_path = Some(path.into());
        self
    }

    /// Set the connection timeout
    #[must_use]
    pub const fn with_timeout_ms(mut self, timeout_ms: u64) -> Self {
        self.timeout_ms = timeout_ms;
        self
    }
}

impl Default for SignalClientConfig {
    fn default() -> Self {
        Self {
            phone_number: String::new(),
            socket_path: Self::DEFAULT_SOCKET_PATH.to_string(),
            data_path: None,
            timeout_ms: Self::DEFAULT_TIMEOUT_MS,
        }
    }
}

// ============================================================================
// JSON-RPC Protocol Types
// ============================================================================

/// JSON-RPC request structure
#[derive(Debug, Clone, Serialize)]
pub struct JsonRpcRequest<P> {
    /// JSON-RPC version (always "2.0")
    pub jsonrpc: &'static str,
    /// Request method
    pub method: String,
    /// Request parameters
    pub params: P,
    /// Request ID
    pub id: u64,
}

impl<P> JsonRpcRequest<P> {
    /// Create a new JSON-RPC request
    #[must_use]
    pub fn new(method: impl Into<String>, params: P, id: u64) -> Self {
        Self {
            jsonrpc: "2.0",
            method: method.into(),
            params,
            id,
        }
    }
}

/// JSON-RPC response structure
#[derive(Debug, Clone, Deserialize)]
pub struct JsonRpcResponse<R> {
    /// JSON-RPC version
    #[allow(dead_code)]
    pub jsonrpc: String,
    /// Result (if successful)
    pub result: Option<R>,
    /// Error (if failed)
    pub error: Option<JsonRpcError>,
    /// Response ID
    #[allow(dead_code)]
    pub id: u64,
}

/// JSON-RPC error structure
#[derive(Debug, Clone, Deserialize)]
pub struct JsonRpcError {
    /// Error code
    pub code: i32,
    /// Error message
    pub message: String,
    /// Additional data
    pub data: Option<serde_json::Value>,
}

// ============================================================================
// signal-cli Method Parameters
// ============================================================================

/// Parameters for the `send` method
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SendParams {
    /// Recipient phone number (E.164 format)
    pub recipient: Vec<String>,
    /// Text message to send
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    /// Attachments to send
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub attachment: Vec<String>,
    /// Quote/reply to a previous message
    #[serde(skip_serializing_if = "Option::is_none")]
    pub quote_timestamp: Option<i64>,
    /// Quote author
    #[serde(skip_serializing_if = "Option::is_none")]
    pub quote_author: Option<String>,
}

impl SendParams {
    /// Create params for a text message
    #[must_use]
    pub fn text(recipient: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            recipient: vec![recipient.into()],
            message: Some(message.into()),
            attachment: Vec::new(),
            quote_timestamp: None,
            quote_author: None,
        }
    }

    /// Create params for an attachment
    #[must_use]
    pub fn attachment(recipient: impl Into<String>, path: impl Into<String>) -> Self {
        Self {
            recipient: vec![recipient.into()],
            message: None,
            attachment: vec![path.into()],
            quote_timestamp: None,
            quote_author: None,
        }
    }

    /// Add a reply reference
    #[must_use]
    pub fn with_reply(mut self, timestamp: i64, author: impl Into<String>) -> Self {
        self.quote_timestamp = Some(timestamp);
        self.quote_author = Some(author.into());
        self
    }
}

/// Parameters for the `sendTyping` method
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
#[allow(dead_code)] // Part of public API for future use
pub struct SendTypingParams {
    /// Recipient phone number
    pub recipient: Vec<String>,
    /// Whether typing started (true) or stopped (false)
    pub stop: bool,
}

/// Parameters for the `sendReceipt` method
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SendReceiptParams {
    /// Recipient (sender of the original message)
    pub recipient: String,
    /// Timestamps of messages to mark as read
    pub target_timestamp: Vec<i64>,
    /// Receipt type
    #[serde(rename = "type")]
    pub receipt_type: ReceiptType,
}

/// Types of read receipts
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ReceiptType {
    /// Message has been read
    Read,
    /// Message has been viewed (for media)
    Viewed,
}

/// Parameters for the `receive` method
#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ReceiveParams {
    /// Timeout in seconds (0 = no wait)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeout: Option<u64>,
}

// ============================================================================
// signal-cli Response Types
// ============================================================================

/// Response from the `send` method
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SendResult {
    /// Timestamp of the sent message
    pub timestamp: i64,
    /// Results per recipient
    #[serde(default)]
    pub results: Vec<SendResultItem>,
}

/// Per-recipient send result
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SendResultItem {
    /// Recipient number
    pub recipient_address: RecipientAddress,
    /// Type of result
    #[serde(rename = "type")]
    pub result_type: String,
}

/// Recipient address structure
#[derive(Debug, Clone, Deserialize)]
pub struct RecipientAddress {
    /// Phone number
    pub number: Option<String>,
    /// UUID
    pub uuid: Option<String>,
}

/// Incoming message envelope from `receive`
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Envelope {
    /// Sender's phone number
    pub source: Option<String>,
    /// Sender's UUID
    pub source_uuid: Option<String>,
    /// Sender's device ID
    pub source_device: Option<i32>,
    /// Message timestamp
    pub timestamp: i64,
    /// Data message content
    pub data_message: Option<DataMessage>,
    /// Typing indicator
    pub typing_message: Option<TypingMessage>,
    /// Receipt message
    pub receipt_message: Option<ReceiptMessage>,
    /// Sync message (from linked devices)
    pub sync_message: Option<SyncMessage>,
}

impl Envelope {
    /// Get the sender's phone number
    #[must_use]
    pub fn sender(&self) -> Option<&str> {
        self.source.as_deref()
    }

    /// Check if this is a data message
    #[must_use]
    pub const fn is_data_message(&self) -> bool {
        self.data_message.is_some()
    }
}

/// Data message content
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DataMessage {
    /// Text body
    pub body: Option<String>,
    /// Message timestamp
    pub timestamp: i64,
    /// Attachments
    #[serde(default)]
    pub attachments: Vec<Attachment>,
    /// Quote/reply reference
    pub quote: Option<Quote>,
    /// Expiration timer (seconds)
    pub expires_in_seconds: Option<i64>,
    /// View-once flag
    pub view_once: Option<bool>,
}

impl DataMessage {
    /// Check if this message has text content
    #[must_use]
    pub fn has_text(&self) -> bool {
        self.body.as_ref().is_some_and(|b| !b.is_empty())
    }

    /// Check if this message has attachments
    #[must_use]
    pub fn has_attachments(&self) -> bool {
        !self.attachments.is_empty()
    }
}

/// Attachment in a data message
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Attachment {
    /// MIME type
    pub content_type: String,
    /// Original filename
    pub filename: Option<String>,
    /// Attachment ID for downloading
    pub id: Option<String>,
    /// Size in bytes
    pub size: Option<u64>,
    /// Width (for images)
    pub width: Option<u32>,
    /// Height (for images)
    pub height: Option<u32>,
    /// Voice note flag
    pub voice_note: Option<bool>,
    /// Local file path (if downloaded)
    pub file: Option<String>,
}

impl Attachment {
    /// Check if this is a voice note
    #[must_use]
    pub fn is_voice_note(&self) -> bool {
        self.voice_note.unwrap_or(false)
    }

    /// Check if this is an audio attachment
    #[must_use]
    pub fn is_audio(&self) -> bool {
        self.content_type.starts_with("audio/")
    }
}

/// Quote (reply) reference
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Quote {
    /// ID of quoted message
    pub id: i64,
    /// Author of quoted message
    pub author: Option<String>,
    /// Text of quoted message
    pub text: Option<String>,
}

/// Typing indicator message
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TypingMessage {
    /// Typing action
    pub action: String,
    /// Timestamp
    pub timestamp: i64,
}

/// Receipt message
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReceiptMessage {
    /// Timestamps of receipted messages
    pub timestamps: Vec<i64>,
    /// Type of receipt
    #[serde(rename = "type")]
    pub receipt_type: String,
}

/// Sync message (from linked devices)
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncMessage {
    /// Sent message sync
    pub sent_message: Option<SentMessage>,
    /// Read messages sync
    pub read_messages: Option<Vec<ReadMessage>>,
}

/// Synced sent message
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SentMessage {
    /// Destination number
    pub destination: Option<String>,
    /// Destination UUID
    pub destination_uuid: Option<String>,
    /// Message content
    pub message: Option<DataMessage>,
    /// Timestamp
    pub timestamp: i64,
}

/// Synced read message
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReadMessage {
    /// Sender of the read message
    pub sender: Option<String>,
    /// Timestamp of the read message
    pub timestamp: i64,
}

#[cfg(test)]
mod tests {
    use super::*;

    mod config_tests {
        use super::*;

        #[test]
        fn new_sets_phone_number() {
            let config = SignalClientConfig::new("+1234567890");
            assert_eq!(config.phone_number, "+1234567890");
            assert_eq!(config.socket_path, SignalClientConfig::DEFAULT_SOCKET_PATH);
        }

        #[test]
        fn with_socket_path_sets_path() {
            let config = SignalClientConfig::new("+1234567890")
                .with_socket_path("/custom/socket");
            assert_eq!(config.socket_path, "/custom/socket");
        }

        #[test]
        fn with_data_path_sets_path() {
            let config = SignalClientConfig::new("+1234567890")
                .with_data_path("/custom/data");
            assert_eq!(config.data_path, Some("/custom/data".to_string()));
        }

        #[test]
        fn with_timeout_sets_timeout() {
            let config = SignalClientConfig::new("+1234567890")
                .with_timeout_ms(5000);
            assert_eq!(config.timeout_ms, 5000);
        }

        #[test]
        fn default_has_empty_phone() {
            let config = SignalClientConfig::default();
            assert!(config.phone_number.is_empty());
        }
    }

    mod json_rpc_tests {
        use super::*;

        #[test]
        fn request_serializes_correctly() {
            let req = JsonRpcRequest::new("send", SendParams::text("+1234567890", "Hello"), 1);
            assert_eq!(req.jsonrpc, "2.0");
            assert_eq!(req.method, "send");
            assert_eq!(req.id, 1);
        }

        #[test]
        fn response_deserializes_success() {
            let json = r#"{"jsonrpc":"2.0","result":{"timestamp":123},"id":1}"#;
            let resp: JsonRpcResponse<SendResult> = serde_json::from_str(json).unwrap();
            assert!(resp.result.is_some());
            assert!(resp.error.is_none());
        }

        #[test]
        fn response_deserializes_error() {
            let json = r#"{"jsonrpc":"2.0","error":{"code":-1,"message":"failed"},"id":1}"#;
            let resp: JsonRpcResponse<SendResult> = serde_json::from_str(json).unwrap();
            assert!(resp.result.is_none());
            assert!(resp.error.is_some());
            let err = resp.error.unwrap();
            assert_eq!(err.code, -1);
            assert_eq!(err.message, "failed");
        }
    }

    mod send_params_tests {
        use super::*;

        #[test]
        fn text_creates_correct_params() {
            let params = SendParams::text("+1234567890", "Hello");
            assert_eq!(params.recipient, vec!["+1234567890"]);
            assert_eq!(params.message, Some("Hello".to_string()));
            assert!(params.attachment.is_empty());
        }

        #[test]
        fn attachment_creates_correct_params() {
            let params = SendParams::attachment("+1234567890", "/path/to/file");
            assert_eq!(params.recipient, vec!["+1234567890"]);
            assert!(params.message.is_none());
            assert_eq!(params.attachment, vec!["/path/to/file"]);
        }

        #[test]
        fn with_reply_sets_quote_fields() {
            let params = SendParams::text("+1234567890", "Reply")
                .with_reply(12345, "+0987654321");
            assert_eq!(params.quote_timestamp, Some(12345));
            assert_eq!(params.quote_author, Some("+0987654321".to_string()));
        }
    }

    mod envelope_tests {
        use super::*;

        #[test]
        fn sender_returns_source() {
            let envelope = Envelope {
                source: Some("+1234567890".to_string()),
                source_uuid: None,
                source_device: None,
                timestamp: 0,
                data_message: None,
                typing_message: None,
                receipt_message: None,
                sync_message: None,
            };
            assert_eq!(envelope.sender(), Some("+1234567890"));
        }

        #[test]
        fn is_data_message_true_when_present() {
            let envelope = Envelope {
                source: None,
                source_uuid: None,
                source_device: None,
                timestamp: 0,
                data_message: Some(DataMessage {
                    body: Some("Hello".to_string()),
                    timestamp: 0,
                    attachments: vec![],
                    quote: None,
                    expires_in_seconds: None,
                    view_once: None,
                }),
                typing_message: None,
                receipt_message: None,
                sync_message: None,
            };
            assert!(envelope.is_data_message());
        }
    }

    mod data_message_tests {
        use super::*;

        #[test]
        fn has_text_true_when_body_present() {
            let msg = DataMessage {
                body: Some("Hello".to_string()),
                timestamp: 0,
                attachments: vec![],
                quote: None,
                expires_in_seconds: None,
                view_once: None,
            };
            assert!(msg.has_text());
        }

        #[test]
        fn has_text_false_when_empty() {
            let msg = DataMessage {
                body: Some(String::new()),
                timestamp: 0,
                attachments: vec![],
                quote: None,
                expires_in_seconds: None,
                view_once: None,
            };
            assert!(!msg.has_text());
        }

        #[test]
        fn has_attachments_true_when_present() {
            let msg = DataMessage {
                body: None,
                timestamp: 0,
                attachments: vec![Attachment {
                    content_type: "audio/ogg".to_string(),
                    filename: None,
                    id: None,
                    size: None,
                    width: None,
                    height: None,
                    voice_note: Some(true),
                    file: None,
                }],
                quote: None,
                expires_in_seconds: None,
                view_once: None,
            };
            assert!(msg.has_attachments());
        }
    }

    mod attachment_tests {
        use super::*;

        #[test]
        fn is_voice_note_true() {
            let att = Attachment {
                content_type: "audio/ogg".to_string(),
                filename: None,
                id: None,
                size: None,
                width: None,
                height: None,
                voice_note: Some(true),
                file: None,
            };
            assert!(att.is_voice_note());
        }

        #[test]
        fn is_voice_note_false_when_none() {
            let att = Attachment {
                content_type: "audio/ogg".to_string(),
                filename: None,
                id: None,
                size: None,
                width: None,
                height: None,
                voice_note: None,
                file: None,
            };
            assert!(!att.is_voice_note());
        }

        #[test]
        fn is_audio_true_for_audio_types() {
            let att = Attachment {
                content_type: "audio/ogg".to_string(),
                filename: None,
                id: None,
                size: None,
                width: None,
                height: None,
                voice_note: None,
                file: None,
            };
            assert!(att.is_audio());
        }

        #[test]
        fn is_audio_false_for_image() {
            let att = Attachment {
                content_type: "image/jpeg".to_string(),
                filename: None,
                id: None,
                size: None,
                width: None,
                height: None,
                voice_note: None,
                file: None,
            };
            assert!(!att.is_audio());
        }
    }
}
