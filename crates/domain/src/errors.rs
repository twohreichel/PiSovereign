//! Domain-level errors

use thiserror::Error;

/// Errors that can occur in the domain layer
#[derive(Debug, Error)]
pub enum DomainError {
    /// Invalid email address format
    #[error("Invalid email address: {0}")]
    InvalidEmailAddress(String),

    /// Invalid phone number format
    #[error("Invalid phone number: {0}")]
    InvalidPhoneNumber(String),

    /// Invalid command format or parameters
    #[error("Invalid command: {0}")]
    InvalidCommand(String),

    /// Entity not found
    #[error("{entity_type} not found: {id}")]
    NotFound { entity_type: String, id: String },

    /// Validation failed
    #[error("Validation failed: {0}")]
    ValidationError(String),

    /// Operation not permitted
    #[error("Operation not permitted: {0}")]
    NotPermitted(String),

    /// Date/time parsing error
    #[error("Invalid date/time: {0}")]
    InvalidDateTime(String),
}

impl DomainError {
    /// Create a not found error
    pub fn not_found(entity_type: impl Into<String>, id: impl Into<String>) -> Self {
        Self::NotFound {
            entity_type: entity_type.into(),
            id: id.into(),
        }
    }
}
