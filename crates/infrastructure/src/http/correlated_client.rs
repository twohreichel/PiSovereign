//! HTTP client with automatic request ID correlation
//!
//! Provides a wrapper around `reqwest::Client` that automatically adds
//! `X-Request-Id` headers to outgoing requests for distributed tracing.
//!
//! # Examples
//!
//! ```ignore
//! use infrastructure::http::CorrelatedHttpClient;
//! use uuid::Uuid;
//!
//! let client = CorrelatedHttpClient::new();
//! let request_id = Uuid::new_v4();
//!
//! // Make a request with correlation ID
//! let response = client
//!     .get("https://api.example.com/data")
//!     .with_request_id(&request_id)
//!     .send()
//!     .await?;
//! ```

use std::sync::Arc;
use std::time::Duration;

use reqwest::{
    Client, Method, RequestBuilder, Response,
    header::{HeaderMap, HeaderName, HeaderValue},
};
use tracing::{debug, instrument};
use uuid::Uuid;

/// Header name for request correlation ID
pub const X_REQUEST_ID: &str = "x-request-id";

/// Trait for types that can provide a request ID
pub trait RequestIdProvider {
    /// Get the request ID to use for correlation
    fn request_id(&self) -> Uuid;
}

impl RequestIdProvider for Uuid {
    fn request_id(&self) -> Uuid {
        *self
    }
}

impl RequestIdProvider for application::RequestContext {
    fn request_id(&self) -> Uuid {
        self.request_id()
    }
}

/// Configuration for the correlated HTTP client
#[derive(Debug, Clone)]
pub struct CorrelatedClientConfig {
    /// Connection timeout
    pub connect_timeout: Duration,
    /// Request timeout
    pub timeout: Duration,
    /// User agent string
    pub user_agent: String,
    /// Default headers to include in all requests
    pub default_headers: HeaderMap,
}

impl Default for CorrelatedClientConfig {
    fn default() -> Self {
        Self {
            connect_timeout: Duration::from_secs(10),
            timeout: Duration::from_secs(30),
            user_agent: format!("PiSovereign/{}", env!("CARGO_PKG_VERSION")),
            default_headers: HeaderMap::new(),
        }
    }
}

impl CorrelatedClientConfig {
    /// Create a new configuration with custom timeout
    #[must_use]
    pub const fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// Create a new configuration with custom connect timeout
    #[must_use]
    pub const fn with_connect_timeout(mut self, timeout: Duration) -> Self {
        self.connect_timeout = timeout;
        self
    }

    /// Set the user agent string
    #[must_use]
    pub fn with_user_agent(mut self, user_agent: impl Into<String>) -> Self {
        self.user_agent = user_agent.into();
        self
    }

    /// Add a default header to all requests
    #[must_use]
    pub fn with_header(
        mut self,
        name: impl TryInto<HeaderName>,
        value: impl TryInto<HeaderValue>,
    ) -> Self {
        if let (Ok(name), Ok(value)) = (name.try_into(), value.try_into()) {
            self.default_headers.insert(name, value);
        }
        self
    }
}

/// HTTP client that automatically propagates request correlation IDs
///
/// This client wraps `reqwest::Client` and provides builder methods that
/// allow attaching a request ID to outgoing requests. The request ID is
/// added as an `X-Request-Id` header.
#[derive(Debug, Clone)]
pub struct CorrelatedHttpClient {
    inner: Client,
    config: CorrelatedClientConfig,
}

impl CorrelatedHttpClient {
    /// Create a new client with default configuration
    ///
    /// # Errors
    ///
    /// Returns an error if the underlying reqwest client cannot be built.
    pub fn new() -> Result<Self, reqwest::Error> {
        Self::with_config(CorrelatedClientConfig::default())
    }

    /// Create a new client with custom configuration
    ///
    /// # Errors
    ///
    /// Returns an error if the underlying reqwest client cannot be built.
    pub fn with_config(config: CorrelatedClientConfig) -> Result<Self, reqwest::Error> {
        let inner = Client::builder()
            .connect_timeout(config.connect_timeout)
            .timeout(config.timeout)
            .user_agent(&config.user_agent)
            .default_headers(config.default_headers.clone())
            .build()?;

        Ok(Self { inner, config })
    }

    /// Get the configuration
    #[must_use]
    pub const fn config(&self) -> &CorrelatedClientConfig {
        &self.config
    }

    /// Get a reference to the underlying reqwest client
    #[must_use]
    pub const fn inner(&self) -> &Client {
        &self.inner
    }

    /// Start a GET request
    pub fn get(&self, url: impl AsRef<str>) -> CorrelatedRequestBuilder {
        CorrelatedRequestBuilder::new(self.inner.get(url.as_ref()))
    }

    /// Start a POST request
    pub fn post(&self, url: impl AsRef<str>) -> CorrelatedRequestBuilder {
        CorrelatedRequestBuilder::new(self.inner.post(url.as_ref()))
    }

    /// Start a PUT request
    pub fn put(&self, url: impl AsRef<str>) -> CorrelatedRequestBuilder {
        CorrelatedRequestBuilder::new(self.inner.put(url.as_ref()))
    }

