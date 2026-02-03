//! Inference errors

use thiserror::Error;

/// Errors that can occur during inference
#[derive(Debug, Error)]
pub enum InferenceError {
    /// Failed to connect to inference server
    #[error("Connection failed: {0}")]
    ConnectionFailed(String),

    /// Request to inference server failed
    #[error("Request failed: {0}")]
    RequestFailed(String),

    /// Model not found or not loaded
    #[error("Model not available: {0}")]
    ModelNotAvailable(String),

    /// Response parsing failed
    #[error("Invalid response: {0}")]
    InvalidResponse(String),

    /// Timeout during inference
    #[error("Inference timeout after {0}ms")]
    Timeout(u64),

    /// Rate limit exceeded
    #[error("Rate limit exceeded")]
    RateLimited,

    /// Server error
    #[error("Server error: {0}")]
    ServerError(String),

    /// Streaming error
    #[error("Stream error: {0}")]
    StreamError(String),
}

impl From<reqwest::Error> for InferenceError {
    fn from(err: reqwest::Error) -> Self {
        if err.is_timeout() {
            Self::Timeout(30000)
        } else if err.is_connect() {
            Self::ConnectionFailed(err.to_string())
        } else {
            Self::RequestFailed(err.to_string())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn connection_failed_error_message() {
        let err = InferenceError::ConnectionFailed("refused".to_string());
        assert_eq!(err.to_string(), "Connection failed: refused");
    }

    #[test]
    fn request_failed_error_message() {
        let err = InferenceError::RequestFailed("500 error".to_string());
        assert_eq!(err.to_string(), "Request failed: 500 error");
    }

    #[test]
    fn model_not_available_error_message() {
        let err = InferenceError::ModelNotAvailable("llama".to_string());
        assert_eq!(err.to_string(), "Model not available: llama");
    }

    #[test]
    fn invalid_response_error_message() {
        let err = InferenceError::InvalidResponse("bad json".to_string());
        assert_eq!(err.to_string(), "Invalid response: bad json");
    }

    #[test]
    fn timeout_error_message() {
        let err = InferenceError::Timeout(5000);
        assert_eq!(err.to_string(), "Inference timeout after 5000ms");
    }

    #[test]
    fn rate_limited_error_message() {
        let err = InferenceError::RateLimited;
        assert_eq!(err.to_string(), "Rate limit exceeded");
    }

    #[test]
    fn server_error_message() {
        let err = InferenceError::ServerError("internal".to_string());
        assert_eq!(err.to_string(), "Server error: internal");
    }

    #[test]
    fn stream_error_message() {
        let err = InferenceError::StreamError("connection closed".to_string());
        assert_eq!(err.to_string(), "Stream error: connection closed");
    }

    #[test]
    fn error_has_debug_impl() {
        let err = InferenceError::RateLimited;
        let debug = format!("{err:?}");
        assert!(debug.contains("RateLimited"));
    }
}
