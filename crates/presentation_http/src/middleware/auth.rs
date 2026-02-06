//! API key authentication middleware
//!
//! Validates Bearer tokens in the Authorization header against configured API keys.
//! Uses constant-time comparison to prevent timing attacks.
//!
//! Supports two modes:
//! - Single-key mode: Legacy mode with a single API key for all users
//! - Multi-user mode: Maps API keys to specific user IDs for tenant isolation
//!
//! When multi-user mode is enabled (via `api_key_users`), the middleware:
//! 1. Validates the API key using constant-time comparison
//! 2. Looks up the associated user ID
//! 3. Creates a `RequestContext` and injects it into request extensions
//!
//! Handlers can then extract the `RequestContext` to access the authenticated user.

use std::{
    collections::HashMap,
    future::Future,
    pin::Pin,
    sync::Arc,
    task::{Context, Poll},
};

use application::RequestContext;
use axum::{
    extract::Request,
    http::header::AUTHORIZATION,
    response::{IntoResponse, Response},
};
use domain::UserId;
use subtle::ConstantTimeEq;
use tower::{Layer, Service};
use tracing::warn;

use crate::error::ApiError;
use crate::middleware::RequestId;

/// Mapping of API keys to user IDs for multi-user authentication
#[derive(Clone, Debug, Default)]
pub struct ApiKeyUserMap {
    /// Maps API key strings to user UUID strings
    inner: HashMap<String, String>,
}

impl ApiKeyUserMap {
    /// Create a new empty API key user map
    #[must_use]
    pub fn new() -> Self {
        Self {
            inner: HashMap::new(),
        }
    }

    /// Create from a HashMap
    #[must_use]
    pub const fn from_map(map: HashMap<String, String>) -> Self {
        Self { inner: map }
    }

    /// Check if the map is empty
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    /// Look up user ID for a given API key using constant-time comparison
    ///
    /// Returns the user ID if found, None otherwise.
    /// Uses constant-time comparison to prevent timing attacks.
    #[must_use]
    pub fn lookup(&self, api_key: &str) -> Option<UserId> {
        // Iterate through all keys to prevent timing attacks
        let mut result: Option<UserId> = None;
        for (key, user_id_str) in &self.inner {
            let matches: bool = api_key.as_bytes().ct_eq(key.as_bytes()).into();
            if matches {
                // Parse user ID - log warning if invalid but don't expose to attacker
                result = UserId::parse(user_id_str).ok();
                if result.is_none() {
                    warn!(
                        user_id = %user_id_str,
                        "Invalid user ID format in api_key_users configuration"
                    );
                }
            }
        }
        result
    }
}

/// Layer that applies API key authentication
#[derive(Clone, Debug)]
pub struct ApiKeyAuthLayer {
    /// The expected API key (if None, auth is disabled) - legacy single-key mode
    api_key: Option<Arc<String>>,
    /// API key to user ID mapping for multi-user mode
    api_key_users: Arc<ApiKeyUserMap>,
    /// Paths that should be excluded from authentication
    excluded_paths: Vec<String>,
}

impl ApiKeyAuthLayer {
    /// Create a new API key auth layer with single-key mode (legacy)
    ///
    /// # Arguments
    /// * `api_key` - The expected API key. If None, authentication is disabled.
    #[must_use]
    pub fn new(api_key: Option<String>) -> Self {
        Self {
            api_key: api_key.map(Arc::new),
            api_key_users: Arc::new(ApiKeyUserMap::new()),
            excluded_paths: vec!["/health".to_string(), "/ready".to_string()],
        }
    }

    /// Create a new API key auth layer with multi-user mode
    ///
    /// Multi-user mode maps API keys to specific user IDs, enabling tenant isolation.
    /// The user ID is injected into the request context for use by handlers.
    ///
    /// # Arguments
    /// * `api_key_users` - Mapping of API keys to user UUID strings
    #[must_use]
    pub fn with_user_mapping(api_key_users: HashMap<String, String>) -> Self {
        Self {
            api_key: None,
            api_key_users: Arc::new(ApiKeyUserMap::from_map(api_key_users)),
            excluded_paths: vec!["/health".to_string(), "/ready".to_string()],
        }
    }

