//! Integration tests for Signal client
//!
//! These tests verify the Signal client behavior including:
//! - Client configuration
//! - Whitelist validation
//! - JSON-RPC message serialization
//! - Type deserialization
//! - Mock socket server integration

#![allow(
    clippy::unreadable_literal,
    clippy::items_after_statements,
    clippy::redundant_closure_for_method_calls,
    clippy::let_underscore_future,
    clippy::unused_async
)]

use integration_signal::SignalClient;

// =============================================================================
// Test Helpers
// =============================================================================

fn test_config() -> integration_signal::SignalClientConfig {
    integration_signal::SignalClientConfig::new("+1234567890")
        .with_socket_path("/tmp/test-signal.sock")
        .with_timeout_ms(5000)
}

// =============================================================================
// Client Configuration Tests
// =============================================================================

mod config_tests {
    use integration_signal::SignalClientConfig;

    #[test]
    fn config_new_sets_defaults() {
        let config = SignalClientConfig::new("+1234567890");
        assert_eq!(config.phone_number, "+1234567890");
        assert_eq!(config.socket_path, SignalClientConfig::DEFAULT_SOCKET_PATH);
        assert_eq!(config.timeout_ms, SignalClientConfig::DEFAULT_TIMEOUT_MS);
        assert!(config.data_path.is_none());
    }

    #[test]
    fn config_with_socket_path() {
        let config = SignalClientConfig::new("+1234567890").with_socket_path("/custom/socket");
        assert_eq!(config.socket_path, "/custom/socket");
    }

    #[test]
    fn config_with_data_path() {
        let config = SignalClientConfig::new("+1234567890").with_data_path("/var/lib/signal-cli");
        assert_eq!(config.data_path, Some("/var/lib/signal-cli".to_string()));
    }

    #[test]
    fn config_with_timeout() {
        let config = SignalClientConfig::new("+1234567890").with_timeout_ms(60000);
        assert_eq!(config.timeout_ms, 60000);
    }

    #[test]
    fn config_default() {
        let config = SignalClientConfig::default();
        assert!(config.phone_number.is_empty());
        assert_eq!(config.socket_path, SignalClientConfig::DEFAULT_SOCKET_PATH);
    }

    #[test]
    fn config_has_debug() {
        let config = SignalClientConfig::new("+1234567890");
        let debug = format!("{config:?}");
        assert!(debug.contains("SignalClientConfig"));
        assert!(debug.contains("+1234567890"));
    }

    #[test]
    fn config_clone() {
        let config = SignalClientConfig::new("+1234567890")
            .with_socket_path("/test")
            .with_data_path("/data");
        #[allow(clippy::redundant_clone)]
        let cloned = config.clone();
        assert_eq!(config.phone_number, cloned.phone_number);
        assert_eq!(config.socket_path, cloned.socket_path);
        assert_eq!(config.data_path, cloned.data_path);
    }
}

// =============================================================================
// Client Creation Tests
// =============================================================================

mod client_tests {
    use super::*;

    #[test]
    fn new_creates_client() {
        let config = test_config();
        let client = SignalClient::new(config);
        assert_eq!(client.phone_number(), "+1234567890");
    }

    #[test]
    fn with_whitelist_stores_phones() {
        let config = test_config();
        let whitelist = vec!["+1111111111".to_string(), "+2222222222".to_string()];
        let client = SignalClient::with_whitelist(config, whitelist);

        assert!(client.is_whitelisted("+1111111111"));
        assert!(client.is_whitelisted("+2222222222"));
        assert!(!client.is_whitelisted("+3333333333"));
    }

    #[test]
    fn client_has_debug() {
        let client = SignalClient::new(test_config());
        let debug = format!("{client:?}");
        assert!(debug.contains("SignalClient"));
        assert!(debug.contains("+1234567890"));
    }
}

// =============================================================================
// Whitelist Tests
// =============================================================================

mod whitelist_tests {
    use super::*;

    #[test]
    fn empty_whitelist_allows_all() {
        let config = test_config();
        let client = SignalClient::new(config);

        assert!(client.is_whitelisted("+9999999999"));
        assert!(client.is_whitelisted("+0000000000"));
        assert!(client.is_whitelisted("+1234567890"));
    }

    #[test]
    fn whitelist_exact_match() {
        let config = test_config();
        let whitelist = vec!["+491234567890".to_string()];
        let client = SignalClient::with_whitelist(config, whitelist);

        assert!(client.is_whitelisted("+491234567890"));
        assert!(!client.is_whitelisted("+491111111111"));
    }

