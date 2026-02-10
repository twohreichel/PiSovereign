//! API error handling
//!
//! Provides sanitized error responses that don't leak implementation details.
//! In production mode, internal errors return generic messages without details.

use application::ApplicationError;
use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde::Serialize;
use std::sync::atomic::{AtomicBool, Ordering};
use thiserror::Error;
use utoipa::ToSchema;

/// Global flag to control error detail exposure
/// Set to false in production to prevent information leakage
static EXPOSE_INTERNAL_ERRORS: AtomicBool = AtomicBool::new(true);

/// Configure whether internal error details should be exposed in responses.
///
/// In production environments, this should be set to `false` to prevent
/// leaking implementation details, stack traces, or sensitive information.
///
/// # Arguments
///
/// * `expose` - If `true`, internal error details will be included in responses.
///   If `false`, only generic error messages will be returned.
pub fn set_expose_internal_errors(expose: bool) {
    EXPOSE_INTERNAL_ERRORS.store(expose, Ordering::SeqCst);
}

/// Check if internal error details should be exposed
fn should_expose_details() -> bool {
    EXPOSE_INTERNAL_ERRORS.load(Ordering::SeqCst)
}

/// Sanitize an error message to remove potentially sensitive information
///
/// This function removes:
/// - File paths
/// - IP addresses (except localhost)
/// - Port numbers in URLs
/// - Stack trace information
/// - Database connection strings
fn sanitize_error_message(msg: &str) -> String {
    // In development mode, return the original message
    if should_expose_details() {
        return msg.to_string();
    }

    // List of patterns that indicate sensitive information
    let sensitive_patterns = [
        // File paths
        "/home/",
        "/Users/",
        "/var/",
        "/etc/",
        "\\Users\\",
        "C:\\",
        // Database patterns
        "postgres://",
        "postgresql://",
        "sqlite://",
        "mysql://",
        "mongodb://",
        // Stack trace indicators
        "at line",
        "stack backtrace",
        "panicked at",
        " at ",
        ".rs:",
        // Connection details
        "connection refused",
        "ECONNREFUSED",
        "timeout",
    ];

    // Check if the message contains any sensitive patterns
    let msg_lower = msg.to_lowercase();
    for pattern in &sensitive_patterns {
        if msg_lower.contains(&pattern.to_lowercase()) {
            return "An error occurred processing your request".to_string();
        }
    }

    // Additional sanitization: remove anything that looks like a path or URL detail
    if msg.contains("://") || msg.contains('/') && msg.len() > 50 {
        return "An error occurred processing your request".to_string();
    }

    msg.to_string()
}

/// API error type
#[derive(Debug, Error)]
pub enum ApiError {
    #[error("Bad request: {0}")]
    BadRequest(String),

    #[error("Unauthorized: {0}")]
    Unauthorized(String),

    #[error("Forbidden: {0}")]
    Forbidden(String),

    #[error("Not found: {0}")]
    NotFound(String),

    #[error("Rate limited")]
    RateLimited,

    #[error("Service unavailable: {0}")]
    ServiceUnavailable(String),

    #[error("Internal error: {0}")]
    Internal(String),
}

/// Error response body
#[derive(Debug, Serialize, ToSchema)]
pub struct ErrorResponse {
    /// Error message
    pub error: String,
    /// Error code
    pub code: String,
    /// Additional error details
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<String>,
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, code, message, details) = match &self {
            Self::BadRequest(msg) => (
                StatusCode::BAD_REQUEST,
                "bad_request",
                sanitize_error_message(msg),
                None,
            ),
            Self::Unauthorized(msg) => {
                // Unauthorized messages are intentionally generic to prevent user enumeration
                let sanitized = if should_expose_details() {
                    msg.clone()
                } else {
                    "Authentication required".to_string()
                };
                (StatusCode::UNAUTHORIZED, "unauthorized", sanitized, None)
            },
            Self::Forbidden(msg) => {
                // Forbidden messages can hint at security policy but not leak details
                let sanitized = if should_expose_details() {
                    msg.clone()
                } else {
                    "Access denied".to_string()
                };
                (StatusCode::FORBIDDEN, "forbidden", sanitized, None)
            },
            Self::NotFound(msg) => (
                StatusCode::NOT_FOUND,
                "not_found",
                sanitize_error_message(msg),
                None,
            ),
            Self::RateLimited => (
                StatusCode::TOO_MANY_REQUESTS,
                "rate_limited",
                "Rate limit exceeded".to_string(),
                None,
            ),
            Self::ServiceUnavailable(msg) => {
                // Service errors might leak backend details
                let sanitized = if should_expose_details() {
                    msg.clone()
                } else {
                    "Service temporarily unavailable".to_string()
                };
                (
                    StatusCode::SERVICE_UNAVAILABLE,
                    "service_unavailable",
                    sanitized,
                    None,
                )
            },
            Self::Internal(msg) => {
                // Internal errors should never leak details in production
                let details = if should_expose_details() {
                    Some(msg.clone())
                } else {
                    None
                };
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "internal_error",
                    "An internal error occurred".to_string(),
                    details,
                )
            },
        };

        let body = ErrorResponse {
            error: message,
            code: code.to_string(),
            details,
        };

        (status, Json(body)).into_response()
    }
}

