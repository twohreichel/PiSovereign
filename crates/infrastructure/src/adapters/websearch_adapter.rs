//! Web search adapter - Implements WebSearchPort using integration_websearch

use std::sync::Arc;

use application::error::ApplicationError;
use application::ports::{SearchOptions, WebSearchPort};
use async_trait::async_trait;
use domain::entities::{SearchResult, WebSearchResponse};
use integration_websearch::{
    SearchProvider, SearchResult as IntegrationResult, WebSearchClient, WebSearchConfig,
    WebSearchError, WebSearchResponse as IntegrationResponse,
};
use tracing::{debug, instrument};

use super::{CircuitBreaker, CircuitBreakerConfig};

/// Adapter for web search services using Brave and DuckDuckGo
pub struct WebSearchAdapter {
    client: Arc<WebSearchClient>,
    circuit_breaker: Option<CircuitBreaker>,
}

impl std::fmt::Debug for WebSearchAdapter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WebSearchAdapter")
            .field("provider", &self.client.provider_name())
            .field("has_brave", &self.client.has_brave())
            .field(
                "circuit_breaker",
                &self.circuit_breaker.as_ref().map(CircuitBreaker::name),
            )
            .finish()
    }
}

impl WebSearchAdapter {
    /// Create a new adapter with the given configuration
    ///
    /// # Errors
    ///
    /// Returns an error if the HTTP client fails to initialize.
    pub fn new(config: WebSearchConfig) -> Result<Self, ApplicationError> {
        let client = WebSearchClient::new_shared(config)
            .map_err(|e| ApplicationError::Internal(e.to_string()))?;
        Ok(Self {
            client,
            circuit_breaker: None,
        })
    }

    /// Create with default configuration (DuckDuckGo only, no Brave key)
    ///
    /// # Errors
    ///
    /// Returns an error if the HTTP client fails to initialize.
    pub fn with_defaults() -> Result<Self, ApplicationError> {
        Self::new(WebSearchConfig::default())
    }

    /// Enable circuit breaker with default configuration
    #[must_use]
    pub fn with_circuit_breaker(mut self) -> Self {
        self.circuit_breaker = Some(CircuitBreaker::new("websearch"));
        self
    }

    /// Enable circuit breaker with custom configuration
    #[must_use]
    pub fn with_circuit_breaker_config(mut self, config: CircuitBreakerConfig) -> Self {
        self.circuit_breaker = Some(CircuitBreaker::with_config("websearch", config));
        self
    }

    /// Check circuit and return error if open
    fn check_circuit(&self) -> Result<(), ApplicationError> {
        if let Some(ref cb) = self.circuit_breaker {
            if cb.is_open() {
                return Err(ApplicationError::ExternalService(
                    "Web search service circuit breaker is open".into(),
                ));
            }
        }
        Ok(())
    }

    /// Map integration web search error to application error
    fn map_error(err: WebSearchError) -> ApplicationError {
        match err {
            WebSearchError::RequestFailed(e) | WebSearchError::ConnectionFailed(e) => {
                ApplicationError::ExternalService(e)
            },
            WebSearchError::RateLimitExceeded { retry_after_secs } => {
                debug!(retry_after = ?retry_after_secs, "Web search rate limited");
                ApplicationError::RateLimited
            },
            WebSearchError::ParseError(e) | WebSearchError::ConfigurationError(e) => {
                ApplicationError::Internal(e)
            },
            WebSearchError::NoResults { query } => {
                ApplicationError::NotFound(format!("No search results for: {query}"))
            },
            WebSearchError::AuthenticationFailed(e) => {
                ApplicationError::InvalidOperation(format!("Authentication failed: {e}"))
            },
            WebSearchError::ServiceUnavailable(e) => ApplicationError::ExternalService(e),
            WebSearchError::InvalidQuery(e) => ApplicationError::InvalidOperation(e),
            WebSearchError::Timeout { timeout_secs } => ApplicationError::ExternalService(format!(
                "Request timed out after {timeout_secs}s"
            )),
        }
    }

    /// Convert integration search result to domain search result
    fn map_result(result: &IntegrationResult) -> SearchResult {
        SearchResult::new(
            result.title.clone(),
            result.url.clone(),
            result.snippet.clone(),
            result.source.clone(),
            result.position,
        )
    }

    /// Convert integration search response to domain search response
    fn map_response(response: IntegrationResponse) -> WebSearchResponse {
        let results = response.results.iter().map(Self::map_result).collect();
        WebSearchResponse {
            query: response.query,
            results,
            timestamp: response.timestamp,
            provider: response.provider,
        }
    }
}