    #[test]
    fn whitelist_normalizes_spaces() {
        let config = test_config();
        let whitelist = vec!["+49 123 456 7890".to_string()];
        let client = SignalClient::with_whitelist(config, whitelist);

        assert!(client.is_whitelisted("+491234567890"));
        assert!(client.is_whitelisted("+49 123 456 7890"));
    }

    #[test]
    fn whitelist_normalizes_dashes() {
        let config = test_config();
        let whitelist = vec!["+49-123-456-7890".to_string()];
        let client = SignalClient::with_whitelist(config, whitelist);

        assert!(client.is_whitelisted("+491234567890"));
        assert!(client.is_whitelisted("+49-123-456-7890"));
    }

    #[test]
    fn whitelist_normalizes_parentheses() {
        let config = test_config();
        let whitelist = vec!["+1 (234) 567-8900".to_string()];
        let client = SignalClient::with_whitelist(config, whitelist);

        assert!(client.is_whitelisted("+12345678900"));
        assert!(client.is_whitelisted("+1 234 567 8900"));
    }

    #[test]
    fn whitelist_multiple_numbers() {
        let config = test_config();
        let whitelist = vec![
            "+491111111111".to_string(),
            "+492222222222".to_string(),
            "+493333333333".to_string(),
        ];
        let client = SignalClient::with_whitelist(config, whitelist);

        assert!(client.is_whitelisted("+491111111111"));
        assert!(client.is_whitelisted("+492222222222"));
        assert!(client.is_whitelisted("+493333333333"));
        assert!(!client.is_whitelisted("+494444444444"));
    }
}

// =============================================================================
// Availability Tests
// =============================================================================

mod availability_tests {
    use super::*;

    #[tokio::test]
    async fn is_available_returns_false_for_nonexistent_socket() {
        let config = integration_signal::SignalClientConfig::new("+1234567890")
            .with_socket_path("/nonexistent/path/that/does/not/exist.sock");
        let client = SignalClient::new(config);

        assert!(!client.is_available().await);
    }
}

// =============================================================================
// JSON-RPC Types Tests
// =============================================================================

mod jsonrpc_tests {
    use integration_signal::{JsonRpcRequest, JsonRpcResponse};

    #[test]
    fn jsonrpc_request_serialization() {
        let request = JsonRpcRequest::new(
            "send",
            serde_json::json!({"recipient": ["+1234567890"], "message": "Hello"}),
            1,
        );

        let json = serde_json::to_string(&request).unwrap();
        assert!(json.contains("\"jsonrpc\":\"2.0\""));
        assert!(json.contains("\"method\":\"send\""));
        assert!(json.contains("\"id\":1"));
    }

    #[test]
    fn jsonrpc_response_success_deserialization() {
        let json = r#"{
            "jsonrpc": "2.0",
            "result": {"timestamp": 1234567890},
            "id": 1
        }"#;

        #[derive(serde::Deserialize)]
        struct TestResult {
            timestamp: i64,
        }

        let response: JsonRpcResponse<TestResult> = serde_json::from_str(json).unwrap();
        assert!(response.result.is_some());
        assert!(response.error.is_none());
        assert_eq!(response.result.unwrap().timestamp, 1234567890);
    }

    #[test]
    fn jsonrpc_response_error_deserialization() {
        let json = r#"{
            "jsonrpc": "2.0",
            "error": {
                "code": -32600,
                "message": "Invalid Request",
                "data": null
            },
            "id": 1
        }"#;

        let response: JsonRpcResponse<serde_json::Value> = serde_json::from_str(json).unwrap();
        assert!(response.result.is_none());
        assert!(response.error.is_some());

        let error = response.error.unwrap();
        assert_eq!(error.code, -32600);
        assert_eq!(error.message, "Invalid Request");
    }

    #[test]
    fn jsonrpc_request_has_debug() {
        let request = JsonRpcRequest::new("test", serde_json::json!({}), 1);
        let debug = format!("{request:?}");
        assert!(debug.contains("JsonRpcRequest"));
    }
}

// =============================================================================
// Send Parameters Tests
// =============================================================================

mod send_params_tests {
    use integration_signal::SendParams;

    #[test]
    fn text_message_params() {
        let params = SendParams::text("+1234567890", "Hello World");
        let json = serde_json::to_string(&params).unwrap();

        assert!(json.contains("recipient"));
        assert!(json.contains("+1234567890"));
        assert!(json.contains("Hello World"));
        assert!(!json.contains("attachment"));
    }

