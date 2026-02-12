//! Brave Search API client
//!
//! Client for the Brave Search API (<https://brave.com/search/api/>).

use async_trait::async_trait;
use reqwest::Client;
use std::time::{Duration, Instant};
use tracing::{debug, instrument, warn};

use crate::{
    WebSearchResponse, config::WebSearchConfig, error::WebSearchError, models::SearchResult,
    provider::SearchProvider,
};

/// Brave Search API response structures
#[allow(dead_code)]
mod api {
    use serde::Deserialize;

    #[derive(Debug, Deserialize)]
    pub struct BraveSearchResponse {
        pub web: Option<WebResults>,
        #[serde(default)]
        pub query: QueryInfo,
    }

    #[derive(Debug, Deserialize, Default)]
    pub struct QueryInfo {
        pub original: Option<String>,
    }

    #[derive(Debug, Deserialize)]
    pub struct WebResults {
        pub results: Vec<WebResult>,
    }

    #[derive(Debug, Deserialize)]
    pub struct WebResult {
        pub title: String,
        pub url: String,
        pub description: Option<String>,
        pub age: Option<String>,
        pub thumbnail: Option<Thumbnail>,
    }

    #[derive(Debug, Deserialize)]
    pub struct Thumbnail {
        pub src: Option<String>,
    }
}

/// Brave Search API client
#[derive(Debug)]
pub struct BraveSearchClient {
    client: Client,
    api_key: String,
    base_url: String,
    safe_search: String,
    result_country: String,
}

impl BraveSearchClient {
    /// Create a new Brave Search client
    ///
    /// # Errors
    ///
    /// Returns an error if the API key is missing or HTTP client cannot be created.
    pub fn new(config: &WebSearchConfig) -> Result<Self, WebSearchError> {
        let api_key = config.brave_api_key.clone().ok_or_else(|| {
            WebSearchError::ConfigurationError("Brave API key is required".to_string())
        })?;

        let client = Client::builder()
            .timeout(Duration::from_secs(config.timeout_secs))
            .build()
            .map_err(|e| WebSearchError::ConnectionFailed(e.to_string()))?;

        Ok(Self {
            client,
            api_key,
            base_url: config.brave_base_url.clone(),
            safe_search: config.safe_search.clone(),
            result_country: config.result_country.clone(),
        })
    }

    /// Build the search URL with query parameters
    fn build_search_url(&self, query: &str, count: usize) -> String {
        let encoded_query = urlencoding::encode(query);
        format!(
            "{}/web/search?q={}&count={}&safesearch={}&country={}",
            self.base_url,
            encoded_query,
            count.min(20), // Brave API limit
            self.safe_search,
            self.result_country.to_lowercase()
        )
    }

    /// Convert API response to search results
    #[allow(clippy::cast_possible_truncation)]
    fn convert_results(response: api::BraveSearchResponse, _query: &str) -> Vec<SearchResult> {
        response
            .web
            .map(|web| {
                web.results
                    .into_iter()
                    .enumerate()
                    .map(|(i, r)| {
                        let mut result = SearchResult::new(
                            r.title,
                            r.url,
                            r.description.unwrap_or_default(),
                            (i + 1) as u32,
                        );
                        result.thumbnail_url = r.thumbnail.and_then(|t| t.src);
                        result
                    })
                    .collect()
            })
            .unwrap_or_default()
    }
}

#[async_trait]
impl SearchProvider for BraveSearchClient {
    #[instrument(skip(self), fields(provider = "brave"))]
    async fn search(
        &self,
        query: &str,
        max_results: usize,
    ) -> Result<WebSearchResponse, WebSearchError> {
        let query = query.trim();
        if query.is_empty() {
            return Err(WebSearchError::InvalidQuery(
                "Search query cannot be empty".to_string(),
            ));
        }

        let url = self.build_search_url(query, max_results);
        let start = Instant::now();

        debug!(url = %url, "Sending Brave Search request");

        let response = self
            .client
            .get(&url)
            .header("X-Subscription-Token", &self.api_key)
            .header("Accept", "application/json")
            .send()
            .await
            .map_err(|e| {
                if e.is_timeout() {
                    WebSearchError::Timeout { timeout_secs: 30 }
                } else if e.is_connect() {
                    WebSearchError::ConnectionFailed(e.to_string())
                } else {
                    WebSearchError::RequestFailed(e.to_string())
                }
            })?;

        let status = response.status();
        debug!(status = %status, "Received Brave Search response");

        if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
            let retry_after = response
                .headers()
                .get("Retry-After")
                .and_then(|h| h.to_str().ok())
                .and_then(|s| s.parse().ok());

            return Err(WebSearchError::RateLimitExceeded {
                retry_after_secs: retry_after,
            });
        }