    /// Create a new API key auth layer supporting both modes
    ///
    /// If `api_key_users` is non-empty, multi-user mode takes precedence.
    /// Otherwise falls back to single-key mode with `api_key`.
    ///
    /// # Arguments
    /// * `api_key` - Legacy single API key (optional)
    /// * `api_key_users` - Mapping of API keys to user UUID strings
    #[must_use]
    pub fn with_config(api_key: Option<String>, api_key_users: HashMap<String, String>) -> Self {
        Self {
            api_key: api_key.map(Arc::new),
            api_key_users: Arc::new(ApiKeyUserMap::from_map(api_key_users)),
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
            api_key_users: Arc::clone(&self.api_key_users),
            excluded_paths: self.excluded_paths.clone(),
        }
    }
}

/// Middleware service for API key authentication
#[derive(Clone, Debug)]
pub struct ApiKeyAuth<S> {
    inner: S,
    api_key: Option<Arc<String>>,
    api_key_users: Arc<ApiKeyUserMap>,
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

    fn call(&mut self, mut req: Request) -> Self::Future {
        let api_key = self.api_key.clone();
        let api_key_users = Arc::clone(&self.api_key_users);
        let excluded_paths = self.excluded_paths.clone();
        let mut inner = self.inner.clone();

        Box::pin(async move {
            // Check if path is excluded from auth
            let path = req.uri().path();
            if excluded_paths.iter().any(|p| path.starts_with(p)) {
                return inner.call(req).await;
            }

            // If no API key and no user mappings configured, auth is disabled
            let auth_disabled = api_key.is_none() && api_key_users.is_empty();
            if auth_disabled {
                // Inject default request context for unauthenticated requests
                inject_request_context(&mut req, UserId::default());
                return inner.call(req).await;
            }

            // Extract Authorization header
            let auth_header = req
                .headers()
                .get(AUTHORIZATION)
                .and_then(|v| v.to_str().ok());

            match auth_header {
                Some(header) if header.starts_with("Bearer ") => {
                    let token = &header[7..]; // Skip "Bearer "

                    // Try multi-user mode first (api_key_users takes precedence)
                    if !api_key_users.is_empty() {
                        if let Some(user_id) = api_key_users.lookup(token) {
                            inject_request_context(&mut req, user_id);
                            return inner.call(req).await;
                        }
                        // If multi-user mode is active but key not found, reject
                        // (don't fall back to single-key mode for security)
                        return Ok(unauthorized_response("Invalid API key"));
                    }

                    // Fall back to legacy single-key mode
                    if let Some(expected_key) = api_key {
                        let token_matches = token.as_bytes().ct_eq(expected_key.as_bytes());
                        if token_matches.into() {
                            // Single-key mode: use default user ID
                            inject_request_context(&mut req, UserId::default());
                            return inner.call(req).await;
                        }
                    }

                    Ok(unauthorized_response("Invalid API key"))
                },
                Some(_) => Ok(unauthorized_response(
                    "Invalid authorization format, expected Bearer token",
                )),
                None => Ok(unauthorized_response("Missing Authorization header")),
            }
        })
    }
}

/// Inject `RequestContext` into request extensions
///
/// Creates a `RequestContext` with the authenticated user ID and the request ID
/// from the request extensions (if available from the `RequestIdLayer`).
fn inject_request_context(req: &mut Request, user_id: UserId) {
    // Try to get existing request ID from RequestIdLayer
    let request_id = req
        .extensions()
        .get::<RequestId>()
        .map_or_else(uuid::Uuid::new_v4, |r| r.0);

    let ctx = RequestContext::with_request_id(user_id, request_id);
    req.extensions_mut().insert(ctx);
}