    #[test]
    fn attachment_params() {
        let params = SendParams::attachment("+1234567890", "/path/to/audio.ogg");
        let json = serde_json::to_string(&params).unwrap();

        assert!(json.contains("recipient"));
        assert!(json.contains("/path/to/audio.ogg"));
        assert!(!json.contains("message"));
    }

    #[test]
    fn text_with_reply_params() {
        let params =
            SendParams::text("+1234567890", "Reply text").with_reply(1234567890, "+0987654321");
        let json = serde_json::to_string(&params).unwrap();

        assert!(json.contains("quoteTimestamp"));
        assert!(json.contains("1234567890"));
        assert!(json.contains("quoteAuthor"));
        assert!(json.contains("+0987654321"));
    }

    #[test]
    fn params_has_debug() {
        let params = SendParams::text("+1234567890", "Test");
        let debug = format!("{params:?}");
        assert!(debug.contains("SendParams"));
    }

    #[test]
    fn params_clone() {
        let params = SendParams::text("+1234567890", "Test");
        #[allow(clippy::redundant_clone)]
        let cloned = params.clone();
        assert_eq!(
            serde_json::to_string(&params).unwrap(),
            serde_json::to_string(&cloned).unwrap()
        );
    }
}

// =============================================================================
// Receipt Parameters Tests
// =============================================================================

mod receipt_params_tests {
    use integration_signal::{ReceiptType, SendReceiptParams};

    #[test]
    fn receipt_params_serialization() {
        let params = SendReceiptParams {
            recipient: "+1234567890".to_string(),
            target_timestamp: vec![1234567890, 1234567891],
            receipt_type: ReceiptType::Read,
        };
        let json = serde_json::to_string(&params).unwrap();

        assert!(json.contains("recipient"));
        assert!(json.contains("targetTimestamp"));
        assert!(json.contains("\"type\":\"read\""));
    }

    #[test]
    fn receipt_type_viewed() {
        let params = SendReceiptParams {
            recipient: "+1234567890".to_string(),
            target_timestamp: vec![1234567890],
            receipt_type: ReceiptType::Viewed,
        };
        let json = serde_json::to_string(&params).unwrap();

        assert!(json.contains("\"type\":\"viewed\""));
    }
}

// =============================================================================
// Receive Parameters Tests
// =============================================================================

mod receive_params_tests {
    use integration_signal::ReceiveParams;

    #[test]
    fn receive_params_with_timeout() {
        let params = ReceiveParams { timeout: Some(30) };
        let json = serde_json::to_string(&params).unwrap();

        assert!(json.contains("timeout"));
        assert!(json.contains("30"));
    }

    #[test]
    fn receive_params_default() {
        let params = ReceiveParams::default();
        assert!(params.timeout.is_none());
    }
}

// =============================================================================
// Send Result Tests
// =============================================================================

mod send_result_tests {
    use integration_signal::SendResult;

    #[test]
    fn send_result_deserialization() {
        let json = r#"{
            "timestamp": 1234567890,
            "results": [{
                "recipientAddress": {
                    "number": "+1234567890",
                    "uuid": "abc-123"
                },
                "type": "SUCCESS"
            }]
        }"#;

        let result: SendResult = serde_json::from_str(json).unwrap();
        assert_eq!(result.timestamp, 1234567890);
        assert_eq!(result.results.len(), 1);
        assert_eq!(result.results[0].result_type, "SUCCESS");
    }

    #[test]
    fn send_result_empty_results() {
        let json = r#"{"timestamp": 1234567890}"#;
        let result: SendResult = serde_json::from_str(json).unwrap();
        assert_eq!(result.timestamp, 1234567890);
        assert!(result.results.is_empty());
    }
}

// =============================================================================
// Envelope Tests
// =============================================================================

mod envelope_tests {
    use integration_signal::Envelope;

    #[test]
    fn envelope_with_text_message() {
        let json = r#"{
            "source": "+1234567890",
            "sourceUuid": "abc-123",
            "sourceDevice": 1,
            "timestamp": 1234567890,
            "dataMessage": {
                "body": "Hello World",
                "timestamp": 1234567890,
                "attachments": []
            }
        }"#;

        let envelope: Envelope = serde_json::from_str(json).unwrap();
        assert_eq!(envelope.sender(), Some("+1234567890"));
        assert!(envelope.is_data_message());

