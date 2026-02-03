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
            InferenceError::Timeout(30000)
        } else if err.is_connect() {
            InferenceError::ConnectionFailed(err.to_string())
        } else {
            InferenceError::RequestFailed(err.to_string())
        }
    }
}
