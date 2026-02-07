//! Web search data models

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// A single search result from a web search
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SearchResult {
    /// Title of the search result
    pub title: String,

    /// URL of the search result
    pub url: String,

    /// Short snippet/description of the content
    pub snippet: String,

    /// Source domain (e.g., "wikipedia.org")
    pub source: String,

    /// Position in search results (1-indexed)
    pub position: u32,

    /// Publication date if available
    #[serde(skip_serializing_if = "Option::is_none")]
    pub published_date: Option<DateTime<Utc>>,

    /// Thumbnail URL if available
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thumbnail_url: Option<String>,
}

impl SearchResult {
    /// Create a new search result
    #[must_use]
    pub fn new(title: String, url: String, snippet: String, position: u32) -> Self {
        let source = Self::extract_domain(&url);
        Self {
            title,
            url,
            snippet,
            source,
            position,
            published_date: None,
            thumbnail_url: None,
        }
    }

    /// Extract domain from URL
    fn extract_domain(url: &str) -> String {
        url::Url::parse(url)
            .ok()
            .and_then(|u| u.host_str().map(ToString::to_string))
            .unwrap_or_else(|| "unknown".to_string())
    }

    /// Format as a citation reference for LLM context
    ///
    /// Returns a string like: "[1] Title - source.com: Snippet..."
    #[must_use]
    pub fn format_citation(&self) -> String {
        let truncated_snippet = if self.snippet.len() > 200 {
            format!("{}...", &self.snippet[..197])
        } else {
            self.snippet.clone()
        };
        format!(
            "[{}] {} - {}: {}",
            self.position, self.title, self.source, truncated_snippet
        )
    }

    /// Format as a footnote reference
    ///
    /// Returns a string like: "[1] source.com/path"
    #[must_use]
    pub fn format_footnote(&self) -> String {
        format!("[{}] {}", self.position, self.url)
    }
}

/// Response from a web search operation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebSearchResponse {
    /// Original search query
    pub query: String,

    /// List of search results
    pub results: Vec<SearchResult>,

    /// Timestamp of the search
    pub timestamp: DateTime<Utc>,

    /// Search provider used (e.g., "brave", "duckduckgo")
    pub provider: String,

    /// Total number of results found (may be more than returned)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_results: Option<u64>,

    /// Time taken for the search in milliseconds
    #[serde(skip_serializing_if = "Option::is_none")]
    pub search_time_ms: Option<u64>,
}

impl WebSearchResponse {
    /// Create a new search response
    #[must_use]
    pub fn new(query: String, results: Vec<SearchResult>, provider: &str) -> Self {
        Self {
            query,
            results,
            timestamp: Utc::now(),
            provider: provider.to_string(),
            total_results: None,
            search_time_ms: None,
        }
    }

    /// Format all results as citation context for LLM
    ///
    /// Returns formatted results suitable for inclusion in LLM prompts.
    #[must_use]
    pub fn format_for_llm(&self) -> String {
        if self.results.is_empty() {
            return format!("No web search results found for: {}", self.query);
        }

        let mut output = format!(
            "Web search results for \"{}\" ({} results):\n\n",
            self.query,
            self.results.len()
        );

        for result in &self.results {
            output.push_str(&result.format_citation());
            output.push_str("\n\n");
        }

        output.push_str("Sources:\n");
        for result in &self.results {
            output.push_str(&result.format_footnote());
            output.push('\n');
        }

        output
    }

    /// Get sources formatted as footnotes
    #[must_use]
    pub fn format_footnotes(&self) -> String {
        self.results
            .iter()
            .map(SearchResult::format_footnote)
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// Check if the response has any results
    #[must_use]
    pub fn has_results(&self) -> bool {
        !self.results.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_result() -> SearchResult {
        SearchResult::new(
            "Rust Programming Language".to_string(),
            "https://www.rust-lang.org/learn".to_string(),
            "Rust is a systems programming language focused on safety and performance.".to_string(),
            1,
        )
    }

    #[test]
    fn test_search_result_creation() {
        let result = sample_result();
        assert_eq!(result.title, "Rust Programming Language");
        assert_eq!(result.source, "www.rust-lang.org");
        assert_eq!(result.position, 1);
    }

    #[test]
    fn test_domain_extraction() {
        let result = SearchResult::new(
            "Test".to_string(),
            "https://en.wikipedia.org/wiki/Rust".to_string(),
            "Test snippet".to_string(),
            1,
        );
        assert_eq!(result.source, "en.wikipedia.org");
    }

    #[test]
    fn test_domain_extraction_invalid_url() {
        let result = SearchResult::new(
            "Test".to_string(),
            "not-a-valid-url".to_string(),
            "Test snippet".to_string(),
            1,
        );
        assert_eq!(result.source, "unknown");
    }

    #[test]
    fn test_format_citation() {
        let result = sample_result();
        let citation = result.format_citation();
        assert!(citation.starts_with("[1]"));
        assert!(citation.contains("Rust Programming Language"));
        assert!(citation.contains("www.rust-lang.org"));
    }

    #[test]
    fn test_format_citation_truncates_long_snippets() {
        let long_snippet = "a".repeat(300);
        let result = SearchResult::new(
            "Test".to_string(),
            "https://example.com".to_string(),
            long_snippet,
            1,
        );
        let citation = result.format_citation();
        assert!(citation.ends_with("..."));
        assert!(citation.len() < 350);
    }

    #[test]
    fn test_format_footnote() {
        let result = sample_result();
        let footnote = result.format_footnote();
        assert_eq!(footnote, "[1] https://www.rust-lang.org/learn");
    }

    #[test]
    fn test_search_response_creation() {
        let results = vec![sample_result()];
        let response = WebSearchResponse::new("rust programming".to_string(), results, "brave");
        assert_eq!(response.query, "rust programming");
        assert_eq!(response.provider, "brave");
        assert!(response.has_results());
    }

    #[test]
    fn test_format_for_llm() {
        let results = vec![
            sample_result(),
            SearchResult::new(
                "Rust Book".to_string(),
                "https://doc.rust-lang.org/book/".to_string(),
                "The Rust Programming Language book.".to_string(),
                2,
            ),
        ];
        let response = WebSearchResponse::new("rust".to_string(), results, "brave");
        let formatted = response.format_for_llm();

        assert!(formatted.contains("Web search results"));
        assert!(formatted.contains("[1]"));
        assert!(formatted.contains("[2]"));
        assert!(formatted.contains("Sources:"));
    }

    #[test]
    fn test_format_for_llm_empty_results() {
        let response = WebSearchResponse::new("test".to_string(), vec![], "brave");
        let formatted = response.format_for_llm();
        assert!(formatted.contains("No web search results found"));
    }

    #[test]
    fn test_search_response_has_results() {
        let empty_response = WebSearchResponse::new("test".to_string(), vec![], "brave");
        assert!(!empty_response.has_results());

        let response = WebSearchResponse::new("test".to_string(), vec![sample_result()], "brave");
        assert!(response.has_results());
    }
}