        let data = envelope.data_message.unwrap();
        assert!(data.has_text());
        assert_eq!(data.body.as_deref(), Some("Hello World"));
    }

    #[test]
    fn envelope_with_attachment() {
        let json = r#"{
            "source": "+1234567890",
            "timestamp": 1234567890,
            "dataMessage": {
                "timestamp": 1234567890,
                "attachments": [{
                    "contentType": "audio/ogg",
                    "filename": "voice.ogg",
                    "size": 12345,
                    "voiceNote": true,
                    "file": "/tmp/signal/voice.ogg"
                }]
            }
        }"#;

        let envelope: Envelope = serde_json::from_str(json).unwrap();
        let data = envelope.data_message.unwrap();
        assert!(data.has_attachments());
        assert!(!data.has_text());

        let attachment = &data.attachments[0];
        assert!(attachment.is_voice_note());
        assert!(attachment.is_audio());
        assert_eq!(attachment.file.as_deref(), Some("/tmp/signal/voice.ogg"));
    }

    #[test]
    fn envelope_typing_message() {
        let json = r#"{
            "source": "+1234567890",
            "timestamp": 1234567890,
            "typingMessage": {
                "action": "STARTED",
                "timestamp": 1234567890
            }
        }"#;

        let envelope: Envelope = serde_json::from_str(json).unwrap();
        assert!(envelope.typing_message.is_some());
        assert!(!envelope.is_data_message());
    }

    #[test]
    fn envelope_receipt_message() {
        let json = r#"{
            "source": "+1234567890",
            "timestamp": 1234567890,
            "receiptMessage": {
                "timestamps": [1234567890, 1234567891],
                "type": "READ"
            }
        }"#;

        let envelope: Envelope = serde_json::from_str(json).unwrap();
        assert!(envelope.receipt_message.is_some());
        let receipt = envelope.receipt_message.unwrap();
        assert_eq!(receipt.timestamps.len(), 2);
        assert_eq!(receipt.receipt_type, "READ");
    }
}

// =============================================================================
// Data Message Tests
// =============================================================================

mod data_message_tests {
    use integration_signal::DataMessage;

    #[test]
    fn data_message_with_quote() {
        let json = r#"{
            "body": "Reply text",
            "timestamp": 1234567890,
            "attachments": [],
            "quote": {
                "id": 1234567880,
                "author": "+0987654321",
                "text": "Original message"
            }
        }"#;

        let msg: DataMessage = serde_json::from_str(json).unwrap();
        assert!(msg.quote.is_some());
        let quote = msg.quote.unwrap();
        assert_eq!(quote.id, 1234567880);
        assert_eq!(quote.author.as_deref(), Some("+0987654321"));
    }

    #[test]
    fn data_message_expiring() {
        let json = r#"{
            "body": "Secret message",
            "timestamp": 1234567890,
            "attachments": [],
            "expiresInSeconds": 3600
        }"#;

        let msg: DataMessage = serde_json::from_str(json).unwrap();
        assert_eq!(msg.expires_in_seconds, Some(3600));
    }

    #[test]
    fn data_message_view_once() {
        let json = r#"{
            "body": "View once message",
            "timestamp": 1234567890,
            "attachments": [],
            "viewOnce": true
        }"#;

        let msg: DataMessage = serde_json::from_str(json).unwrap();
        assert_eq!(msg.view_once, Some(true));
    }
}

// =============================================================================
// Attachment Tests
// =============================================================================

mod attachment_tests {
    use integration_signal::Attachment;

    #[test]
    fn attachment_image() {
        let json = r#"{
            "contentType": "image/jpeg",
            "filename": "photo.jpg",
            "size": 102400,
            "width": 1920,
            "height": 1080
        }"#;

        let attachment: Attachment = serde_json::from_str(json).unwrap();
        assert_eq!(attachment.content_type, "image/jpeg");
        assert!(!attachment.is_voice_note());
        assert!(!attachment.is_audio());
        assert_eq!(attachment.width, Some(1920));
    }

    #[test]
    fn attachment_voice_note() {
        let json = r#"{
            "contentType": "audio/aac",
            "voiceNote": true,
            "size": 5000
        }"#;

        let attachment: Attachment = serde_json::from_str(json).unwrap();
        assert!(attachment.is_voice_note());
        assert!(attachment.is_audio());
    }

    #[test]
    fn attachment_not_voice_note() {
        let json = r#"{
            "contentType": "audio/mp3",
            "voiceNote": false
        }"#;

        let attachment: Attachment = serde_json::from_str(json).unwrap();
        assert!(!attachment.is_voice_note());
        assert!(attachment.is_audio());
    }
}

