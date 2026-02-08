//! Signal messenger integration via signal-cli JSON-RPC
//!
//! This crate provides integration with Signal messenger using signal-cli
//! running in JSON-RPC daemon mode over a Unix domain socket.
//!
//! # Architecture
//!
//! ```text
//! ┌─────────────────┐     Unix Socket      ┌─────────────────┐
//! │  SignalClient   │ ◄──────────────────► │   signal-cli    │
//! │  (This crate)   │      JSON-RPC        │    daemon       │
//! └─────────────────┘                      └─────────────────┘
//! ```
//!
//! # Prerequisites
//!
//! - signal-cli must be installed and running in daemon mode
//! - A Signal account must be registered with signal-cli
//!
//! # Example
//!
//! ```no_run
//! use integration_signal::{SignalClient, SignalClientConfig};
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! let config = SignalClientConfig::new("+1234567890")
//!     .with_socket_path("/var/run/signal-cli/socket");
//!
//! let client = SignalClient::new(config);
//!
//! // Check if daemon is running
//! if client.is_available().await {
//!     // Send a message
//!     let result = client.send_text("+0987654321", "Hello from Signal!").await?;
//!     println!("Message sent at timestamp: {}", result.timestamp);
//! }
//! # Ok(())
//! # }
//! ```

#![forbid(unsafe_code)]
#![warn(missing_docs)]

mod client;
mod error;
mod types;

pub use client::SignalClient;
pub use error::SignalError;
pub use types::{
    Attachment, DataMessage, Envelope, JsonRpcError, JsonRpcRequest, JsonRpcResponse,
    Quote, ReceiptType, ReceiveParams, SendParams, SendReceiptParams, SendResult,
    SendResultItem, SignalClientConfig, SyncMessage, TypingMessage,
};

#[cfg(test)]
mod integration_tests {
    use super::*;

    /// These tests require signal-cli daemon to be running.
    /// They are ignored by default and should be run manually.
    mod daemon_tests {
        use super::*;

        #[tokio::test]
        #[ignore = "requires signal-cli daemon"]
        async fn test_is_available() {
            let config = SignalClientConfig::new("+1234567890");
            let client = SignalClient::new(config);
            // Will be false unless daemon is running
            let _ = client.is_available().await;
        }

        #[tokio::test]
        #[ignore = "requires signal-cli daemon"]
        async fn test_receive_messages() {
            let config = SignalClientConfig::new("+1234567890");
            let client = SignalClient::new(config);
            // Should return empty vec or messages
            let result = client.receive(0).await;
            assert!(result.is_ok() || matches!(result, Err(SignalError::Connection(_))));
        }
    }
}
