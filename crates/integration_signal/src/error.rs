//! Error types for Signal integration

use thiserror::Error;

/// Errors that can occur during Signal operations
#[derive(Debug, Error)]
pub enum SignalError {
    /// Connection to signal-cli daemon failed
    #[error("Connection failed: {0}")]
    Connection(String),

    /// JSON-RPC protocol error
    #[error("Protocol error: {0}")]
    Protocol(String),

    /// Signal-cli returned an error
    #[error("Signal-cli error (code {code}): {message}")]
    SignalCli {
        /// Error code from signal-cli
        code: i32,
        /// Error message
        message: String,
    },

    /// Account not registered with Signal
    #[error("Account not registered: {0}")]
    NotRegistered(String),

    /// Message send failed
    #[error("Send failed: {0}")]
    SendFailed(String),

    /// Invalid phone number format
    #[error("Invalid phone number: {0}")]
    InvalidPhoneNumber(String),

    /// Media/attachment error
    #[error("Media error: {0}")]
    Media(String),

    /// Configuration error
    #[error("Configuration error: {0}")]
    Configuration(String),

    /// Timeout waiting for response
    #[error("Operation timed out")]
    Timeout,

    /// IO error
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    /// JSON serialization/deserialization error
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
}

impl SignalError {
    /// Create a connection error
    #[must_use]
    pub fn connection(msg: impl Into<String>) -> Self {
        Self::Connection(msg.into())
    }

    /// Create a protocol error
    #[must_use]
    pub fn protocol(msg: impl Into<String>) -> Self {
        Self::Protocol(msg.into())
    }

    /// Create a signal-cli error
    #[must_use]
    pub fn signal_cli(code: i32, message: impl Into<String>) -> Self {
        Self::SignalCli {
            code,
            message: message.into(),
        }
    }

    /// Create a not registered error
    #[must_use]
    pub fn not_registered(account: impl Into<String>) -> Self {
        Self::NotRegistered(account.into())
    }

    /// Create a send failed error
    #[must_use]
    pub fn send_failed(msg: impl Into<String>) -> Self {
        Self::SendFailed(msg.into())
    }

    /// Create a media error
    #[must_use]
    pub fn media(msg: impl Into<String>) -> Self {
        Self::Media(msg.into())
    }

    /// Create a configuration error
    #[must_use]
    pub fn config(msg: impl Into<String>) -> Self {
        Self::Configuration(msg.into())
    }

    /// Check if this error is retryable
    #[must_use]
    pub const fn is_retryable(&self) -> bool {
        matches!(self, Self::Connection(_) | Self::Timeout)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn connection_error_display() {
        let err = SignalError::connection("socket not found");
        assert_eq!(err.to_string(), "Connection failed: socket not found");
    }

    #[test]
    fn protocol_error_display() {
        let err = SignalError::protocol("invalid JSON-RPC");
        assert_eq!(err.to_string(), "Protocol error: invalid JSON-RPC");
    }

    #[test]
    fn signal_cli_error_display() {
        let err = SignalError::signal_cli(-1, "unknown recipient");
        assert_eq!(
            err.to_string(),
            "Signal-cli error (code -1): unknown recipient"
        );
    }

    #[test]
    fn not_registered_error_display() {
        let err = SignalError::not_registered("+1234567890");
        assert_eq!(err.to_string(), "Account not registered: +1234567890");
    }

    #[test]
    fn send_failed_error_display() {
        let err = SignalError::send_failed("recipient not found");
        assert_eq!(err.to_string(), "Send failed: recipient not found");
    }

    #[test]
    fn media_error_display() {
        let err = SignalError::media("file not found");
        assert_eq!(err.to_string(), "Media error: file not found");
    }

    #[test]
    fn config_error_display() {
        let err = SignalError::config("missing phone number");
        assert_eq!(err.to_string(), "Configuration error: missing phone number");
    }

    #[test]
    fn timeout_error_display() {
        let err = SignalError::Timeout;
        assert_eq!(err.to_string(), "Operation timed out");
    }

    #[test]
    fn connection_is_retryable() {
        assert!(SignalError::connection("test").is_retryable());
    }

    #[test]
    fn timeout_is_retryable() {
        assert!(SignalError::Timeout.is_retryable());
    }

    #[test]
    fn send_failed_is_not_retryable() {
        assert!(!SignalError::send_failed("test").is_retryable());
    }

    #[test]
    fn config_is_not_retryable() {
        assert!(!SignalError::config("test").is_retryable());
    }

    #[test]
    fn io_error_converts() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "file missing");
        let err: SignalError = io_err.into();
        assert!(matches!(err, SignalError::Io(_)));
    }

    #[test]
    fn json_error_converts() {
        let json_err = serde_json::from_str::<String>("invalid").unwrap_err();
        let err: SignalError = json_err.into();
        assert!(matches!(err, SignalError::Json(_)));
    }
}
