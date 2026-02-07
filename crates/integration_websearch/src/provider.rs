//! Search provider trait

use async_trait::async_trait;

use crate::{WebSearchError, WebSearchResponse};

/// Trait for web search providers
///
/// Implemented by all search backends (Brave, DuckDuckGo, etc.)
#[async_trait]
pub trait SearchProvider: Send + Sync {
    /// Perform a web search
    ///
    /// # Arguments
    ///
    /// * `query` - The search query string
    /// * `max_results` - Maximum number of results to return
    ///
    /// # Errors
    ///
    /// Returns an error if the search fails or returns no results.
    async fn search(
        &self,
        query: &str,
        max_results: usize,
    ) -> Result<WebSearchResponse, WebSearchError>;

    /// Check if the search provider is healthy/reachable
    async fn is_healthy(&self) -> bool;

    /// Get the provider name (e.g., "brave", "duckduckgo")
    fn provider_name(&self) -> &'static str;
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use crate::SearchResult;

    /// Mock search provider for testing
    pub struct MockSearchProvider {
        pub results: Vec<SearchResult>,
        pub should_fail: bool,
        pub healthy: bool,
    }

    impl MockSearchProvider {
        #[must_use]
        pub fn new() -> Self {
            Self {
                results: vec![],
                should_fail: false,
                healthy: true,
            }
        }

        #[must_use]
        pub fn with_results(mut self, results: Vec<SearchResult>) -> Self {
            self.results = results;
            self
        }

        #[must_use]
        pub const fn failing(mut self) -> Self {
            self.should_fail = true;
            self
        }

        #[must_use]
        pub const fn unhealthy(mut self) -> Self {
            self.healthy = false;
            self
        }
    }

    impl Default for MockSearchProvider {
        fn default() -> Self {
            Self::new()
        }
    }

    #[async_trait]
    impl SearchProvider for MockSearchProvider {
        async fn search(
            &self,
            query: &str,
            max_results: usize,
        ) -> Result<WebSearchResponse, WebSearchError> {
            if self.should_fail {
                return Err(WebSearchError::ServiceUnavailable(
                    "Mock service unavailable".to_string(),
                ));
            }

            let results: Vec<_> = self.results.iter().take(max_results).cloned().collect();

            Ok(WebSearchResponse::new(query.to_string(), results, "mock"))
        }

        async fn is_healthy(&self) -> bool {
            self.healthy
        }

        fn provider_name(&self) -> &'static str {
            "mock"
        }
    }

    #[tokio::test]
    async fn test_mock_provider_returns_results() {
        let results = vec![SearchResult::new(
            "Test".to_string(),
            "https://example.com".to_string(),
            "Test snippet".to_string(),
            1,
        )];

        let provider = MockSearchProvider::new().with_results(results);
        let response = provider.search("test", 5).await.unwrap();

        assert_eq!(response.results.len(), 1);
        assert_eq!(response.provider, "mock");
    }

    #[tokio::test]
    async fn test_mock_provider_fails_when_configured() {
        let provider = MockSearchProvider::new().failing();
        let result = provider.search("test", 5).await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_mock_provider_health_check() {
        let healthy_provider = MockSearchProvider::new();
        assert!(healthy_provider.is_healthy().await);

        let unhealthy_provider = MockSearchProvider::new().unhealthy();
        assert!(!unhealthy_provider.is_healthy().await);
    }

    #[tokio::test]
    async fn test_mock_provider_respects_max_results() {
        let results = vec![
            SearchResult::new(
                "1".to_string(),
                "https://1.com".to_string(),
                String::new(),
                1,
            ),
            SearchResult::new(
                "2".to_string(),
                "https://2.com".to_string(),
                String::new(),
                2,
            ),
            SearchResult::new(
                "3".to_string(),
                "https://3.com".to_string(),
                String::new(),
                3,
            ),
        ];

        let provider = MockSearchProvider::new().with_results(results);
        let response = provider.search("test", 2).await.unwrap();

        assert_eq!(response.results.len(), 2);
    }
}
