//! Application-level errors

use domain::DomainError;
use thiserror::Error;

/// Errors that can occur in the application layer
#[derive(Debug, Error)]
pub enum ApplicationError {
    /// Domain-level error
    #[error(transparent)]
    Domain(#[from] DomainError),

    /// Inference/AI error
    #[error("Inference error: {0}")]
    Inference(String),

    /// External service error
    #[error("External service error: {0}")]
    ExternalService(String),

    /// Command execution failed
    #[error("Command execution failed: {0}")]
    CommandFailed(String),

    /// Approval required for this action
    #[error("Approval required: {0}")]
    ApprovalRequired(String),

    /// Rate limit exceeded
    #[error("Rate limit exceeded")]
    RateLimited,

    /// User not authorized
    #[error("Not authorized: {0}")]
    NotAuthorized(String),

    /// Configuration error
    #[error("Configuration error: {0}")]
    Configuration(String),

    /// Internal error
    #[error("Internal error: {0}")]
    Internal(String),
}

impl ApplicationError {
    /// Check if this error is retryable
    pub fn is_retryable(&self) -> bool {
        matches!(
            self,
            ApplicationError::RateLimited | ApplicationError::ExternalService(_)
        )
    }
}
