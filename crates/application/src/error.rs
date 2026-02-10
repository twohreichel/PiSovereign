//! Application-level errors

use domain::DomainError;
use domain::entities::ThreatLevel;
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

    /// Resource not found
    #[error("Not found: {0}")]
    NotFound(String),

    /// Invalid operation
    #[error("Invalid operation: {0}")]
    InvalidOperation(String),

    /// Internal error
    #[error("Internal error: {0}")]
    Internal(String),

    /// Prompt security violation detected
    #[error("Security violation detected: {reason} (threat level: {threat_level})")]
    PromptSecurityViolation {
        /// Description of the security violation
        reason: String,
        /// Severity of the detected threat
        threat_level: ThreatLevel,
    },

    /// IP address is blocked due to suspicious activity
    #[error("Access blocked: {0}")]
    Blocked(String),
}

impl ApplicationError {
    /// Check if this error is retryable
    pub const fn is_retryable(&self) -> bool {
        matches!(self, Self::RateLimited | Self::ExternalService(_))
    }

    /// Check if this error is a security violation
    pub const fn is_security_violation(&self) -> bool {
        matches!(
            self,
            Self::PromptSecurityViolation { .. } | Self::Blocked(_)
        )
    }

    /// Create a prompt security violation error
    #[must_use]
    pub fn security_violation(reason: impl Into<String>, threat_level: ThreatLevel) -> Self {
        Self::PromptSecurityViolation {
            reason: reason.into(),
            threat_level,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use domain::entities::ThreatLevel;

    #[test]
    fn rate_limited_is_retryable() {
        let err = ApplicationError::RateLimited;
        assert!(err.is_retryable());
    }

    #[test]
    fn external_service_is_retryable() {
        let err = ApplicationError::ExternalService("timeout".to_string());
        assert!(err.is_retryable());
    }

    #[test]
    fn inference_error_is_not_retryable() {
        let err = ApplicationError::Inference("model failed".to_string());
        assert!(!err.is_retryable());
    }

    #[test]
    fn command_failed_is_not_retryable() {
        let err = ApplicationError::CommandFailed("syntax error".to_string());
        assert!(!err.is_retryable());
    }

    #[test]
    fn approval_required_is_not_retryable() {
        let err = ApplicationError::ApprovalRequired("email send".to_string());
        assert!(!err.is_retryable());
    }

    #[test]
    fn not_authorized_is_not_retryable() {
        let err = ApplicationError::NotAuthorized("invalid token".to_string());
        assert!(!err.is_retryable());
    }

    #[test]
    fn configuration_is_not_retryable() {
        let err = ApplicationError::Configuration("missing key".to_string());
        assert!(!err.is_retryable());
    }

    #[test]
    fn internal_is_not_retryable() {
        let err = ApplicationError::Internal("unexpected".to_string());
        assert!(!err.is_retryable());
    }

    #[test]
    fn security_violation_is_not_retryable() {
        let err = ApplicationError::PromptSecurityViolation {
            reason: "injection attempt".to_string(),
            threat_level: ThreatLevel::High,
        };
        assert!(!err.is_retryable());
        assert!(err.is_security_violation());
    }

    #[test]
    fn blocked_is_security_violation() {
        let err = ApplicationError::Blocked("Too many violations".to_string());
        assert!(err.is_security_violation());
        assert!(!err.is_retryable());
    }

    #[test]
    fn security_violation_factory() {
        let err = ApplicationError::security_violation("test", ThreatLevel::Critical);
        assert!(matches!(
            err,
            ApplicationError::PromptSecurityViolation {
                threat_level: ThreatLevel::Critical,
                ..
            }
        ));
    }

    #[test]
    fn error_messages_are_correct() {
        assert_eq!(
            ApplicationError::RateLimited.to_string(),
            "Rate limit exceeded"
        );
        assert_eq!(
            ApplicationError::Inference("failed".to_string()).to_string(),
            "Inference error: failed"
        );
        assert_eq!(
            ApplicationError::ExternalService("timeout".to_string()).to_string(),
            "External service error: timeout"
        );
        assert_eq!(
            ApplicationError::CommandFailed("error".to_string()).to_string(),
            "Command execution failed: error"
        );
        assert_eq!(
            ApplicationError::ApprovalRequired("action".to_string()).to_string(),
            "Approval required: action"
        );
        assert_eq!(
            ApplicationError::NotAuthorized("reason".to_string()).to_string(),
            "Not authorized: reason"
        );
        assert_eq!(
            ApplicationError::Configuration("missing".to_string()).to_string(),
            "Configuration error: missing"
        );
        assert_eq!(
            ApplicationError::Internal("oops".to_string()).to_string(),
            "Internal error: oops"
        );
    }

    #[test]
    fn domain_error_converts_to_application_error() {
        let domain_err = DomainError::InvalidEmailAddress("bad".to_string());
        let app_err: ApplicationError = domain_err.into();
        assert!(matches!(app_err, ApplicationError::Domain(_)));
    }
}
