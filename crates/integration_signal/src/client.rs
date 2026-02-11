//! Signal client for signal-cli JSON-RPC daemon
//!
//! This client communicates with signal-cli running in JSON-RPC daemon mode
//! over a Unix domain socket.

use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;
use tokio::sync::Mutex;
use tracing::{debug, error, instrument, warn};

use crate::error::SignalError;
use crate::types::{
    Attachment, Envelope, JsonRpcRequest, JsonRpcResponse, ReceiptType, ReceiveParams, SendParams,
    SendReceiptParams, SendResult, SignalClientConfig,
};

/// Client for communicating with signal-cli JSON-RPC daemon
pub struct SignalClient {
    /// Configuration
    config: SignalClientConfig,
    /// Connection to the daemon (lazily initialized)
    connection: Mutex<Option<Connection>>,
    /// Request ID counter
    request_id: AtomicU64,
    /// Phone numbers allowed to interact (empty = allow all)
    whitelisted_phones: Arc<Vec<String>>,
}

/// Active connection to the daemon
struct Connection {
    reader: BufReader<tokio::net::unix::OwnedReadHalf>,
    writer: tokio::net::unix::OwnedWriteHalf,
}

impl SignalClient {
    /// Create a new Signal client
    #[must_use]
    pub fn new(config: SignalClientConfig) -> Self {
        Self {
            config,
            connection: Mutex::new(None),
            request_id: AtomicU64::new(1),
            whitelisted_phones: Arc::new(Vec::new()),
        }
    }

    /// Create a client with whitelisted phone numbers
    #[must_use]
    pub fn with_whitelist(config: SignalClientConfig, whitelist: Vec<String>) -> Self {
        Self {
            config,
            connection: Mutex::new(None),
            request_id: AtomicU64::new(1),
            whitelisted_phones: Arc::new(whitelist),
        }
    }

    /// Get the configured phone number
    #[must_use]
    pub fn phone_number(&self) -> &str {
        &self.config.phone_number
    }

    /// Check if a phone number is whitelisted
    #[must_use]
    pub fn is_whitelisted(&self, phone: &str) -> bool {
        if self.whitelisted_phones.is_empty() {
            return true; // Empty whitelist = allow all
        }
        // Normalize phone numbers for comparison
        let normalized = phone.trim().replace([' ', '-', '(', ')'], "");
        self.whitelisted_phones.iter().any(|w| {
            let w_normalized = w.trim().replace([' ', '-', '(', ')'], "");
            w_normalized == normalized
        })
    }

    /// Check if signal-cli daemon is available
    #[instrument(skip(self), fields(socket = %self.config.socket_path))]
    pub async fn is_available(&self) -> bool {
        let path = Path::new(&self.config.socket_path);
        if !path.exists() {
            debug!("Signal socket does not exist");
            return false;
        }

        // Try to connect
        match UnixStream::connect(path).await {
            Ok(_) => {
                debug!("Signal daemon is available");
                true
            },
            Err(e) => {
                warn!(error = %e, "Failed to connect to signal daemon");
                false
            },
        }
    }

    /// Send a text message
    #[instrument(skip(self, message), fields(recipient = %recipient))]
    pub async fn send_text(
        &self,
        recipient: &str,
        message: &str,
    ) -> Result<SendResult, SignalError> {
        let params = SendParams::text(recipient, message);
        self.call_method("send", params).await
    }

    /// Send a text message as a reply
    #[instrument(skip(self, message), fields(recipient = %recipient))]
    pub async fn send_text_reply(
        &self,
        recipient: &str,
        message: &str,
        reply_timestamp: i64,
        reply_author: &str,
    ) -> Result<SendResult, SignalError> {
        let params = SendParams::text(recipient, message).with_reply(reply_timestamp, reply_author);
        self.call_method("send", params).await
    }

    /// Send an audio file as an attachment
    #[instrument(skip(self), fields(recipient = %recipient, path = %audio_path))]
    pub async fn send_audio(
        &self,
        recipient: &str,
        audio_path: &str,
    ) -> Result<SendResult, SignalError> {
        let params = SendParams::attachment(recipient, audio_path);
        self.call_method("send", params).await
    }

    /// Mark messages as read
    #[instrument(skip(self, timestamps), fields(recipient = %recipient, count = timestamps.len()))]
    pub async fn send_read_receipt(
        &self,
        recipient: &str,
        timestamps: Vec<i64>,
    ) -> Result<(), SignalError> {
        let params = SendReceiptParams {
            recipient: recipient.to_string(),
            target_timestamp: timestamps,
            receipt_type: ReceiptType::Read,
        };
        let _: serde_json::Value = self.call_method("sendReceipt", params).await?;
        Ok(())
    }