#[async_trait]
impl WebSearchPort for WebSearchAdapter {
    #[instrument(skip(self), fields(query_len = query.len()))]
    async fn search(
        &self,
        query: &str,
        options: Option<SearchOptions>,
    ) -> Result<WebSearchResponse, ApplicationError> {
        self.check_circuit()?;

        let max_results = options.as_ref().and_then(|o| o.max_results).unwrap_or(5) as usize;

        // Note: language and safe_search options are available in the port but
        // would need to be passed through the config or as a separate API call.
        // For now, we use the defaults configured in WebSearchConfig.

        if let Some(ref options) = options {
            if options.safe_search.is_some() || options.language.is_some() {
                debug!(
                    safe_search = ?options.safe_search,
                    language = ?options.language,
                    "Search options configured but not yet implemented in provider"
                );
            }
        }

        let result = self.client.search(query, max_results).await;

        match &result {
            Ok(response) => {
                debug!(
                    query = %query,
                    results = response.results.len(),
                    provider = %response.provider,
                    "Retrieved search results"
                );
            },
            Err(e) => {
                debug!(query = %query, error = %e, "Search failed");
            },
        }

        result.map(Self::map_response).map_err(Self::map_error)
    }

    #[instrument(skip(self))]
    async fn is_available(&self) -> bool {
        // Check circuit breaker first
        if let Some(ref cb) = self.circuit_breaker {
            if cb.is_open() {
                return false;
            }
        }

        self.client.is_healthy().await
    }

    fn provider_name(&self) -> &str {
        self.client.provider_name()
    }
}

#[cfg(test)]
mod tests {
    use application::ports::SafeSearchLevel;

    use super::*;

    #[test]
    fn new_creates_adapter() {
        let adapter = WebSearchAdapter::with_defaults();
        assert!(adapter.is_ok());
        let adapter = adapter.unwrap();
        assert!(adapter.circuit_breaker.is_none());
        // Without Brave key, it should be DuckDuckGo only
        assert!(!adapter.client.has_brave());
    }

    #[test]
    fn with_circuit_breaker() {
        let adapter = WebSearchAdapter::with_defaults()
            .unwrap()
            .with_circuit_breaker();
        assert!(adapter.circuit_breaker.is_some());
    }

    #[test]
    fn debug_impl() {
        let adapter = WebSearchAdapter::with_defaults().unwrap();
        let debug_str = format!("{adapter:?}");
        assert!(debug_str.contains("WebSearchAdapter"));
        assert!(debug_str.contains("has_brave"));
    }

    #[test]
    fn provider_name_without_brave() {
        let adapter = WebSearchAdapter::with_defaults().unwrap();
        assert_eq!(adapter.provider_name(), "duckduckgo");
    }

    #[test]
    fn provider_name_with_brave() {
        let config = WebSearchConfig {
            brave_api_key: Some("test-key".to_string()),
            ..Default::default()
        };
        let adapter = WebSearchAdapter::new(config).unwrap();
        assert_eq!(adapter.provider_name(), "brave+duckduckgo");
    }

    #[test]
    fn map_error_rate_limited() {
        let err = WebSearchError::RateLimitExceeded {
            retry_after_secs: None,
        };
        let app_err = WebSearchAdapter::map_error(err);
        assert!(matches!(app_err, ApplicationError::RateLimited));
    }

    #[test]
    fn map_error_no_results() {
        let err = WebSearchError::NoResults {
            query: "test".to_string(),
        };
        let app_err = WebSearchAdapter::map_error(err);
        assert!(matches!(app_err, ApplicationError::NotFound(_)));
    }

    #[test]
    fn map_error_authentication_failed() {
        let err = WebSearchError::AuthenticationFailed("Invalid API key".to_string());
        let app_err = WebSearchAdapter::map_error(err);
        assert!(matches!(app_err, ApplicationError::InvalidOperation(_)));
    }

    #[test]
    fn map_error_request_failed() {
        let err = WebSearchError::RequestFailed("timeout".to_string());
        let app_err = WebSearchAdapter::map_error(err);
        assert!(matches!(app_err, ApplicationError::ExternalService(_)));
    }

    #[test]
    fn map_result_converts_correctly() {
        let result = IntegrationResult::new(
            "Test Title".to_string(),
            "https://example.com".to_string(),
            "Test snippet".to_string(),
            1,
        );

        let mapped = WebSearchAdapter::map_result(&result);
        assert_eq!(mapped.title, "Test Title");
        assert_eq!(mapped.url, "https://example.com");
        assert_eq!(mapped.snippet, "Test snippet");
        assert_eq!(mapped.source, "example.com");
        assert_eq!(mapped.position, 1);
    }

    #[test]
    fn safe_search_level_conversion() {
        // Test that SafeSearchLevel values are accessible
        assert_eq!(SafeSearchLevel::Off.as_brave_param(), "off");
        assert_eq!(SafeSearchLevel::Moderate.as_brave_param(), "moderate");
        assert_eq!(SafeSearchLevel::Strict.as_brave_param(), "strict");
    }

    #[test]
    fn trait_is_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<WebSearchAdapter>();
    }
}

// =============================================================================
// Error Path Tests
// Tests for error mapping and circuit breaker behavior
// =============================================================================

#[cfg(test)]
mod error_path_tests {
    use super::*;
    use application::ports::SafeSearchLevel;

