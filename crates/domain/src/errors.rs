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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn not_found_creates_correct_error() {
        let err = DomainError::not_found("User", "123");
        match err {
            DomainError::NotFound { entity_type, id } => {
                assert_eq!(entity_type, "User");
                assert_eq!(id, "123");
            },
            _ => unreachable!("Expected NotFound error"),
        }
    }

    #[test]
    fn not_found_error_message_is_correct() {
        let err = DomainError::not_found("User", "123");
        assert_eq!(err.to_string(), "User not found: 123");
    }

    #[test]
    fn invalid_email_error_message() {
        let err = DomainError::InvalidEmailAddress("bad-email".to_string());
        assert_eq!(err.to_string(), "Invalid email address: bad-email");
    }

    #[test]
    fn invalid_phone_error_message() {
        let err = DomainError::InvalidPhoneNumber("123".to_string());
        assert_eq!(err.to_string(), "Invalid phone number: 123");
    }

    #[test]
    fn invalid_command_error_message() {
        let err = DomainError::InvalidCommand("unknown".to_string());
        assert_eq!(err.to_string(), "Invalid command: unknown");
    }

    #[test]
    fn validation_error_message() {
        let err = DomainError::ValidationError("field is required".to_string());
        assert_eq!(err.to_string(), "Validation failed: field is required");
    }

    #[test]
    fn not_permitted_error_message() {
        let err = DomainError::NotPermitted("admin only".to_string());
        assert_eq!(err.to_string(), "Operation not permitted: admin only");
    }

    #[test]
    fn invalid_datetime_error_message() {
        let err = DomainError::InvalidDateTime("not a date".to_string());
        assert_eq!(err.to_string(), "Invalid date/time: not a date");
    }
}