    /// Receive pending messages (non-blocking if timeout is 0)
    #[instrument(skip(self), fields(timeout = timeout_seconds))]
    pub async fn receive(&self, timeout_seconds: u64) -> Result<Vec<Envelope>, SignalError> {
        let params = ReceiveParams {
            timeout: Some(timeout_seconds),
        };
        self.call_method("receive", params).await
    }

    /// Get an attachment file path for an attachment
    ///
    /// Returns the local file path where signal-cli stores the attachment.
    #[instrument(skip(self), fields(attachment_id = ?attachment.id))]
    pub fn get_attachment_path(&self, attachment: &Attachment) -> Option<String> {
        attachment.file.clone()
    }

    /// Internal method to make JSON-RPC calls
    async fn call_method<P, R>(&self, method: &str, params: P) -> Result<R, SignalError>
    where
        P: serde::Serialize + Send,
        R: serde::de::DeserializeOwned,
    {
        let id = self.request_id.fetch_add(1, Ordering::SeqCst);

        // Build params with account field
        let mut wrapped_params = serde_json::to_value(&params)?;
        if let serde_json::Value::Object(ref mut map) = wrapped_params {
            map.insert(
                "account".to_string(),
                serde_json::Value::String(self.config.phone_number.clone()),
            );
        }

        let request = JsonRpcRequest::new(method, wrapped_params, id);
        let request_json = serde_json::to_string(&request)?;

        debug!(method, id, "Sending JSON-RPC request");

        let response_json = self.send_request(&request_json).await?;
        let response: JsonRpcResponse<R> = serde_json::from_str(&response_json)?;

        if let Some(error) = response.error {
            error!(code = error.code, message = %error.message, "JSON-RPC error");
            return Err(SignalError::signal_cli(error.code, error.message));
        }

        response
            .result
            .ok_or_else(|| SignalError::protocol("Response contained neither result nor error"))
    }

    /// Send a raw JSON-RPC request and receive response
    async fn send_request(&self, request: &str) -> Result<String, SignalError> {
        let mut conn_guard = self.connection.lock().await;

        // Ensure we have a connection
        if conn_guard.is_none() {
            let stream = UnixStream::connect(&self.config.socket_path).await?;
            let (read_half, write_half) = stream.into_split();
            *conn_guard = Some(Connection {
                reader: BufReader::new(read_half),
                writer: write_half,
            });
        }

        // unwrap is safe: we just created the connection above if it was None
        let Some(conn) = conn_guard.as_mut() else {
            return Err(SignalError::protocol("Connection state error"));
        };

        // Send request (with newline delimiter)
        conn.writer.write_all(request.as_bytes()).await?;
        conn.writer.write_all(b"\n").await?;
        conn.writer.flush().await?;

        // Read response (one line)
        let mut response = String::new();
        conn.reader.read_line(&mut response).await?;

        if response.is_empty() {
            // Connection closed, clear it for next attempt
            *conn_guard = None;
            return Err(SignalError::connection("Connection closed by daemon"));
        }

        Ok(response)
    }

    /// Close the connection (it will be re-established on next request)
    pub async fn close(&self) {
        let mut conn_guard = self.connection.lock().await;
        *conn_guard = None;
    }
}

impl std::fmt::Debug for SignalClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SignalClient")
            .field("phone_number", &self.config.phone_number)
            .field("socket_path", &self.config.socket_path)
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_config() -> SignalClientConfig {
        SignalClientConfig::new("+1234567890").with_socket_path("/tmp/test-signal.sock")
    }

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
    fn empty_whitelist_allows_all() {
        let config = test_config();
        let client = SignalClient::new(config);
        assert!(client.is_whitelisted("+9999999999"));
        assert!(client.is_whitelisted("+0000000000"));
    }

    #[test]
    fn whitelist_normalizes_numbers() {
        let config = test_config();
        let whitelist = vec!["+1 (234) 567-8900".to_string()];
        let client = SignalClient::with_whitelist(config, whitelist);
        assert!(client.is_whitelisted("+12345678900"));
        assert!(client.is_whitelisted("+1 234 567 8900"));
        assert!(client.is_whitelisted("+1-234-567-8900"));
    }

    #[test]
    fn debug_format() {
        let client = SignalClient::new(test_config());
        let debug = format!("{client:?}");
        assert!(debug.contains("SignalClient"));
        assert!(debug.contains("+1234567890"));
    }

    #[tokio::test]
    async fn is_available_returns_false_for_nonexistent_socket() {
        let config = SignalClientConfig::new("+1234567890")
            .with_socket_path("/nonexistent/socket/path/that/does/not/exist.sock");
        let client = SignalClient::new(config);
        assert!(!client.is_available().await);
    }
}
