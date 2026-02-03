//! API error handling

use application::ApplicationError;
use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde::Serialize;
use thiserror::Error;

/// API error type
#[derive(Debug, Error)]
pub enum ApiError {
    #[error("Bad request: {0}")]
    BadRequest(String),

    #[error("Unauthorized: {0}")]
    Unauthorized(String),

    #[error("Not found: {0}")]
    #[allow(dead_code)]
    NotFound(String),

    #[error("Rate limited")]
    RateLimited,

    #[error("Service unavailable: {0}")]
    ServiceUnavailable(String),

    #[error("Internal error: {0}")]
    Internal(String),
}

/// Error response body
#[derive(Serialize)]
pub struct ErrorResponse {
    pub error: String,
    pub code: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<String>,
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, code, message, details) = match &self {
            Self::BadRequest(msg) => (StatusCode::BAD_REQUEST, "bad_request", msg.clone(), None),
            Self::Unauthorized(msg) => {
                (StatusCode::UNAUTHORIZED, "unauthorized", msg.clone(), None)
            },
            Self::NotFound(msg) => (StatusCode::NOT_FOUND, "not_found", msg.clone(), None),
            Self::RateLimited => (
                StatusCode::TOO_MANY_REQUESTS,
                "rate_limited",
                "Rate limit exceeded".to_string(),
                None,
            ),
            Self::ServiceUnavailable(msg) => (
                StatusCode::SERVICE_UNAVAILABLE,
                "service_unavailable",
                msg.clone(),
                None,
            ),
            Self::Internal(msg) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "internal_error",
                "An internal error occurred".to_string(),
                Some(msg.clone()),
            ),
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
            ApplicationError::Inference(msg) | ApplicationError::ExternalService(msg) => {
                Self::ServiceUnavailable(msg)
            },
            ApplicationError::ApprovalRequired(msg) => {
                Self::BadRequest(format!("Approval required: {msg}"))
            },
            ApplicationError::Configuration(msg)
            | ApplicationError::CommandFailed(msg)
            | ApplicationError::Internal(msg) => Self::Internal(msg),
        }
    }
}