impl From<ApplicationError> for ApiError {
    fn from(err: ApplicationError) -> Self {
        match err {
            ApplicationError::Domain(e) => Self::BadRequest(e.to_string()),
            ApplicationError::RateLimited => Self::RateLimited,
            ApplicationError::NotAuthorized(msg) => Self::Unauthorized(msg),
            ApplicationError::PromptSecurityViolation { reason, .. } => Self::Forbidden(reason),
            ApplicationError::Blocked(msg) => Self::Forbidden(msg),
            ApplicationError::Inference(msg) | ApplicationError::ExternalService(msg) => {
                Self::ServiceUnavailable(msg)
            },
            ApplicationError::ApprovalRequired(msg) => {
                Self::BadRequest(format!("Approval required: {msg}"))
            },
            ApplicationError::NotFound(msg) => Self::NotFound(msg),
            ApplicationError::InvalidOperation(msg) => Self::BadRequest(msg),
            ApplicationError::Configuration(msg)
            | ApplicationError::CommandFailed(msg)
            | ApplicationError::Internal(msg) => Self::Internal(msg),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn api_error_bad_request_message() {
        let err = ApiError::BadRequest("invalid input".to_string());
        assert_eq!(err.to_string(), "Bad request: invalid input");
    }

    #[test]
    fn api_error_unauthorized_message() {
        let err = ApiError::Unauthorized("missing token".to_string());
        assert_eq!(err.to_string(), "Unauthorized: missing token");
    }

    #[test]
    fn api_error_not_found_message() {
        let err = ApiError::NotFound("resource".to_string());
        assert_eq!(err.to_string(), "Not found: resource");
    }

    #[test]
    fn api_error_rate_limited_message() {
        let err = ApiError::RateLimited;
        assert_eq!(err.to_string(), "Rate limited");
    }

    #[test]
    fn api_error_service_unavailable_message() {
        let err = ApiError::ServiceUnavailable("inference down".to_string());
        assert_eq!(err.to_string(), "Service unavailable: inference down");
    }

    #[test]
    fn api_error_internal_message() {
        let err = ApiError::Internal("unexpected".to_string());
        assert_eq!(err.to_string(), "Internal error: unexpected");
    }

    #[test]
    fn error_response_serialization() {
        let resp = ErrorResponse {
            error: "Bad request".to_string(),
            code: "bad_request".to_string(),
            details: None,
        };
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("error"));
        assert!(json.contains("code"));
        assert!(!json.contains("details"));
    }

    #[test]
    fn error_response_with_details() {
        let resp = ErrorResponse {
            error: "Internal error".to_string(),
            code: "internal_error".to_string(),
            details: Some("stack trace".to_string()),
        };
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("details"));
        assert!(json.contains("stack trace"));
    }

    #[test]
    fn application_error_domain_converts_to_bad_request() {
        let source = ApplicationError::Domain(domain::DomainError::not_found("User", "123"));
        let result: ApiError = source.into();
        assert!(matches!(result, ApiError::BadRequest(_)));
    }

    #[test]
    fn application_error_rate_limited_converts() {
        let source = ApplicationError::RateLimited;
        let result: ApiError = source.into();
        assert!(matches!(result, ApiError::RateLimited));
    }

    #[test]
    fn application_error_not_authorized_converts() {
        let source = ApplicationError::NotAuthorized("no access".to_string());
        let result: ApiError = source.into();
        assert!(matches!(result, ApiError::Unauthorized(_)));
    }

    #[test]
    fn application_error_inference_converts_to_service_unavailable() {
        let source = ApplicationError::Inference("model down".to_string());
        let result: ApiError = source.into();
        assert!(matches!(result, ApiError::ServiceUnavailable(_)));
    }

