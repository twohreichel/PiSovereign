//! Security headers middleware
//!
//! Adds security-related HTTP headers to all responses to protect against
//! common web vulnerabilities like XSS, clickjacking, and MIME sniffing.
//!
//! Headers added:
//! - `X-Content-Type-Options: nosniff` - Prevents MIME type sniffing
//! - `X-Frame-Options: DENY` - Prevents clickjacking
//! - `X-XSS-Protection: 1; mode=block` - XSS filter (legacy browsers)
//! - `Referrer-Policy: strict-origin-when-cross-origin` - Controls referrer info
//! - `Content-Security-Policy` - Restricts resource loading (API mode)
//! - `Permissions-Policy` - Restricts browser features
//!
//! # Example
//!
//! ```ignore
//! use presentation_http::middleware::SecurityHeadersLayer;
//!
//! let app = Router::new()
//!     .route("/api", get(handler))
//!     .layer(SecurityHeadersLayer::new());
//! ```

use std::{
    future::Future,
    pin::Pin,
    task::{Context, Poll},
};

use axum::{
    extract::Request,
    http::{HeaderName, HeaderValue},
    response::Response,
};
use tower::{Layer, Service};

/// Layer that adds security headers to all responses
#[derive(Clone, Debug, Default)]
pub struct SecurityHeadersLayer;

impl SecurityHeadersLayer {
    /// Create a new security headers layer
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

impl<S> Layer<S> for SecurityHeadersLayer {
    type Service = SecurityHeaders<S>;

    fn layer(&self, inner: S) -> Self::Service {
        SecurityHeaders { inner }
    }
}

/// Middleware service that adds security headers
#[derive(Clone, Debug)]
pub struct SecurityHeaders<S> {
    inner: S,
}

impl<S> Service<Request> for SecurityHeaders<S>
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
        let mut inner = self.inner.clone();

        Box::pin(async move {
            let mut response = inner.call(req).await?;

            // Add security headers
            let headers = response.headers_mut();

            // Prevent MIME type sniffing
            headers.insert(
                HeaderName::from_static("x-content-type-options"),
                HeaderValue::from_static("nosniff"),
            );

            // Prevent clickjacking - DENY is strictest
            headers.insert(
                HeaderName::from_static("x-frame-options"),
                HeaderValue::from_static("DENY"),
            );

            // XSS Protection for legacy browsers
            // Modern browsers use CSP instead, but this helps older browsers
            headers.insert(
                HeaderName::from_static("x-xss-protection"),
                HeaderValue::from_static("1; mode=block"),
            );

            // Control referrer information
            // strict-origin-when-cross-origin: Full URL for same-origin, only origin for cross-origin
            headers.insert(
                HeaderName::from_static("referrer-policy"),
                HeaderValue::from_static("strict-origin-when-cross-origin"),
            );

            // Content Security Policy
            // For an API server, we restrict everything as we don't serve HTML/JS
            headers.insert(
                HeaderName::from_static("content-security-policy"),
                HeaderValue::from_static("default-src 'none'; frame-ancestors 'none'"),
            );

            // Permissions Policy (formerly Feature-Policy)
            // Disable all powerful browser features for API responses
            headers.insert(
                HeaderName::from_static("permissions-policy"),
                HeaderValue::from_static(
                    "accelerometer=(), camera=(), geolocation=(), gyroscope=(), \
                     magnetometer=(), microphone=(), payment=(), usb=()",
                ),
            );

            // Cache-Control for API responses
            // By default, don't cache API responses to prevent stale data
            if !headers.contains_key("cache-control") {
                headers.insert(
                    HeaderName::from_static("cache-control"),
                    HeaderValue::from_static("no-store, no-cache, must-revalidate"),
                );
            }

            Ok(response)
        })
    }
}

#[cfg(test)]
mod tests {
    use axum::{Router, body::Body, routing::get};
    use tower::ServiceExt;

    use super::*;