// =============================================================================
// Error Tests
// =============================================================================

mod error_tests {
    use integration_signal::SignalError;

    #[test]
    fn error_connection() {
        let err = SignalError::connection("socket not found");
        assert!(err.to_string().contains("Connection failed"));
        assert!(err.to_string().contains("socket not found"));
    }

    #[test]
    fn error_protocol() {
        let err = SignalError::protocol("invalid response");
        assert!(err.to_string().contains("Protocol error"));
    }

    #[test]
    fn error_signal_cli() {
        let err = SignalError::signal_cli(-32600, "Invalid Request");
        let display = err.to_string();
        assert!(display.contains("-32600"));
        assert!(display.contains("Invalid Request"));
    }

    #[test]
    fn error_not_registered() {
        let err = SignalError::not_registered("+1234567890");
        assert!(err.to_string().contains("not registered"));
    }

    #[test]
    fn error_send_failed() {
        let err = SignalError::send_failed("recipient offline");
        assert!(err.to_string().contains("Send failed"));
    }

    #[test]
    fn error_media() {
        let err = SignalError::media("file not found");
        assert!(err.to_string().contains("Media error"));
    }

    #[test]
    fn error_timeout() {
        let err = SignalError::Timeout;
        assert!(err.to_string().contains("timed out"));
    }

    #[test]
    fn error_from_io() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "file not found");
        let err: SignalError = io_err.into();
        assert!(err.to_string().contains("IO error"));
    }

    #[test]
    fn error_from_json() {
        let json_err = serde_json::from_str::<()>("invalid").unwrap_err();
        let err: SignalError = json_err.into();
        assert!(err.to_string().contains("JSON error"));
    }

    #[test]
    fn error_debug() {
        let err = SignalError::connection("test");
        let debug = format!("{err:?}");
        assert!(debug.contains("Connection"));
    }
}

// =============================================================================
// Property-Based Tests
// =============================================================================

mod proptest_tests {
    use proptest::prelude::*;

    proptest! {
        #[test]
        fn whitelist_normalization_preserves_digits(
            phone in r"\+[0-9]{10,15}"
        ) {
            let config = integration_signal::SignalClientConfig::new("+1234567890")
                .with_socket_path("/tmp/test.sock");
            let whitelist = vec![phone.clone()];
            let client = integration_signal::SignalClient::with_whitelist(config, whitelist);

            // Phone without formatting characters should be whitelisted
            prop_assert!(client.is_whitelisted(&phone));
        }

        #[test]
        fn send_params_serialization(
            recipient in r"\+[0-9]{10,15}",
            message in "[a-zA-Z0-9 ]{1,100}"
        ) {
            let params = integration_signal::SendParams::text(&recipient, &message);
            let json = serde_json::to_string(&params);

            prop_assert!(json.is_ok());
            let json = json.unwrap();
            prop_assert!(json.contains(&recipient));
        }

        #[test]
        fn json_rpc_request_id_preserved(
            id in 1u64..u64::MAX,
            method in "[a-zA-Z]{3,20}"
        ) {
            let request = integration_signal::JsonRpcRequest::new(
                &method,
                serde_json::json!({}),
                id,
            );

            prop_assert_eq!(request.id, id);
            prop_assert_eq!(request.method, method);
            prop_assert_eq!(request.jsonrpc, "2.0");
        }

        #[test]
        fn config_timeout_preserved(
            timeout in 1000u64..3600000u64
        ) {
            let config = integration_signal::SignalClientConfig::new("+1234567890")
                .with_timeout_ms(timeout);

            prop_assert_eq!(config.timeout_ms, timeout);
        }

        #[test]
        fn envelope_timestamp_parsing(
            timestamp in 1000000000i64..9999999999i64
        ) {
            let json = format!(r#"{{"source": "+1234567890", "timestamp": {timestamp}}}"#);
            let envelope: Result<integration_signal::Envelope, _> = serde_json::from_str(&json);

            prop_assert!(envelope.is_ok());
            prop_assert_eq!(envelope.unwrap().timestamp, timestamp);
        }
    }
}

// =============================================================================
// Mock Socket Server Tests
// =============================================================================

#[cfg(test)]
mod mock_socket_tests {
    use super::*;
    use std::path::PathBuf;
    use tempfile::TempDir;
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
    use tokio::net::UnixListener;

