//! Web search domain entities

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
}

impl SearchResult {
    /// Create a new search result
    #[must_use]
    pub const fn new(
        title: String,
        url: String,
        snippet: String,
        source: String,
        position: u32,
    ) -> Self {
        Self {
            title,
            url,
            snippet,
            source,
            position,
        }
    }

    /// Format as a citation reference for LLM context
    ///
    /// Returns a string like: "\[1\] Title - source.com: Snippet..."
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
    /// Returns a string like: "\[1\] source.com/path"
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
}

impl WebSearchResponse {
    /// Create a new search response
    #[must_use]
    pub fn new(query: String, results: Vec<SearchResult>, provider: String) -> Self {
        Self {
            query,
            results,
            timestamp: Utc::now(),
            provider,
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
            "www.rust-lang.org".to_string(),
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
            "example.com".to_string(),
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
        let response =
            WebSearchResponse::new("rust programming".to_string(), results, "brave".to_string());
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
                "doc.rust-lang.org".to_string(),
                2,
            ),
        ];
        let response = WebSearchResponse::new("rust".to_string(), results, "brave".to_string());
        let formatted = response.format_for_llm();

        assert!(formatted.contains("Web search results"));
        assert!(formatted.contains("[1]"));
        assert!(formatted.contains("[2]"));
        assert!(formatted.contains("Sources:"));
    }

    #[test]
    fn test_format_for_llm_empty_results() {
        let response = WebSearchResponse::new("test".to_string(), vec![], "brave".to_string());
        let formatted = response.format_for_llm();
        assert!(formatted.contains("No web search results found"));
    }

    #[test]
    fn test_search_response_has_results() {
        let empty_response =
            WebSearchResponse::new("test".to_string(), vec![], "brave".to_string());
        assert!(!empty_response.has_results());

        let response = WebSearchResponse::new(
            "test".to_string(),
            vec![sample_result()],
            "brave".to_string(),
        );
        assert!(response.has_results());
    }
}