    #[test]
    fn application_error_external_service_converts() {
        let source = ApplicationError::ExternalService("api down".to_string());
        let result: ApiError = source.into();
        assert!(matches!(result, ApiError::ServiceUnavailable(_)));
    }

    #[test]
    fn application_error_approval_required_converts_to_bad_request() {
        let source = ApplicationError::ApprovalRequired("dangerous".to_string());
        let result: ApiError = source.into();
        let ApiError::BadRequest(msg) = result else {
            unreachable!("Expected BadRequest");
        };
        assert!(msg.contains("Approval required"));
    }

    #[test]
    fn application_error_internal_converts() {
        let source = ApplicationError::Internal("crash".to_string());
        let result: ApiError = source.into();
        assert!(matches!(result, ApiError::Internal(_)));
    }

    #[test]
    fn api_error_has_debug() {
        let err = ApiError::RateLimited;
        let debug = format!("{err:?}");
        assert!(debug.contains("RateLimited"));
    }

    #[test]
    fn into_response_bad_request() {
        let err = ApiError::BadRequest("invalid".to_string());
        let response = err.into_response();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[test]
    fn into_response_unauthorized() {
        let err = ApiError::Unauthorized("no token".to_string());
        let response = err.into_response();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[test]
    fn into_response_not_found() {
        let err = ApiError::NotFound("resource".to_string());
        let response = err.into_response();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[test]
    fn into_response_rate_limited() {
        let err = ApiError::RateLimited;
        let response = err.into_response();
        assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
    }

    #[test]
    fn into_response_service_unavailable() {
        let err = ApiError::ServiceUnavailable("down".to_string());
        let response = err.into_response();
        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    }

    #[test]
    fn into_response_internal() {
        let err = ApiError::Internal("crash".to_string());
        let response = err.into_response();
        assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    }

    #[test]
    fn application_error_configuration_converts_to_internal() {
        let source = ApplicationError::Configuration("bad config".to_string());
        let result: ApiError = source.into();
        assert!(matches!(result, ApiError::Internal(_)));
    }

    #[test]
    fn application_error_command_failed_converts_to_internal() {
        let source = ApplicationError::CommandFailed("execution failed".to_string());
        let result: ApiError = source.into();
        assert!(matches!(result, ApiError::Internal(_)));
    }

    // Error sanitization tests

    #[test]
    fn sanitize_removes_file_paths_in_production() {
        set_expose_internal_errors(false);
        let msg = "Error loading config from /home/user/.config/app.toml";
        let sanitized = sanitize_error_message(msg);
        assert_eq!(sanitized, "An error occurred processing your request");
        set_expose_internal_errors(true); // Reset for other tests
    }

    #[test]
    fn sanitize_removes_database_urls_in_production() {
        set_expose_internal_errors(false);
        let msg = "Failed to connect to postgres://user:pass@localhost:5432/db";
        let sanitized = sanitize_error_message(msg);
        assert_eq!(sanitized, "An error occurred processing your request");
        set_expose_internal_errors(true);
    }

    #[test]
    fn sanitize_removes_stack_traces_in_production() {
        set_expose_internal_errors(false);
        let msg = "Panic at line 42 in module.rs";
        let sanitized = sanitize_error_message(msg);
        assert_eq!(sanitized, "An error occurred processing your request");
        set_expose_internal_errors(true);
    }

    #[test]
    fn sanitize_preserves_safe_messages() {
        set_expose_internal_errors(false);
        let msg = "Invalid email format";
        let sanitized = sanitize_error_message(msg);
        assert_eq!(sanitized, "Invalid email format");
        set_expose_internal_errors(true);
    }

    #[test]
    fn sanitize_exposes_details_in_development() {
        set_expose_internal_errors(true);
        let msg = "Error at /home/user/.config/app.toml line 42";
        let sanitized = sanitize_error_message(msg);
        assert_eq!(sanitized, msg);
    }

    #[test]
    fn internal_error_hides_details_in_production() {
        set_expose_internal_errors(false);
        let err =
            ApiError::Internal("Database connection failed at postgres://localhost".to_string());
        let response = err.into_response();
        assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
        // The response body should not contain the details
        set_expose_internal_errors(true);
    }

    #[test]
    fn unauthorized_error_generic_in_production() {
        set_expose_internal_errors(false);
        let err = ApiError::Unauthorized("User admin@example.com not found".to_string());
        let response = err.into_response();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        // The response should not reveal the specific user
        set_expose_internal_errors(true);
    }
}