    async fn test_handler() -> &'static str {
        "ok"
    }

    #[tokio::test]
    async fn adds_x_content_type_options() {
        let app = Router::new()
            .route("/test", get(test_handler))
            .layer(SecurityHeadersLayer::new());

        let response = app
            .oneshot(Request::builder().uri("/test").body(Body::empty()).unwrap())
            .await
            .unwrap();

        assert_eq!(
            response.headers().get("x-content-type-options"),
            Some(&HeaderValue::from_static("nosniff"))
        );
    }

    #[tokio::test]
    async fn adds_x_frame_options() {
        let app = Router::new()
            .route("/test", get(test_handler))
            .layer(SecurityHeadersLayer::new());

        let response = app
            .oneshot(Request::builder().uri("/test").body(Body::empty()).unwrap())
            .await
            .unwrap();

        assert_eq!(
            response.headers().get("x-frame-options"),
            Some(&HeaderValue::from_static("DENY"))
        );
    }

    #[tokio::test]
    async fn adds_x_xss_protection() {
        let app = Router::new()
            .route("/test", get(test_handler))
            .layer(SecurityHeadersLayer::new());

        let response = app
            .oneshot(Request::builder().uri("/test").body(Body::empty()).unwrap())
            .await
            .unwrap();

        assert_eq!(
            response.headers().get("x-xss-protection"),
            Some(&HeaderValue::from_static("1; mode=block"))
        );
    }

    #[tokio::test]
    async fn adds_referrer_policy() {
        let app = Router::new()
            .route("/test", get(test_handler))
            .layer(SecurityHeadersLayer::new());

        let response = app
            .oneshot(Request::builder().uri("/test").body(Body::empty()).unwrap())
            .await
            .unwrap();

        assert_eq!(
            response.headers().get("referrer-policy"),
            Some(&HeaderValue::from_static("strict-origin-when-cross-origin"))
        );
    }

    #[tokio::test]
    async fn adds_content_security_policy() {
        let app = Router::new()
            .route("/test", get(test_handler))
            .layer(SecurityHeadersLayer::new());

        let response = app
            .oneshot(Request::builder().uri("/test").body(Body::empty()).unwrap())
            .await
            .unwrap();

        assert_eq!(
            response.headers().get("content-security-policy"),
            Some(&HeaderValue::from_static(
                "default-src 'none'; frame-ancestors 'none'"
            ))
        );
    }

    #[tokio::test]
    async fn adds_permissions_policy() {
        let app = Router::new()
            .route("/test", get(test_handler))
            .layer(SecurityHeadersLayer::new());

        let response = app
            .oneshot(Request::builder().uri("/test").body(Body::empty()).unwrap())
            .await
            .unwrap();

        assert!(response.headers().contains_key("permissions-policy"));
    }

    #[tokio::test]
    async fn adds_cache_control_if_not_present() {
        let app = Router::new()
            .route("/test", get(test_handler))
            .layer(SecurityHeadersLayer::new());

        let response = app
            .oneshot(Request::builder().uri("/test").body(Body::empty()).unwrap())
            .await
            .unwrap();

        assert_eq!(
            response.headers().get("cache-control"),
            Some(&HeaderValue::from_static(
                "no-store, no-cache, must-revalidate"
            ))
        );
    }

    #[tokio::test]
    async fn preserves_existing_cache_control() {
        use axum::http::header::CACHE_CONTROL;

        async fn handler_with_cache() -> ([(HeaderName, &'static str); 1], &'static str) {
            ([(CACHE_CONTROL, "public, max-age=3600")], "cached response")
        }

        let app = Router::new()
            .route("/test", get(handler_with_cache))
            .layer(SecurityHeadersLayer::new());

        let response = app
            .oneshot(Request::builder().uri("/test").body(Body::empty()).unwrap())
            .await
            .unwrap();

        assert_eq!(
            response.headers().get("cache-control"),
            Some(&HeaderValue::from_static("public, max-age=3600"))
        );
    }

    #[tokio::test]
    async fn all_security_headers_present() {
        let app = Router::new()
            .route("/test", get(test_handler))
            .layer(SecurityHeadersLayer::new());

        let response = app
            .oneshot(Request::builder().uri("/test").body(Body::empty()).unwrap())
            .await
            .unwrap();

        let headers = response.headers();

        // Verify all expected headers are present
        assert!(headers.contains_key("x-content-type-options"));
        assert!(headers.contains_key("x-frame-options"));
        assert!(headers.contains_key("x-xss-protection"));
        assert!(headers.contains_key("referrer-policy"));
        assert!(headers.contains_key("content-security-policy"));
        assert!(headers.contains_key("permissions-policy"));
        assert!(headers.contains_key("cache-control"));
    }
}
