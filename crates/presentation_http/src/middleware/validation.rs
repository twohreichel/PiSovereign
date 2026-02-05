//! Request validation
//!
//! Provides a `ValidatedJson` extractor that validates request bodies using the validator crate.

use axum::{
    Json,
    extract::{FromRequest, Request, rejection::JsonRejection},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde::de::DeserializeOwned;
use thiserror::Error;
use validator::Validate;

/// Validation error type
#[derive(Debug, Error)]
pub enum ValidationError {
    #[error("Invalid JSON: {0}")]
    JsonError(#[from] JsonRejection),
    #[error("Validation failed: {0}")]
    ValidationFailed(String),
}

impl IntoResponse for ValidationError {
    fn into_response(self) -> Response {
        let (status, message) = match &self {
            Self::JsonError(e) => (StatusCode::BAD_REQUEST, e.to_string()),
            Self::ValidationFailed(msg) => (StatusCode::BAD_REQUEST, msg.clone()),
        };

        let body = serde_json::json!({
            "error": message,
            "code": "validation_error"
        });

        (status, Json(body)).into_response()
    }
}

/// A JSON extractor that also validates the request body
///
/// Use this instead of `Json<T>` when you want automatic validation
/// of the request body using the `validator` crate.
///
/// # Example
///
/// ```ignore
/// use validator::Validate;
/// use serde::Deserialize;
///
/// #[derive(Deserialize, Validate)]
/// struct MyRequest {
///     #[validate(length(min = 1, max = 1000))]
///     message: String,
/// }
///
/// async fn handler(ValidatedJson(req): ValidatedJson<MyRequest>) {
///     // req is validated
/// }
/// ```
#[derive(Debug, Clone, Copy, Default)]
pub struct ValidatedJson<T>(pub T);

impl<T, S> FromRequest<S> for ValidatedJson<T>
where
    T: DeserializeOwned + Validate,
    S: Send + Sync,
{
    type Rejection = ValidationError;

    async fn from_request(req: Request, state: &S) -> Result<Self, Self::Rejection> {
        let Json(value) = Json::<T>::from_request(req, state).await?;

        value.validate().map_err(|e| {
            // Format validation errors nicely
            let errors: Vec<String> = e
                .field_errors()
                .iter()
                .flat_map(|(field, errors)| {
                    errors
                        .iter()
                        .map(|error| {
                            format!(
                                "{}: {}",
                                field,
                                error
                                    .message
                                    .as_ref()
                                    .map_or_else(|| error.code.to_string(), ToString::to_string)
                            )
                        })
                        .collect::<Vec<_>>()
                })
                .collect();

            ValidationError::ValidationFailed(errors.join("; "))
        })?;

        Ok(Self(value))
    }
}

#[cfg(test)]
mod tests {
    use axum::{Router, body::Body, routing::post};
    use serde::Deserialize;
    use tower::ServiceExt;
    use validator::Validate;

    use super::*;

    #[derive(Debug, Deserialize, Validate)]
    struct TestRequest {
        #[validate(length(min = 1, max = 100, message = "must be between 1 and 100 characters"))]
        message: String,
        #[validate(range(min = 0, max = 10, message = "must be between 0 and 10"))]
        #[serde(default)]
        count: u32,
    }

    async fn test_handler(ValidatedJson(req): ValidatedJson<TestRequest>) -> String {
        req.message
    }

    fn create_test_app() -> Router {
        Router::new().route("/test", post(test_handler))
    }

    #[tokio::test]
    async fn valid_request_passes() {
        let app = create_test_app();

        let response = app
            .oneshot(
                axum::http::Request::builder()
                    .method("POST")
                    .uri("/test")
                    .header("content-type", "application/json")
                    .body(Body::from(r#"{"message": "hello"}"#))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn empty_message_rejected() {
        let app = create_test_app();

        let response = app
            .oneshot(
                axum::http::Request::builder()
                    .method("POST")
                    .uri("/test")
                    .header("content-type", "application/json")
                    .body(Body::from(r#"{"message": ""}"#))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn message_too_long_rejected() {
        let app = create_test_app();

        let long_message = "x".repeat(101);
        let json = format!(r#"{{"message": "{long_message}"}}"#);

        let response = app
            .oneshot(
                axum::http::Request::builder()
                    .method("POST")
                    .uri("/test")
                    .header("content-type", "application/json")
                    .body(Body::from(json))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn count_out_of_range_rejected() {
        let app = create_test_app();

        let response = app
            .oneshot(
                axum::http::Request::builder()
                    .method("POST")
                    .uri("/test")
                    .header("content-type", "application/json")
                    .body(Body::from(r#"{"message": "hello", "count": 100}"#))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn invalid_json_rejected() {
        let app = create_test_app();

        let response = app
            .oneshot(
                axum::http::Request::builder()
                    .method("POST")
                    .uri("/test")
                    .header("content-type", "application/json")
                    .body(Body::from(r#"{"message": not valid json}"#))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[test]
    fn validation_error_debug() {
        let error = ValidationError::ValidationFailed("test".to_string());
        let debug = format!("{error:?}");
        assert!(debug.contains("ValidationFailed"));
    }
}
