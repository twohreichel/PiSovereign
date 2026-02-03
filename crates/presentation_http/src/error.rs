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
#[derive(Debug, Serialize)]
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
        let app_err = ApplicationError::Domain(domain::DomainError::not_found("User", "123"));
        let api_err: ApiError = app_err.into();
        assert!(matches!(api_err, ApiError::BadRequest(_)));
    }

    #[test]
    fn application_error_rate_limited_converts() {
        let app_err = ApplicationError::RateLimited;
        let api_err: ApiError = app_err.into();
        assert!(matches!(api_err, ApiError::RateLimited));
    }

    #[test]
    fn application_error_not_authorized_converts() {
        let app_err = ApplicationError::NotAuthorized("no access".to_string());
        let api_err: ApiError = app_err.into();
        assert!(matches!(api_err, ApiError::Unauthorized(_)));
    }

    #[test]
    fn application_error_inference_converts_to_service_unavailable() {
        let app_err = ApplicationError::Inference("model down".to_string());
        let api_err: ApiError = app_err.into();
        assert!(matches!(api_err, ApiError::ServiceUnavailable(_)));
    }

    #[test]
    fn application_error_external_service_converts() {
        let app_err = ApplicationError::ExternalService("api down".to_string());
        let api_err: ApiError = app_err.into();
        assert!(matches!(api_err, ApiError::ServiceUnavailable(_)));
    }

    #[test]
    fn application_error_approval_required_converts_to_bad_request() {
        let app_err = ApplicationError::ApprovalRequired("dangerous".to_string());
        let api_err: ApiError = app_err.into();
        if let ApiError::BadRequest(msg) = api_err {
            assert!(msg.contains("Approval required"));
        } else {
            panic!("Expected BadRequest");
        }
    }

    #[test]
    fn application_error_internal_converts() {
        let app_err = ApplicationError::Internal("crash".to_string());
        let api_err: ApiError = app_err.into();
        assert!(matches!(api_err, ApiError::Internal(_)));
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
        let app_err = ApplicationError::Configuration("bad config".to_string());
        let api_err: ApiError = app_err.into();
        assert!(matches!(api_err, ApiError::Internal(_)));
    }

    #[test]
    fn application_error_command_failed_converts_to_internal() {
        let app_err = ApplicationError::CommandFailed("execution failed".to_string());
        let api_err: ApiError = app_err.into();
        assert!(matches!(api_err, ApiError::Internal(_)));
    }
}