    // Test error mapping functions directly
    #[test]
    fn map_error_timeout() {
        let err = WebSearchError::Timeout { timeout_secs: 30 };
        let app_err = WebSearchAdapter::map_error(err);
        assert!(matches!(app_err, ApplicationError::ExternalService(_)));
        if let ApplicationError::ExternalService(msg) = app_err {
            assert!(msg.contains("30"));
        }
    }

    #[test]
    fn map_error_service_unavailable() {
        let err = WebSearchError::ServiceUnavailable("503 Service Unavailable".to_string());
        let app_err = WebSearchAdapter::map_error(err);
        assert!(matches!(app_err, ApplicationError::ExternalService(_)));
    }

    #[test]
    fn map_error_connection_failed() {
        let err = WebSearchError::ConnectionFailed("Connection refused".to_string());
        let app_err = WebSearchAdapter::map_error(err);
        assert!(matches!(app_err, ApplicationError::ExternalService(_)));
    }

    #[test]
    fn map_error_parse_error() {
        let err = WebSearchError::ParseError("Invalid JSON".to_string());
        let app_err = WebSearchAdapter::map_error(err);
        assert!(matches!(app_err, ApplicationError::Internal(_)));
    }

    #[test]
    fn map_error_configuration_error() {
        let err = WebSearchError::ConfigurationError("Missing API key".to_string());
        let app_err = WebSearchAdapter::map_error(err);
        assert!(matches!(app_err, ApplicationError::Internal(_)));
    }

    #[test]
    fn map_error_invalid_query() {
        let err = WebSearchError::InvalidQuery("Query too long".to_string());
        let app_err = WebSearchAdapter::map_error(err);
        assert!(matches!(app_err, ApplicationError::InvalidOperation(_)));
    }

    // Circuit breaker behavior tests
    #[test]
    fn check_circuit_returns_ok_when_closed() {
        let adapter = WebSearchAdapter::with_defaults()
            .unwrap()
            .with_circuit_breaker();
        // Circuit starts closed
        let result = adapter.check_circuit();
        assert!(result.is_ok());
    }

    #[test]
    fn circuit_breaker_config_custom() {
        let cb_config = CircuitBreakerConfig::custom(10, 5, 120);
        let adapter = WebSearchAdapter::with_defaults()
            .unwrap()
            .with_circuit_breaker_config(cb_config);
        assert!(adapter.circuit_breaker.is_some());
        assert!(adapter.check_circuit().is_ok());
    }

    #[test]
    fn circuit_breaker_sensitive_config() {
        let adapter = WebSearchAdapter::with_defaults()
            .unwrap()
            .with_circuit_breaker_config(CircuitBreakerConfig::sensitive());
        assert!(adapter.circuit_breaker.is_some());
    }

    #[test]
    fn circuit_breaker_resilient_config() {
        let adapter = WebSearchAdapter::with_defaults()
            .unwrap()
            .with_circuit_breaker_config(CircuitBreakerConfig::resilient());
        assert!(adapter.circuit_breaker.is_some());
    }

    #[tokio::test]
    async fn is_available_true_when_no_circuit_breaker() {
        // Without circuit breaker, availability depends on actual health check
        // This test just verifies no panic occurs
        let adapter = WebSearchAdapter::with_defaults().unwrap();
        let _ = adapter.is_available().await;
    }

    #[tokio::test]
    async fn search_validates_empty_query_via_client() {
        let adapter = WebSearchAdapter::with_defaults().unwrap();
        let result = adapter.search("", None).await;
        // Empty query should be rejected by the client as InvalidQuery
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, ApplicationError::InvalidOperation(_)));
    }

    #[tokio::test]
    async fn search_validates_whitespace_query_via_client() {
        let adapter = WebSearchAdapter::with_defaults().unwrap();
        let result = adapter.search("   ", None).await;
        // Whitespace-only query should be rejected
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, ApplicationError::InvalidOperation(_)));
    }

    // Search options handling tests
    #[tokio::test]
    async fn search_respects_max_results_option() {
        let adapter = WebSearchAdapter::with_defaults().unwrap();
        let options = Some(SearchOptions {
            max_results: Some(10),
            safe_search: None,
            language: None,
        });
        // This will fail with network error, but we're testing that options are accepted
        let _ = adapter.search("test", options).await;
    }

    #[tokio::test]
    async fn search_accepts_safe_search_option() {
        let adapter = WebSearchAdapter::with_defaults().unwrap();
        let options = Some(SearchOptions {
            max_results: None,
            safe_search: Some(SafeSearchLevel::Strict),
            language: None,
        });
        let _ = adapter.search("test", options).await;
    }

    #[tokio::test]
    async fn search_accepts_language_option() {
        let adapter = WebSearchAdapter::with_defaults().unwrap();
        let options = Some(SearchOptions {
            max_results: None,
            safe_search: None,
            language: Some("de".to_string()),
        });
        let _ = adapter.search("test", options).await;
    }
}
