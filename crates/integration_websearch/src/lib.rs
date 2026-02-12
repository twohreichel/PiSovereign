#![forbid(unsafe_code)]
//! Web search integration for PiSovereign
//!
//! Provides web search capabilities via Brave Search API with DuckDuckGo as fallback.
//! Results are returned with source URLs for citation in LLM responses.
//!
//! # Architecture
//!
//! The crate follows a provider pattern with a common trait [`SearchProvider`] implemented
//! by both [`BraveSearchClient`] and [`DuckDuckGoClient`]. The [`WebSearchClient`] combines
//! both providers with automatic fallback support.
//!
//! # Example
//!
//! ```rust,ignore
//! use integration_websearch::{WebSearchClient, WebSearchConfig};
//!
//! let config = WebSearchConfig::default();
//! let client = WebSearchClient::new(config)?;
//!
//! let results = client.search("Rust programming language", 5).await?;
//! for result in results.results {
//!     println!("[{}] {} - {}", result.source, result.title, result.url);
//! }
//! ```

mod brave;
mod config;
mod duckduckgo;
mod error;
mod models;
mod provider;
mod urlencoding;

pub use brave::BraveSearchClient;
pub use config::WebSearchConfig;
pub use duckduckgo::DuckDuckGoClient;
pub use error::WebSearchError;
pub use models::{SearchResult, WebSearchResponse};
pub use provider::SearchProvider;

use std::sync::Arc;

use async_trait::async_trait;
use tracing::{debug, info, warn};

/// Combined web search client with fallback support
///
/// Uses Brave Search as primary provider and DuckDuckGo as fallback
/// when Brave is unavailable or returns no results.
#[derive(Debug)]
pub struct WebSearchClient {
    brave: Option<BraveSearchClient>,
    duckduckgo: DuckDuckGoClient,
    config: WebSearchConfig,
}

impl WebSearchClient {
    /// Create a new web search client with the given configuration
    ///
    /// # Errors
    ///
    /// Returns an error if the HTTP clients cannot be initialized.
    pub fn new(config: WebSearchConfig) -> Result<Self, WebSearchError> {
        let brave = if config.brave_api_key.is_some() {
            Some(BraveSearchClient::new(&config)?)
        } else {
            warn!("No Brave API key configured, using DuckDuckGo only");
            None
        };

        let duckduckgo = DuckDuckGoClient::new(&config)?;

        Ok(Self {
            brave,
            duckduckgo,
            config,
        })
    }

    /// Create a shareable client wrapped in Arc
    ///
    /// # Errors
    ///
    /// Returns an error if the HTTP clients cannot be initialized.
    pub fn new_shared(config: WebSearchConfig) -> Result<Arc<Self>, WebSearchError> {
        Ok(Arc::new(Self::new(config)?))
    }

    /// Check if Brave Search is configured and available
    #[must_use]
    pub const fn has_brave(&self) -> bool {
        self.brave.is_some()
    }

    /// Check if fallback is enabled
    #[must_use]
    pub const fn fallback_enabled(&self) -> bool {
        self.config.fallback_enabled
    }
}

#[async_trait]
impl SearchProvider for WebSearchClient {
    async fn search(
        &self,
        query: &str,
        max_results: usize,
    ) -> Result<WebSearchResponse, WebSearchError> {
        let max_results = max_results.min(self.config.max_results);

        // Try Brave first if available
        if let Some(ref brave) = self.brave {
            match brave.search(query, max_results).await {
                Ok(response) if !response.results.is_empty() => {
                    debug!(
                        query = %query,
                        results = response.results.len(),
                        "Brave Search returned results"
                    );
                    return Ok(response);
                },
                Ok(_) => {
                    info!(query = %query, "Brave Search returned no results, trying fallback");
                },
                Err(e) => {
                    warn!(
                        query = %query,
                        error = %e,
                        "Brave Search failed, trying fallback"
                    );
                },
            }
        }

        // Fallback to DuckDuckGo
        if self.config.fallback_enabled || self.brave.is_none() {
            debug!(query = %query, "Using DuckDuckGo search");
            self.duckduckgo.search(query, max_results).await
        } else {
            Err(WebSearchError::NoResults {
                query: query.to_string(),
            })
        }
    }

    async fn is_healthy(&self) -> bool {
        // Check Brave first if available
        if let Some(ref brave) = self.brave {
            if brave.is_healthy().await {
                return true;
            }
        }

        // Check DuckDuckGo as fallback
        if self.config.fallback_enabled || self.brave.is_none() {
            return self.duckduckgo.is_healthy().await;
        }

        false
    }

    fn provider_name(&self) -> &'static str {
        if self.brave.is_some() {
            "brave+duckduckgo"
        } else {
            "duckduckgo"
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_client_without_brave_key() {
        let config = WebSearchConfig {
            brave_api_key: None,
            ..Default::default()
        };

        let client = WebSearchClient::new(config).unwrap();
        assert!(!client.has_brave());
        assert_eq!(client.provider_name(), "duckduckgo");
    }

    #[test]
    fn test_client_with_brave_key() {
        let config = WebSearchConfig {
            brave_api_key: Some("test-key".to_string()),
            ..Default::default()
        };

        let client = WebSearchClient::new(config).unwrap();
        assert!(client.has_brave());
        assert_eq!(client.provider_name(), "brave+duckduckgo");
    }
}