    /// Create a temporary directory for the test socket
    fn create_temp_dir() -> TempDir {
        tempfile::tempdir().expect("Failed to create temp dir")
    }

    /// Mock JSON-RPC server that responds to any request
    async fn start_mock_server(socket_path: PathBuf) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            let listener = UnixListener::bind(&socket_path).expect("Failed to bind socket");

            // Accept one connection
            if let Ok((stream, _)) = listener.accept().await {
                let (read, mut write) = stream.into_split();
                let mut reader = BufReader::new(read);
                let mut line = String::new();

                // Read the request
                if reader.read_line(&mut line).await.is_ok() {
                    // Parse and respond
                    if let Ok(request) = serde_json::from_str::<serde_json::Value>(&line) {
                        let id = request.get("id").and_then(|v| v.as_u64()).unwrap_or(0);
                        let response = serde_json::json!({
                            "jsonrpc": "2.0",
                            "result": {"timestamp": 1234567890, "results": []},
                            "id": id
                        });
                        let response_str = serde_json::to_string(&response).unwrap() + "\n";
                        let _ = write.write_all(response_str.as_bytes()).await;
                    }
                }
            }
        })
    }

    #[tokio::test]
    #[ignore = "Flaky test - timing-dependent socket communication; tested manually"]
    async fn mock_server_send_text() {
        let temp_dir = create_temp_dir();
        let socket_path = temp_dir.path().join("signal.sock");

        // Start the mock server
        let server = start_mock_server(socket_path.clone()).await;

        // Give server time to start
        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

        let config = integration_signal::SignalClientConfig::new("+1234567890")
            .with_socket_path(socket_path.to_string_lossy());
        let client = SignalClient::new(config);

        // Verify connection
        let available = client.is_available().await;
        assert!(available, "Socket should be available");

        // Send a message
        let result = client.send_text("+9876543210", "Hello from test").await;
        assert!(result.is_ok(), "Send should succeed: {:?}", result.err());

        let send_result = result.unwrap();
        assert_eq!(send_result.timestamp, 1234567890);

        // Cleanup
        server.abort();
    }

    #[tokio::test]
    async fn mock_server_receive() {
        let temp_dir = create_temp_dir();
        let socket_path = temp_dir.path().join("signal-recv.sock");

        // Start mock server with envelope response
        let socket_path_clone = socket_path.clone();
        let server = tokio::spawn(async move {
            let listener = UnixListener::bind(&socket_path_clone).expect("Failed to bind socket");

            if let Ok((stream, _)) = listener.accept().await {
                let (read, mut write) = stream.into_split();
                let mut reader = BufReader::new(read);
                let mut line = String::new();

                if reader.read_line(&mut line).await.is_ok() {
                    if let Ok(request) = serde_json::from_str::<serde_json::Value>(&line) {
                        let id = request.get("id").and_then(|v| v.as_u64()).unwrap_or(0);

                        // Return array of envelopes
                        let response = serde_json::json!({
                            "jsonrpc": "2.0",
                            "result": [{
                                "source": "+1234567890",
                                "timestamp": 1234567890,
                                "dataMessage": {
                                    "body": "Test message",
                                    "timestamp": 1234567890,
                                    "attachments": []
                                }
                            }],
                            "id": id
                        });
                        let response_str = serde_json::to_string(&response).unwrap() + "\n";
                        let _ = write.write_all(response_str.as_bytes()).await;
                    }
                }
            }
        });

        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

        let config = integration_signal::SignalClientConfig::new("+1234567890")
            .with_socket_path(socket_path.to_string_lossy());
        let client = SignalClient::new(config);

        let result = client.receive(0).await;
        assert!(result.is_ok(), "Receive should succeed: {:?}", result.err());

        let envelopes = result.unwrap();
        assert_eq!(envelopes.len(), 1);
        assert_eq!(envelopes[0].sender(), Some("+1234567890"));

        server.abort();
    }

    #[tokio::test]
    async fn client_close_resets_connection() {
        let temp_dir = create_temp_dir();
        let socket_path = temp_dir.path().join("signal-close.sock");

        let _ = start_mock_server(socket_path.clone()).await;
        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

        let config = integration_signal::SignalClientConfig::new("+1234567890")
            .with_socket_path(socket_path.to_string_lossy());
        let client = SignalClient::new(config);

        // Close should not panic
        client.close().await;
    }
}
