//! API key authentication middleware
//!
//! Validates Bearer tokens in the Authorization header against configured API keys.
//! Uses Argon2 hash verification for secure API key storage and constant-time
//! comparison to prevent timing attacks.
//!
//! API keys are stored as Argon2id hashes in configuration, and incoming keys
//! are verified against these hashes. Each key is associated with a user ID
//! for tenant isolation.

use std::{
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
use domain::{TenantId, UserId};
use infrastructure::{ApiKeyHasher, config::ApiKeyEntry};
use tower::{Layer, Service};
use tracing::{debug, warn};

use crate::error::ApiError;
use crate::middleware::RequestId;

/// Verified API key entry with parsed user ID
#[derive(Clone, Debug)]
struct VerifiedKeyEntry {
    /// Argon2 hash of the API key
    hash: String,
    /// Parsed user ID
    user_id: UserId,
}

/// Storage for API key entries with hash verification
#[derive(Clone, Debug, Default)]
pub struct ApiKeyStore {
    /// Verified API key entries
    entries: Vec<VerifiedKeyEntry>,
    /// Hasher for verification
    hasher: ApiKeyHasher,
}

impl ApiKeyStore {
    /// Create a new empty API key store
    #[must_use]
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
            hasher: ApiKeyHasher::new(),
        }
    }

    /// Create from a list of API key entries
    ///
    /// Parses user IDs and logs warnings for invalid entries.
    #[must_use]
    pub fn from_entries(entries: Vec<ApiKeyEntry>) -> Self {
        let verified_entries: Vec<VerifiedKeyEntry> = entries
            .into_iter()
            .filter_map(|entry| {
                match UserId::parse(&entry.user_id) {
                    Ok(user_id) => Some(VerifiedKeyEntry {
                        hash: entry.hash,
                        user_id,
                    }),
                    Err(e) => {
                        warn!(
                            user_id = %entry.user_id,
                            error = %e,
                            "Invalid user ID format in api_keys configuration, skipping entry"
                        );
                        None
                    }
                }
            })
            .collect();

        Self {
            entries: verified_entries,
            hasher: ApiKeyHasher::new(),
        }
    }

    /// Check if the store is empty
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Verify an API key and return the associated user ID if valid
    ///
    /// Uses Argon2 hash verification which provides constant-time comparison
    /// protection against timing attacks.
    #[must_use]
    pub fn verify(&self, api_key: &str) -> Option<UserId> {
        for entry in &self.entries {
            match self.hasher.verify(api_key, &entry.hash) {
                Ok(true) => {
                    debug!("API key verified successfully");
                    return Some(entry.user_id.clone());
                }
                Ok(false) => continue,
                Err(e) => {
                    warn!(error = %e, "Error verifying API key hash");
                    continue;
                }
            }
        }
        None
    }
}

/// Layer that applies API key authentication
#[derive(Clone, Debug)]
pub struct ApiKeyAuthLayer {
    /// API key store for hash verification
    api_key_store: Arc<ApiKeyStore>,
    /// Paths that should be excluded from authentication
    excluded_paths: Vec<String>,
}

impl ApiKeyAuthLayer {
    /// Create a new API key auth layer with no authentication
    ///
    /// Authentication is disabled - all requests pass through.
    #[must_use]
    pub fn disabled() -> Self {
        Self {
            api_key_store: Arc::new(ApiKeyStore::new()),
            excluded_paths: vec!["/health".to_string(), "/ready".to_string()],
        }
    }

    /// Create a new API key auth layer from API key entries
    ///
    /// Each entry contains an Argon2 hash and associated user ID.
    ///
    /// # Arguments
    /// * `entries` - List of API key entries with hashes and user IDs
    #[must_use]
    pub fn from_api_keys(entries: Vec<ApiKeyEntry>) -> Self {
        Self {
            api_key_store: Arc::new(ApiKeyStore::from_entries(entries)),
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
            api_key_store: Arc::clone(&self.api_key_store),
            excluded_paths: self.excluded_paths.clone(),
        }
    }
}

/// Middleware service for API key authentication
#[derive(Clone, Debug)]
pub struct ApiKeyAuth<S> {
    inner: S,
    api_key_store: Arc<ApiKeyStore>,
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
        let api_key_store = Arc::clone(&self.api_key_store);
        let excluded_paths = self.excluded_paths.clone();
        let mut inner = self.inner.clone();