    /// Start a DELETE request
    pub fn delete(&self, url: impl AsRef<str>) -> CorrelatedRequestBuilder {
        CorrelatedRequestBuilder::new(self.inner.delete(url.as_ref()))
    }

    /// Start a PATCH request
    pub fn patch(&self, url: impl AsRef<str>) -> CorrelatedRequestBuilder {
        CorrelatedRequestBuilder::new(self.inner.patch(url.as_ref()))
    }

    /// Start a HEAD request
    pub fn head(&self, url: impl AsRef<str>) -> CorrelatedRequestBuilder {
        CorrelatedRequestBuilder::new(self.inner.head(url.as_ref()))
    }

    /// Start a request with a specific method
    pub fn request(&self, method: Method, url: impl AsRef<str>) -> CorrelatedRequestBuilder {
        CorrelatedRequestBuilder::new(self.inner.request(method, url.as_ref()))
    }
}

/// A request builder that supports correlation ID attachment
pub struct CorrelatedRequestBuilder {
    inner: RequestBuilder,
    request_id: Option<Uuid>,
}

impl std::fmt::Debug for CorrelatedRequestBuilder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CorrelatedRequestBuilder")
            .field("request_id", &self.request_id)
            // RequestBuilder doesn't implement Debug, so we skip it
            .finish_non_exhaustive()
    }
}

impl CorrelatedRequestBuilder {
    /// Create a new correlated request builder
    // Note: Cannot be const fn because RequestBuilder is not const-constructible
    #[allow(clippy::missing_const_for_fn)]
    fn new(inner: RequestBuilder) -> Self {
        Self {
            inner,
            request_id: None,
        }
    }

    /// Attach a request ID for correlation
    ///
    /// The ID will be sent as an `X-Request-Id` header.
    #[must_use]
    pub fn with_request_id(mut self, id: &impl RequestIdProvider) -> Self {
        self.request_id = Some(id.request_id());
        self
    }

    /// Add a header to the request
    #[must_use]
    pub fn header(
        mut self,
        name: impl TryInto<HeaderName>,
        value: impl TryInto<HeaderValue>,
    ) -> Self {
        if let (Ok(name), Ok(value)) = (name.try_into(), value.try_into()) {
            self.inner = self.inner.header(name, value);
        }
        self
    }

    /// Set the request body as JSON
    #[must_use]
    pub fn json<T: serde::Serialize + ?Sized>(mut self, json: &T) -> Self {
        self.inner = self.inner.json(json);
        self
    }

    /// Set the request body as form data
    #[must_use]
    pub fn form<T: serde::Serialize + ?Sized>(mut self, form: &T) -> Self {
        self.inner = self.inner.form(form);
        self
    }

    /// Set the request body as raw bytes
    #[must_use]
    pub fn body(mut self, body: impl Into<reqwest::Body>) -> Self {
        self.inner = self.inner.body(body);
        self
    }

    /// Set a query string
    #[must_use]
    pub fn query<T: serde::Serialize + ?Sized>(mut self, query: &T) -> Self {
        self.inner = self.inner.query(query);
        self
    }

    /// Set a bearer auth token
    #[must_use]
    pub fn bearer_auth(mut self, token: impl std::fmt::Display) -> Self {
        self.inner = self.inner.bearer_auth(token);
        self
    }

    /// Set basic auth credentials
    #[must_use]
    pub fn basic_auth(
        mut self,
        username: impl std::fmt::Display,
        password: Option<impl std::fmt::Display>,
    ) -> Self {
        self.inner = self.inner.basic_auth(username, password);
        self
    }

    /// Set the request timeout
    #[must_use]
    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.inner = self.inner.timeout(timeout);
        self
    }

    /// Send the request
    ///
    /// # Errors
    ///
    /// Returns an error if the request fails.
    #[instrument(skip(self), fields(request_id = ?self.request_id))]
    pub async fn send(self) -> Result<Response, reqwest::Error> {
        let mut builder = self.inner;

        // Add request ID header if provided
        if let Some(request_id) = self.request_id {
            builder = builder.header(X_REQUEST_ID, request_id.to_string());
            debug!(request_id = %request_id, "Sending correlated HTTP request");
        }

        // Note: OpenTelemetry trace context propagation can be added here
        // when the 'otel' feature is enabled in infrastructure

        builder.send().await
    }
}

/// Extension trait for adding correlation to existing reqwest clients
pub trait RequestBuilderExt {
    /// Add a request ID header for correlation
    #[must_use]
    fn with_correlation_id(self, id: impl RequestIdProvider) -> Self;
}

impl RequestBuilderExt for RequestBuilder {
    fn with_correlation_id(self, id: impl RequestIdProvider) -> Self {
        self.header(X_REQUEST_ID, id.request_id().to_string())
    }
}

/// Create a shareable correlated HTTP client
pub type SharedCorrelatedClient = Arc<CorrelatedHttpClient>;