fn unauthorized_response(message: &str) -> Response {
    let error = ApiError::Unauthorized(message.to_string());
    error.into_response()
}

#[cfg(test)]
mod tests {
    use axum::{Extension, Router, body::Body, http::StatusCode, routing::get};
    use tower::ServiceExt;

    use super::*;

    async fn test_handler() -> &'static str {
        "ok"
    }

    /// Handler that extracts and returns the user ID from RequestContext
    async fn user_id_handler(Extension(ctx): Extension<RequestContext>) -> String {
        ctx.user_id().to_string()
    }

    fn create_test_router(api_key: Option<String>) -> Router {
        Router::new()
            .route("/test", get(test_handler))
            .route("/health", get(test_handler))
            .layer(ApiKeyAuthLayer::new(api_key))
    }

    fn create_multi_user_router(api_key_users: HashMap<String, String>) -> Router {
        Router::new()
            .route("/test", get(test_handler))
            .route("/user", get(user_id_handler))
            .route("/health", get(test_handler))
            .layer(ApiKeyAuthLayer::with_user_mapping(api_key_users))
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

    // Multi-user mode tests

    #[tokio::test]
    async fn multi_user_valid_key_authenticates() {
        let mut users = HashMap::new();
        users.insert(
            "sk-user1".to_string(),
            "550e8400-e29b-41d4-a716-446655440001".to_string(),
        );
        users.insert(
            "sk-user2".to_string(),
            "550e8400-e29b-41d4-a716-446655440002".to_string(),
        );
        let app = create_multi_user_router(users);

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/test")
                    .header(AUTHORIZATION, "Bearer sk-user1")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn multi_user_invalid_key_rejected() {
        let mut users = HashMap::new();
        users.insert(
            "sk-user1".to_string(),
            "550e8400-e29b-41d4-a716-446655440001".to_string(),
        );
        let app = create_multi_user_router(users);

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/test")
                    .header(AUTHORIZATION, "Bearer sk-unknown")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn multi_user_injects_correct_user_id() {
        let user_uuid = "550e8400-e29b-41d4-a716-446655440001";
        let mut users = HashMap::new();
        users.insert("sk-user1".to_string(), user_uuid.to_string());
        let app = create_multi_user_router(users);

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/user")
                    .header(AUTHORIZATION, "Bearer sk-user1")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), 1024)
            .await
            .unwrap();
        assert_eq!(String::from_utf8_lossy(&body), user_uuid);
    }

    #[tokio::test]
    async fn api_key_user_map_constant_time_lookup() {
        let mut users = HashMap::new();
        users.insert(
            "sk-valid".to_string(),
            "550e8400-e29b-41d4-a716-446655440001".to_string(),
        );
        let map = ApiKeyUserMap::from_map(users);

        // Valid key returns user ID
        let result = map.lookup("sk-valid");
        assert!(result.is_some());
        assert_eq!(
            result.unwrap().to_string(),
            "550e8400-e29b-41d4-a716-446655440001"
        );

        // Invalid key returns None
        assert!(map.lookup("sk-invalid").is_none());
    }

    #[tokio::test]
    async fn with_config_prefers_multi_user_when_available() {
        let mut users = HashMap::new();
        users.insert(
            "sk-multi".to_string(),
            "550e8400-e29b-41d4-a716-446655440001".to_string(),
        );

        // Create layer with both single key and multi-user
        let layer = ApiKeyAuthLayer::with_config(Some("sk-single".to_string()), users);

        let app = Router::new().route("/test", get(test_handler)).layer(layer);

        // Multi-user key should work
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/test")
                    .header(AUTHORIZATION, "Bearer sk-multi")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        // Single key should NOT work when multi-user is active (security)
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/test")
                    .header(AUTHORIZATION, "Bearer sk-single")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }
}