        Box::pin(async move {
            // Check if path is excluded from auth
            let path = req.uri().path();
            if excluded_paths.iter().any(|p| path.starts_with(p)) {
                return inner.call(req).await;
            }

            // If no API keys configured, auth is disabled
            if api_key_store.is_empty() {
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

                    // Verify API key against stored hashes
                    if let Some(user_id) = api_key_store.verify(token) {
                        inject_request_context(&mut req, user_id);
                        return inner.call(req).await;
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
///
/// # Tenant Resolution
///
/// Currently uses a default `TenantId` (single-tenant mode). In a multi-tenant
/// deployment, this should be enhanced to:
/// 1. Extract tenant from JWT claims (preferred)
/// 2. Extract tenant from `X-Tenant-Id` header
/// 3. Look up tenant based on API key mapping
fn inject_request_context(req: &mut Request, user_id: UserId) {
    // Try to get existing request ID from RequestIdLayer
    let request_id = req
        .extensions()
        .get::<RequestId>()
        .map_or_else(uuid::Uuid::new_v4, |r| r.0);

    // TODO: Extract tenant from JWT claims or X-Tenant-Id header for multi-tenant mode
    let tenant_id = TenantId::default();

    let ctx = RequestContext::with_request_id(user_id, tenant_id, request_id);
    req.extensions_mut().insert(ctx);
}

fn unauthorized_response(message: &str) -> Response {
    let error = ApiError::Unauthorized(message.to_string());
    error.into_response()
}

#[cfg(test)]
mod tests {
    use axum::{Extension, Router, body::Body, http::StatusCode, routing::get};
    use infrastructure::ApiKeyHasher;
    use tower::ServiceExt;

    use super::*;

    async fn test_handler() -> &'static str {
        "ok"
    }

    /// Handler that extracts and returns the user ID from RequestContext
    async fn user_id_handler(Extension(ctx): Extension<RequestContext>) -> String {
        ctx.user_id().to_string()
    }

    fn create_test_router(entries: Vec<ApiKeyEntry>) -> Router {
        Router::new()
            .route("/test", get(test_handler))
            .route("/user", get(user_id_handler))
            .route("/health", get(test_handler))
            .layer(ApiKeyAuthLayer::from_api_keys(entries))
    }

    fn create_disabled_router() -> Router {
        Router::new()
            .route("/test", get(test_handler))
            .route("/health", get(test_handler))
            .layer(ApiKeyAuthLayer::disabled())
    }

    fn hash_key(key: &str) -> String {
        ApiKeyHasher::new().hash(key).unwrap()
    }

    #[tokio::test]
    async fn auth_disabled_when_no_keys_configured() {
        let app = create_disabled_router();

        let response = app
            .oneshot(Request::builder().uri("/test").body(Body::empty()).unwrap())
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn valid_bearer_token_passes() {
        let entries = vec![ApiKeyEntry {
            hash: hash_key("secret-key"),
            user_id: "550e8400-e29b-41d4-a716-446655440001".to_string(),
        }];
        let app = create_test_router(entries);

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
        let entries = vec![ApiKeyEntry {
            hash: hash_key("secret-key"),
            user_id: "550e8400-e29b-41d4-a716-446655440001".to_string(),
        }];
        let app = create_test_router(entries);

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
        let entries = vec![ApiKeyEntry {
            hash: hash_key("secret-key"),
            user_id: "550e8400-e29b-41d4-a716-446655440001".to_string(),
        }];
        let app = create_test_router(entries);

        let response = app
            .oneshot(Request::builder().uri("/test").body(Body::empty()).unwrap())
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn non_bearer_auth_rejected() {
        let entries = vec![ApiKeyEntry {
            hash: hash_key("secret-key"),
            user_id: "550e8400-e29b-41d4-a716-446655440001".to_string(),
        }];
        let app = create_test_router(entries);

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
        let entries = vec![ApiKeyEntry {
            hash: hash_key("secret-key"),
            user_id: "550e8400-e29b-41d4-a716-446655440001".to_string(),
        }];
        let app = create_test_router(entries);

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

    #[tokio::test]
    async fn valid_key_authenticates_and_returns_correct_user() {
        let user_uuid = "550e8400-e29b-41d4-a716-446655440001";
        let entries = vec![ApiKeyEntry {
            hash: hash_key("sk-user1"),
            user_id: user_uuid.to_string(),
        }];
        let app = create_test_router(entries);

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
    async fn multiple_users_each_get_correct_user_id() {
        let entries = vec![
            ApiKeyEntry {
                hash: hash_key("sk-user1"),
                user_id: "550e8400-e29b-41d4-a716-446655440001".to_string(),
            },
            ApiKeyEntry {
                hash: hash_key("sk-user2"),
                user_id: "550e8400-e29b-41d4-a716-446655440002".to_string(),
            },
        ];
        let app = create_test_router(entries);

        // First user
        let response = app
            .clone()
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
        assert_eq!(
            String::from_utf8_lossy(&body),
            "550e8400-e29b-41d4-a716-446655440001"
        );

        // Second user
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/user")
                    .header(AUTHORIZATION, "Bearer sk-user2")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), 1024)
            .await
            .unwrap();
        assert_eq!(
            String::from_utf8_lossy(&body),
            "550e8400-e29b-41d4-a716-446655440002"
        );
    }

    #[tokio::test]
    async fn api_key_store_verify_valid() {
        let hasher = ApiKeyHasher::new();
        let hash = hasher.hash("sk-valid").unwrap();
        
        let entries = vec![ApiKeyEntry {
            hash,
            user_id: "550e8400-e29b-41d4-a716-446655440001".to_string(),
        }];
        let store = ApiKeyStore::from_entries(entries);

        let result = store.verify("sk-valid");
        assert!(result.is_some());
        assert_eq!(
            result.unwrap().to_string(),
            "550e8400-e29b-41d4-a716-446655440001"
        );
    }

    #[tokio::test]
    async fn api_key_store_verify_invalid() {
        let hasher = ApiKeyHasher::new();
        let hash = hasher.hash("sk-valid").unwrap();
        
        let entries = vec![ApiKeyEntry {
            hash,
            user_id: "550e8400-e29b-41d4-a716-446655440001".to_string(),
        }];
        let store = ApiKeyStore::from_entries(entries);

        assert!(store.verify("sk-invalid").is_none());
    }

    #[tokio::test]
    async fn api_key_store_skips_invalid_user_ids() {
        let hasher = ApiKeyHasher::new();
        
        let entries = vec![ApiKeyEntry {
            hash: hasher.hash("sk-test").unwrap(),
            user_id: "not-a-valid-uuid".to_string(),
        }];
        let store = ApiKeyStore::from_entries(entries);

        // Store should be empty because the user ID was invalid
        assert!(store.is_empty());
    }
}
