//! Request ID middleware for HTTP request correlation
//!
//! Extracts or generates a unique request ID for each incoming request,
//! making it available in the tracing span for log correlation.

use axum::{body::Body, extract::Request, http::header::HeaderValue, response::Response};
use std::{
    future::Future,
    pin::Pin,
    task::{Context, Poll},
};
use tower::{Layer, Service};
use tracing::Instrument;
use uuid::Uuid;

/// The header name for the request ID
pub const REQUEST_ID_HEADER: &str = "X-Request-Id";

/// Layer that adds request ID handling to HTTP services
#[derive(Debug, Clone, Default)]
pub struct RequestIdLayer;

impl RequestIdLayer {
    /// Create a new request ID layer
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

impl<S> Layer<S> for RequestIdLayer {
    type Service = RequestIdService<S>;

    fn layer(&self, inner: S) -> Self::Service {
        RequestIdService { inner }
    }
}

/// Service that extracts or generates a request ID for each request
#[derive(Debug, Clone)]
pub struct RequestIdService<S> {
    inner: S,
}

impl<S> Service<Request<Body>> for RequestIdService<S>
where
    S: Service<Request<Body>, Response = Response<Body>> + Clone + Send + 'static,
    S::Future: Send,
{
    type Response = S::Response;
    type Error = S::Error;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, mut request: Request<Body>) -> Self::Future {
        // Extract existing request ID from header or generate a new one
        let request_id = request
            .headers()
            .get(REQUEST_ID_HEADER)
            .and_then(|v| v.to_str().ok())
            .and_then(|s| Uuid::parse_str(s).ok())
            .unwrap_or_else(Uuid::now_v7);

        // Store request ID in request extensions for use by handlers
        request.extensions_mut().insert(RequestId(request_id));

        // Create a span with the request ID
        let method = request.method().to_string();
        let uri = request.uri().path().to_string();
        let span = tracing::info_span!(
            "http_request",
            request_id = %request_id,
            method = %method,
            uri = %uri,
        );

        let mut inner = self.inner.clone();

        Box::pin(
            async move {
                let mut response = inner.call(request).await?;

                // Add request ID to response headers
                if let Ok(value) = HeaderValue::from_str(&request_id.to_string()) {
                    response.headers_mut().insert(REQUEST_ID_HEADER, value);
                }

                Ok(response)
            }
            .instrument(span),
        )
    }
}

/// Request ID extracted from the request headers or generated
#[derive(Debug, Clone, Copy)]
pub struct RequestId(pub Uuid);

impl RequestId {
    /// Get the request ID as a UUID
    #[must_use]
    pub const fn as_uuid(&self) -> Uuid {
        self.0
    }
}

impl std::fmt::Display for RequestId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_id_layer_new() {
        let layer = RequestIdLayer::new();
        assert!(std::mem::size_of_val(&layer) == 0); // Zero-sized type
    }

    #[test]
    fn request_id_display() {
        let id = RequestId(Uuid::nil());
        assert_eq!(id.to_string(), "00000000-0000-0000-0000-000000000000");
    }

    #[test]
    fn request_id_as_uuid() {
        let uuid = Uuid::now_v7();
        let id = RequestId(uuid);
        assert_eq!(id.as_uuid(), uuid);
    }

    #[test]
    fn request_id_debug() {
        let id = RequestId(Uuid::nil());
        let debug_str = format!("{id:?}");
        assert!(debug_str.contains("RequestId"));
    }
}