        if status == reqwest::StatusCode::UNAUTHORIZED {
            return Err(WebSearchError::AuthenticationFailed(
                "Invalid Brave API key".to_string(),
            ));
        }

        if !status.is_success() {
            let error_text = response.text().await.unwrap_or_default();
            return Err(WebSearchError::RequestFailed(format!(
                "HTTP {status}: {error_text}"
            )));
        }

        let api_response: api::BraveSearchResponse = response
            .json()
            .await
            .map_err(|e| WebSearchError::ParseError(e.to_string()))?;

        let results = Self::convert_results(api_response, query);
        let elapsed = start.elapsed();

        if results.is_empty() {
            return Err(WebSearchError::NoResults {
                query: query.to_string(),
            });
        }

        let mut response = WebSearchResponse::new(query.to_string(), results, "brave");
        #[allow(clippy::cast_possible_truncation)]
        {
            response.search_time_ms = Some(elapsed.as_millis() as u64);
        }

        debug!(
            results = response.results.len(),
            time_ms = elapsed.as_millis(),
            "Brave Search completed"
        );

        Ok(response)
    }

    async fn is_healthy(&self) -> bool {
        // Perform a minimal search to check health
        // Using a simple query that should always return results
        match self.search("test", 1).await {
            Ok(_) => true,
            Err(e) => {
                warn!(error = %e, "Brave Search health check failed");
                false
            },
        }
    }

    fn provider_name(&self) -> &'static str {
        "brave"
    }
}

use crate::urlencoding;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_url_encoding() {
        assert_eq!(urlencoding::encode("hello world"), "hello+world");
        assert_eq!(urlencoding::encode("test&query"), "test%26query");
        assert_eq!(urlencoding::encode("rust-lang"), "rust-lang");
    }

    #[test]
    fn test_build_search_url() {
        let config = WebSearchConfig {
            brave_api_key: Some("test-key".to_string()),
            brave_base_url: "https://api.search.brave.com/res/v1".to_string(),
            safe_search: "moderate".to_string(),
            result_country: "DE".to_string(),
            ..Default::default()
        };

        let client = BraveSearchClient::new(&config).unwrap();
        let url = client.build_search_url("rust programming", 5);

        assert!(url.contains("q=rust+programming"));
        assert!(url.contains("count=5"));
        assert!(url.contains("safesearch=moderate"));
        assert!(url.contains("country=de"));
    }

    #[test]
    fn test_convert_results() {
        let api_response = api::BraveSearchResponse {
            web: Some(api::WebResults {
                results: vec![api::WebResult {
                    title: "Rust Lang".to_string(),
                    url: "https://rust-lang.org".to_string(),
                    description: Some("A systems programming language".to_string()),
                    age: None,
                    thumbnail: None,
                }],
            }),
            query: api::QueryInfo {
                original: Some("rust".to_string()),
            },
        };

        let results = BraveSearchClient::convert_results(api_response, "rust");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].title, "Rust Lang");
        assert_eq!(results[0].position, 1);
    }

    #[test]
    fn test_convert_empty_results() {
        let api_response = api::BraveSearchResponse {
            web: None,
            query: api::QueryInfo { original: None },
        };

        let results = BraveSearchClient::convert_results(api_response, "rust");
        assert!(results.is_empty());
    }

    #[test]
    fn test_client_requires_api_key() {
        let config = WebSearchConfig {
            brave_api_key: None,
            ..Default::default()
        };

        let result = BraveSearchClient::new(&config);
        assert!(matches!(result, Err(WebSearchError::ConfigurationError(_))));
    }
}
