//! API error handling

use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde::Serialize;
use thiserror::Error;

use application::ApplicationError;

/// API error type
#[derive(Debug, Error)]
pub enum ApiError {
    #[error("Bad request: {0}")]
    BadRequest(String),

    #[error("Unauthorized: {0}")]
    Unauthorized(String),

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
            ApiError::BadRequest(msg) => (StatusCode::BAD_REQUEST, "bad_request", msg.clone(), None),
            ApiError::Unauthorized(msg) => (StatusCode::UNAUTHORIZED, "unauthorized", msg.clone(), None),
            ApiError::NotFound(msg) => (StatusCode::NOT_FOUND, "not_found", msg.clone(), None),
            ApiError::RateLimited => (
                StatusCode::TOO_MANY_REQUESTS,
                "rate_limited",
                "Rate limit exceeded".to_string(),
                None,
            ),
            ApiError::ServiceUnavailable(msg) => (
                StatusCode::SERVICE_UNAVAILABLE,
                "service_unavailable",
                msg.clone(),
                None,
            ),
            ApiError::Internal(msg) => (
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
            ApplicationError::Domain(e) => ApiError::BadRequest(e.to_string()),
            ApplicationError::RateLimited => ApiError::RateLimited,
            ApplicationError::NotAuthorized(msg) => ApiError::Unauthorized(msg),
            ApplicationError::Inference(msg) => ApiError::ServiceUnavailable(msg),
            ApplicationError::ExternalService(msg) => ApiError::ServiceUnavailable(msg),
            ApplicationError::ApprovalRequired(msg) => ApiError::BadRequest(format!("Approval required: {}", msg)),
            ApplicationError::Configuration(msg) => ApiError::Internal(msg),
            ApplicationError::CommandFailed(msg) => ApiError::Internal(msg),
            ApplicationError::Internal(msg) => ApiError::Internal(msg),
        }
    }
}