/// Create a new shared correlated HTTP client
///
/// # Errors
///
/// Returns an error if the client cannot be built.
pub fn create_shared_client() -> Result<SharedCorrelatedClient, reqwest::Error> {
    Ok(Arc::new(CorrelatedHttpClient::new()?))
}

/// Create a new shared correlated HTTP client with custom config
///
/// # Errors
///
/// Returns an error if the client cannot be built.
pub fn create_shared_client_with_config(
    config: CorrelatedClientConfig,
) -> Result<SharedCorrelatedClient, reqwest::Error> {
    Ok(Arc::new(CorrelatedHttpClient::with_config(config)?))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_default() {
        let config = CorrelatedClientConfig::default();
        assert_eq!(config.connect_timeout, Duration::from_secs(10));
        assert_eq!(config.timeout, Duration::from_secs(30));
        assert!(config.user_agent.contains("PiSovereign"));
    }

    #[test]
    fn config_with_timeout() {
        let config = CorrelatedClientConfig::default().with_timeout(Duration::from_secs(60));
        assert_eq!(config.timeout, Duration::from_secs(60));
    }

    #[test]
    fn config_with_connect_timeout() {
        let config = CorrelatedClientConfig::default().with_connect_timeout(Duration::from_secs(5));
        assert_eq!(config.connect_timeout, Duration::from_secs(5));
    }

    #[test]
    fn config_with_user_agent() {
        let config = CorrelatedClientConfig::default().with_user_agent("TestAgent/1.0");
        assert_eq!(config.user_agent, "TestAgent/1.0");
    }

    #[test]
    fn config_with_header() {
        let config = CorrelatedClientConfig::default().with_header("Authorization", "Bearer token");
        assert!(config.default_headers.contains_key("authorization"));
    }

    #[test]
    fn client_new() {
        let client = CorrelatedHttpClient::new();
        assert!(client.is_ok());
    }

    #[test]
    fn client_with_config() {
        let config = CorrelatedClientConfig::default().with_timeout(Duration::from_secs(60));
        let client = CorrelatedHttpClient::with_config(config);
        assert!(client.is_ok());
    }

    #[test]
    fn request_id_header_constant() {
        assert_eq!(X_REQUEST_ID, "x-request-id");
    }

    #[test]
    fn uuid_implements_request_id_provider() {
        let id = Uuid::new_v4();
        assert_eq!(id.request_id(), id);
    }

    #[test]
    fn client_builds_get_request() {
        let client = CorrelatedHttpClient::new().unwrap();
        let _builder = client.get("https://example.com");
    }

    #[test]
    fn client_builds_post_request() {
        let client = CorrelatedHttpClient::new().unwrap();
        let _builder = client.post("https://example.com");
    }

    #[test]
    fn client_builds_put_request() {
        let client = CorrelatedHttpClient::new().unwrap();
        let _builder = client.put("https://example.com");
    }

    #[test]
    fn client_builds_delete_request() {
        let client = CorrelatedHttpClient::new().unwrap();
        let _builder = client.delete("https://example.com");
    }

    #[test]
    fn client_builds_patch_request() {
        let client = CorrelatedHttpClient::new().unwrap();
        let _builder = client.patch("https://example.com");
    }

    #[test]
    fn client_builds_head_request() {
        let client = CorrelatedHttpClient::new().unwrap();
        let _builder = client.head("https://example.com");
    }

    #[test]
    fn request_builder_with_request_id() {
        let client = CorrelatedHttpClient::new().unwrap();
        let id = Uuid::new_v4();
        let builder = client.get("https://example.com").with_request_id(&id);
        assert_eq!(builder.request_id, Some(id));
    }

    #[test]
    fn request_builder_chaining() {
        let client = CorrelatedHttpClient::new().unwrap();
        let id = Uuid::new_v4();
        let _builder = client
            .post("https://example.com")
            .with_request_id(&id)
            .header("Content-Type", "application/json")
            .timeout(Duration::from_secs(5));
    }

    #[test]
    fn create_shared_client_works() {
        let client = create_shared_client();
        assert!(client.is_ok());
    }

    #[test]
    fn shared_client_is_clone() {
        let client = create_shared_client().unwrap();
        let _clone = Arc::clone(&client);
    }

    #[test]
    fn request_builder_ext_trait() {
        let client = Client::new();
        let id = Uuid::new_v4();
        let _builder = client.get("https://example.com").with_correlation_id(id);
    }

    #[test]
    fn client_inner_access() {
        let client = CorrelatedHttpClient::new().unwrap();
        let _inner = client.inner();
    }

    #[test]
    fn client_config_access() {
        let client = CorrelatedHttpClient::new().unwrap();
        let config = client.config();
        assert!(config.user_agent.contains("PiSovereign"));
    }

    #[test]
    fn client_debug() {
        let client = CorrelatedHttpClient::new().unwrap();
        let debug = format!("{client:?}");
        assert!(debug.contains("CorrelatedHttpClient"));
    }

    #[test]
    fn config_debug() {
        let config = CorrelatedClientConfig::default();
        let debug = format!("{config:?}");
        assert!(debug.contains("CorrelatedClientConfig"));
    }
}
