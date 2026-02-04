//! API key authentication middleware
//!
//! Validates Bearer tokens in the Authorization header against configured API keys.
//! Uses constant-time comparison to prevent timing attacks.

use std::{
    future::Future,
    pin::Pin,
    sync::Arc,
    task::{Context, Poll},
};

use axum::{
    extract::Request,
    http::header::AUTHORIZATION,
    response::{IntoResponse, Response},
};
use subtle::ConstantTimeEq;
use tower::{Layer, Service};

use crate::error::ApiError;

/// Layer that applies API key authentication
#[derive(Clone, Debug)]
pub struct ApiKeyAuthLayer {
    /// The expected API key (if None, auth is disabled)
    api_key: Option<Arc<String>>,
    /// Paths that should be excluded from authentication
    excluded_paths: Vec<String>,
}

impl ApiKeyAuthLayer {
    /// Create a new API key auth layer
    ///
    /// # Arguments
    /// * `api_key` - The expected API key. If None, authentication is disabled.
    #[must_use]
    pub fn new(api_key: Option<String>) -> Self {
        Self {
            api_key: api_key.map(Arc::new),
            excluded_paths: vec!["/health".to_string(), "/ready".to_string()],
        }
    }

    /// Add paths that should be excluded from authentication
    #[must_use]
    pub fn exclude_paths(mut self, paths: Vec<String>) -> Self {
        self.excluded_paths.extend(paths);
        self
    }
}

impl<S> Layer<S> for ApiKeyAuthLayer {
    type Service = ApiKeyAuth<S>;

    fn layer(&self, inner: S) -> Self::Service {
        ApiKeyAuth {
            inner,
            api_key: self.api_key.clone(),
            excluded_paths: self.excluded_paths.clone(),
        }
    }
}

/// Middleware service for API key authentication
#[derive(Clone, Debug)]
pub struct ApiKeyAuth<S> {
    inner: S,
    api_key: Option<Arc<String>>,
    excluded_paths: Vec<String>,
}

impl<S> Service<Request> for ApiKeyAuth<S>
where
    S: Service<Request, Response = Response> + Clone + Send + 'static,
    S::Future: Send,
{
    type Response = Response;
    type Error = S::Error;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, req: Request) -> Self::Future {
        let api_key = self.api_key.clone();
        let excluded_paths = self.excluded_paths.clone();
        let mut inner = self.inner.clone();

        Box::pin(async move {
            // Check if path is excluded from auth
            let path = req.uri().path();
            if excluded_paths.iter().any(|p| path.starts_with(p)) {
                return inner.call(req).await;
            }

            // If no API key is configured, auth is disabled
            let Some(expected_key) = api_key else {
                return inner.call(req).await;
            };

            // Extract Authorization header
            let auth_header = req
                .headers()
                .get(AUTHORIZATION)
                .and_then(|v| v.to_str().ok());

            match auth_header {
                Some(header) if header.starts_with("Bearer ") => {
                    let token = &header[7..]; // Skip "Bearer "

                    // Use constant-time comparison to prevent timing attacks
                    let token_matches = token.as_bytes().ct_eq(expected_key.as_bytes());

                    if token_matches.into() {
                        inner.call(req).await
                    } else {
                        Ok(unauthorized_response("Invalid API key"))
                    }
                },
                Some(_) => Ok(unauthorized_response(
                    "Invalid authorization format, expected Bearer token",
                )),
                None => Ok(unauthorized_response("Missing Authorization header")),
            }
        })
    }
}

fn unauthorized_response(message: &str) -> Response {
    let error = ApiError::Unauthorized(message.to_string());
    error.into_response()
}

#[cfg(test)]
mod tests {
    use axum::{Router, body::Body, http::StatusCode, routing::get};
    use tower::ServiceExt;

    use super::*;

    async fn test_handler() -> &'static str {
        "ok"
    }

    fn create_test_router(api_key: Option<String>) -> Router {
        Router::new()
            .route("/test", get(test_handler))
            .route("/health", get(test_handler))
            .layer(ApiKeyAuthLayer::new(api_key))
    }

    #[tokio::test]
    async fn auth_disabled_when_no_key_configured() {
        let app = create_test_router(None);

        let response = app
            .oneshot(Request::builder().uri("/test").body(Body::empty()).unwrap())
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn valid_bearer_token_passes() {
        let app = create_test_router(Some("secret-key".to_string()));

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/test")
                    .header(AUTHORIZATION, "Bearer secret-key")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn invalid_bearer_token_rejected() {
        let app = create_test_router(Some("secret-key".to_string()));

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/test")
                    .header(AUTHORIZATION, "Bearer wrong-key")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn missing_authorization_header_rejected() {
        let app = create_test_router(Some("secret-key".to_string()));

        let response = app
            .oneshot(Request::builder().uri("/test").body(Body::empty()).unwrap())
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn non_bearer_auth_rejected() {
        let app = create_test_router(Some("secret-key".to_string()));

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/test")
                    .header(AUTHORIZATION, "Basic dXNlcjpwYXNz")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn health_endpoint_excluded_from_auth() {
        let app = create_test_router(Some("secret-key".to_string()));

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/health")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
    }
}
