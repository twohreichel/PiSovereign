//! Web search service port
//!
//! Defines the interface for web search operations, allowing the LLM
//! to search the internet and retrieve results with citations.

use async_trait::async_trait;
use domain::entities::{SearchResult, WebSearchResponse};
#[cfg(test)]
use mockall::automock;

use crate::error::ApplicationError;

/// Options for web search queries
#[derive(Debug, Clone, Default)]
pub struct SearchOptions {
    /// Maximum number of results to return (default: 5)
    pub max_results: Option<u32>,

    /// Language preference for search results (e.g., "de", "en")
    pub language: Option<String>,

    /// Safe search level (off, moderate, strict)
    pub safe_search: Option<SafeSearchLevel>,
}

impl SearchOptions {
    /// Create new search options with default values
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Set maximum results
    #[must_use]
    pub const fn with_max_results(mut self, max: u32) -> Self {
        self.max_results = Some(max);
        self
    }

    /// Set language preference
    #[must_use]
    pub fn with_language(mut self, lang: impl Into<String>) -> Self {
        self.language = Some(lang.into());
        self
    }

    /// Set safe search level
    #[must_use]
    pub const fn with_safe_search(mut self, level: SafeSearchLevel) -> Self {
        self.safe_search = Some(level);
        self
    }
}

/// Safe search filtering level
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SafeSearchLevel {
    /// No filtering
    Off,
    /// Moderate filtering (default)
    #[default]
    Moderate,
    /// Strict filtering
    Strict,
}

impl SafeSearchLevel {
    /// Convert to Brave API parameter
    #[must_use]
    pub const fn as_brave_param(&self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::Moderate => "moderate",
            Self::Strict => "strict",
        }
    }
}

/// Port for web search operations
///
/// This port defines the interface for searching the web and retrieving
/// results. Implementations may use various search providers like Brave
/// Search or DuckDuckGo.
#[allow(clippy::struct_field_names)] // automock generates struct with prefixes
#[cfg_attr(test, automock)]
#[async_trait]
pub trait WebSearchPort: Send + Sync {
    /// Perform a web search with the given query
    ///
    /// # Arguments
    /// * `query` - The search query string
    /// * `options` - Optional search configuration
    ///
    /// # Returns
    /// A [`WebSearchResponse`] containing search results with citations,
    /// or an error if the search fails.
    ///
    /// # Example
    /// ```ignore
    /// let response = search_port.search("Rust programming language", None).await?;
    /// for result in response.results {
    ///     println!("{}: {}", result.title, result.url);
    /// }
    /// ```
    async fn search(
        &self,
        query: &str,
        options: Option<SearchOptions>,
    ) -> Result<WebSearchResponse, ApplicationError>;

    /// Search and return formatted results ready for LLM context
    ///
    /// This is a convenience method that performs the search and formats
    /// the results for inclusion in LLM prompts, including citations.
    ///
    /// # Arguments
    /// * `query` - The search query string
    /// * `max_results` - Maximum number of results (default: 5)
    ///
    /// # Returns
    /// A formatted string containing search results with numbered citations.
    async fn search_for_llm(
        &self,
        query: &str,
        max_results: u32,
    ) -> Result<String, ApplicationError> {
        let options = SearchOptions::new().with_max_results(max_results);
        let response = self.search(query, Some(options)).await?;
        Ok(response.format_for_llm())
    }

    /// Get individual search results
    ///
    /// # Arguments
    /// * `query` - The search query string
    /// * `max_results` - Maximum number of results
    ///
    /// # Returns
    /// A vector of [`SearchResult`] items.
    async fn get_results(
        &self,
        query: &str,
        max_results: u32,
    ) -> Result<Vec<SearchResult>, ApplicationError> {
        let options = SearchOptions::new().with_max_results(max_results);
        let response = self.search(query, Some(options)).await?;
        Ok(response.results)
    }

    /// Check if the web search service is available
    ///
    /// This may perform a health check or verify API connectivity.
    async fn is_available(&self) -> bool;

    /// Get the name of the current search provider
    ///
    /// Returns a string like "brave" or "duckduckgo" indicating
    /// which backend is currently being used.
    fn provider_name(&self) -> &str;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn _assert_object_safe(_: &dyn WebSearchPort) {}

    #[test]
    fn trait_is_send_sync() {
        fn assert_send_sync<T: Send + Sync + ?Sized>() {}
        assert_send_sync::<dyn WebSearchPort>();
    }

    #[test]
    fn search_options_builder() {
        let options = SearchOptions::new()
            .with_max_results(10)
            .with_language("de")
            .with_safe_search(SafeSearchLevel::Strict);

        assert_eq!(options.max_results, Some(10));
        assert_eq!(options.language, Some("de".to_string()));
        assert_eq!(options.safe_search, Some(SafeSearchLevel::Strict));
    }

    #[test]
    fn search_options_default() {
        let options = SearchOptions::default();

        assert_eq!(options.max_results, None);
        assert_eq!(options.language, None);
        assert_eq!(options.safe_search, None);
    }

    #[test]
    fn safe_search_level_default() {
        assert_eq!(SafeSearchLevel::default(), SafeSearchLevel::Moderate);
    }

    #[test]
    fn safe_search_brave_param() {
        assert_eq!(SafeSearchLevel::Off.as_brave_param(), "off");
        assert_eq!(SafeSearchLevel::Moderate.as_brave_param(), "moderate");
        assert_eq!(SafeSearchLevel::Strict.as_brave_param(), "strict");
    }
}
