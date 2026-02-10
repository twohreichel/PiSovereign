//! DuckDuckGo Instant Answer API client
//!
//! Client for the DuckDuckGo Instant Answer API (<https://api.duckduckgo.com/>).
//! Used as a fallback when Brave Search is unavailable.
//!
//! Note: DuckDuckGo's Instant Answer API returns structured data for specific
//! query types (definitions, calculations, etc.) but not general web search results.
//! For general searches, we extract what information is available from the API response.

use async_trait::async_trait;
use reqwest::Client;
use std::time::{Duration, Instant};
use tracing::{debug, instrument, warn};

use crate::{
    WebSearchResponse, config::WebSearchConfig, error::WebSearchError, models::SearchResult,
    provider::SearchProvider,
};

/// DuckDuckGo API response structures
#[allow(dead_code)]
mod api {
    use serde::Deserialize;

    #[derive(Debug, Deserialize)]
    #[serde(rename_all = "PascalCase")]
    pub struct DuckDuckGoResponse {
        /// Abstract text (summary)
        #[serde(default)]
        pub abstract_text: String,

        /// Abstract source (e.g., "Wikipedia")
        #[serde(default)]
        pub abstract_source: String,

        /// Abstract URL
        #[serde(default, rename = "AbstractURL")]
        pub abstract_url: String,

        /// Heading/title
        #[serde(default)]
        pub heading: String,

        /// Answer (for instant answers)
        #[serde(default)]
        pub answer: String,

        /// Answer type
        #[serde(default)]
        pub answer_type: String,

        /// Definition
        #[serde(default)]
        pub definition: String,

        /// Definition source
        #[serde(default)]
        pub definition_source: String,

        /// Definition URL
        #[serde(default, rename = "DefinitionURL")]
        pub definition_url: String,

        /// Related topics
        #[serde(default)]
        pub related_topics: Vec<RelatedTopic>,

        /// Results (external links)
        #[serde(default)]
        pub results: Vec<ExternalResult>,

        /// Image URL
        #[serde(default)]
        pub image: String,

        /// Type of response (A = article, D = disambiguation, etc.)
        #[serde(default, rename = "Type")]
        pub response_type: String,
    }

    #[derive(Debug, Deserialize)]
    #[serde(rename_all = "PascalCase")]
    pub struct RelatedTopic {
        /// Topic text
        #[serde(default)]
        pub text: String,

        /// First URL (link to more info)
        #[serde(default, rename = "FirstURL")]
        pub first_url: String,

        /// Icon information
        pub icon: Option<Icon>,

        /// Result (for nested results)
        #[serde(default)]
        pub result: String,
    }

    #[derive(Debug, Deserialize)]
    #[serde(rename_all = "PascalCase")]
    pub struct ExternalResult {
        /// First URL
        #[serde(default, rename = "FirstURL")]
        pub first_url: String,

        /// Result text (HTML)
        #[serde(default)]
        pub result: String,

        /// Text content
        #[serde(default)]
        pub text: String,
    }

    #[derive(Debug, Deserialize)]
    #[serde(rename_all = "PascalCase")]
    pub struct Icon {
        /// Icon URL
        #[serde(default, rename = "URL")]
        pub url: String,
    }
}

/// DuckDuckGo Instant Answer API client
#[derive(Debug)]
pub struct DuckDuckGoClient {
    client: Client,
    base_url: String,
}

impl DuckDuckGoClient {
    /// Create a new DuckDuckGo client
    ///
    /// # Errors
    ///
    /// Returns an error if the HTTP client cannot be created.
    pub fn new(config: &WebSearchConfig) -> Result<Self, WebSearchError> {
        let client = Client::builder()
            .timeout(Duration::from_secs(config.timeout_secs))
            .user_agent("PiSovereign/1.0 (Web Search Integration)")
            .build()
            .map_err(|e| WebSearchError::ConnectionFailed(e.to_string()))?;

        Ok(Self {
            client,
            base_url: config.duckduckgo_base_url.clone(),
        })
    }

    /// Build the API URL
    fn build_url(&self, query: &str) -> String {
        let encoded_query = urlencoding::encode(query);
        format!(
            "{}/?q={}&format=json&no_html=1&skip_disambig=0",
            self.base_url, encoded_query
        )
    }

    /// Convert API response to search results
    fn convert_response(response: api::DuckDuckGoResponse, query: &str) -> Vec<SearchResult> {
        let mut results = Vec::new();
        let mut position = 1u32;

        // Add abstract as first result if available
        if !response.abstract_text.is_empty() && !response.abstract_url.is_empty() {
            results.push(SearchResult::new(
                if response.heading.is_empty() {
                    query.to_string()
                } else {
                    response.heading.clone()
                },
                response.abstract_url.clone(),
                response.abstract_text.clone(),
                position,
            ));
            position += 1;
        }

        // Add definition if available
        if !response.definition.is_empty() && !response.definition_url.is_empty() {
            results.push(SearchResult::new(
                format!("Definition: {}", response.heading),
                response.definition_url.clone(),
                response.definition.clone(),
                position,
            ));
            position += 1;
        }

        // Add instant answer if available
        if !response.answer.is_empty() {
            results.push(SearchResult::new(
                format!("Answer: {}", response.heading),
                format!("https://duckduckgo.com/?q={}", urlencoding::encode(query)),
                response.answer.clone(),
                position,
            ));
            position += 1;
        }

        // Add related topics (filtered to those with URLs)
        for topic in response.related_topics {
            if !topic.first_url.is_empty() && !topic.text.is_empty() {
                // Extract title from text (before " - ")
                let title = topic
                    .text
                    .split(" - ")
                    .next()
                    .unwrap_or(&topic.text)
                    .to_string();

                results.push(SearchResult::new(
                    title,
                    topic.first_url,
                    topic.text,
                    position,
                ));
                position += 1;
            }
        }

        // Add external results
        for ext_result in response.results {
            if !ext_result.first_url.is_empty() {
                results.push(SearchResult::new(
                    if ext_result.text.is_empty() {
                        "External Result".to_string()
                    } else {
                        ext_result.text.clone()
                    },
                    ext_result.first_url,
                    ext_result.text,
                    position,
                ));
                position += 1;
            }
        }

        results
    }
}

#[async_trait]
impl SearchProvider for DuckDuckGoClient {
    #[instrument(skip(self), fields(provider = "duckduckgo"))]
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

        let url = self.build_url(query);
        let start = Instant::now();

        debug!(url = %url, "Sending DuckDuckGo request");

        let response = self.client.get(&url).send().await.map_err(|e| {
            if e.is_timeout() {
                WebSearchError::Timeout { timeout_secs: 30 }
            } else if e.is_connect() {
                WebSearchError::ConnectionFailed(e.to_string())
            } else {
                WebSearchError::RequestFailed(e.to_string())
            }
        })?;

        let status = response.status();
        debug!(status = %status, "Received DuckDuckGo response");

        if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
            return Err(WebSearchError::RateLimitExceeded {
                retry_after_secs: None,
            });
        }

        if !status.is_success() {
            let error_text = response.text().await.unwrap_or_default();
            return Err(WebSearchError::RequestFailed(format!(
                "HTTP {status}: {error_text}"
            )));
        }

        let api_response: api::DuckDuckGoResponse = response
            .json()
            .await
            .map_err(|e| WebSearchError::ParseError(e.to_string()))?;

        let mut results = Self::convert_response(api_response, query);
        results.truncate(max_results);

        let elapsed = start.elapsed();

        // DuckDuckGo Instant Answer API may return no results for many queries
        // This is expected behavior, not an error
        let mut response = WebSearchResponse::new(query.to_string(), results, "duckduckgo");
        #[allow(clippy::cast_possible_truncation)]
        {
            response.search_time_ms = Some(elapsed.as_millis() as u64);
        }

        debug!(
            results = response.results.len(),
            time_ms = elapsed.as_millis(),
            "DuckDuckGo search completed"
        );

        Ok(response)
    }

    async fn is_healthy(&self) -> bool {
        // DuckDuckGo API is stateless and doesn't require auth
        // Just check if we can reach it
        match self.client.get(&self.base_url).send().await {
            Ok(resp) => resp.status().is_success() || resp.status().is_redirection(),
            Err(e) => {
                warn!(error = %e, "DuckDuckGo health check failed");
                false
            },
        }
    }

    fn provider_name(&self) -> &'static str {
        "duckduckgo"
    }
}

// URL encoding helper (same as brave module)
mod urlencoding {
    pub fn encode(input: &str) -> String {
        let mut result = String::with_capacity(input.len() * 3);
        for c in input.chars() {
            match c {
                'a'..='z' | 'A'..='Z' | '0'..='9' | '-' | '_' | '.' | '~' => result.push(c),
                ' ' => result.push('+'),
                _ => {
                    for b in c.to_string().as_bytes() {
                        result.push_str(&format!("%{b:02X}"));
                    }
                },
            }
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_url() {
        let config = WebSearchConfig::default();
        let client = DuckDuckGoClient::new(&config).unwrap();
        let url = client.build_url("rust programming");

        assert!(url.contains("q=rust+programming"));
        assert!(url.contains("format=json"));
        assert!(url.contains("no_html=1"));
    }

    #[test]
    fn test_convert_response_with_abstract() {
        let response = api::DuckDuckGoResponse {
            abstract_text: "Rust is a systems programming language.".to_string(),
            abstract_source: "Wikipedia".to_string(),
            abstract_url: "https://en.wikipedia.org/wiki/Rust".to_string(),
            heading: "Rust (programming language)".to_string(),
            answer: String::new(),
            answer_type: String::new(),
            definition: String::new(),
            definition_source: String::new(),
            definition_url: String::new(),
            related_topics: vec![],
            results: vec![],
            image: String::new(),
            response_type: "A".to_string(),
        };

        let results = DuckDuckGoClient::convert_response(response, "rust");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].title, "Rust (programming language)");
        assert!(results[0].url.contains("wikipedia"));
    }

    #[test]
    fn test_convert_response_with_related_topics() {
        let response = api::DuckDuckGoResponse {
            abstract_text: String::new(),
            abstract_source: String::new(),
            abstract_url: String::new(),
            heading: String::new(),
            answer: String::new(),
            answer_type: String::new(),
            definition: String::new(),
            definition_source: String::new(),
            definition_url: String::new(),
            related_topics: vec![api::RelatedTopic {
                text: "Rust Programming - A modern systems language".to_string(),
                first_url: "https://rust-lang.org".to_string(),
                icon: None,
                result: String::new(),
            }],
            results: vec![],
            image: String::new(),
            response_type: "D".to_string(),
        };

        let results = DuckDuckGoClient::convert_response(response, "rust");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].title, "Rust Programming");
    }

    #[test]
    fn test_convert_empty_response() {
        let response = api::DuckDuckGoResponse {
            abstract_text: String::new(),
            abstract_source: String::new(),
            abstract_url: String::new(),
            heading: String::new(),
            answer: String::new(),
            answer_type: String::new(),
            definition: String::new(),
            definition_source: String::new(),
            definition_url: String::new(),
            related_topics: vec![],
            results: vec![],
            image: String::new(),
            response_type: String::new(),
        };

        let results = DuckDuckGoClient::convert_response(response, "rust");
        assert!(results.is_empty());
    }

    #[test]
    fn test_convert_response_with_definition() {
        let response = api::DuckDuckGoResponse {
            abstract_text: String::new(),
            abstract_source: String::new(),
            abstract_url: String::new(),
            heading: "Test Word".to_string(),
            answer: String::new(),
            answer_type: String::new(),
            definition: "The meaning of test word".to_string(),
            definition_source: "Dictionary".to_string(),
            definition_url: "https://dictionary.com/test".to_string(),
            related_topics: vec![],
            results: vec![],
            image: String::new(),
            response_type: "D".to_string(),
        };

        let results = DuckDuckGoClient::convert_response(response, "test word");
        assert_eq!(results.len(), 1);
        assert!(results[0].title.contains("Definition"));
        assert!(results[0].snippet.contains("meaning"));
    }

    #[test]
    fn test_convert_response_with_answer() {
        let response = api::DuckDuckGoResponse {
            abstract_text: String::new(),
            abstract_source: String::new(),
            abstract_url: String::new(),
            heading: "Calculator".to_string(),
            answer: "42".to_string(),
            answer_type: "calc".to_string(),
            definition: String::new(),
            definition_source: String::new(),
            definition_url: String::new(),
            related_topics: vec![],
            results: vec![],
            image: String::new(),
            response_type: "C".to_string(),
        };

        let results = DuckDuckGoClient::convert_response(response, "6*7");
        assert_eq!(results.len(), 1);
        assert!(results[0].title.contains("Answer"));
        assert!(results[0].snippet.contains("42"));
    }

    #[test]
    fn test_convert_response_with_external_results() {
        let response = api::DuckDuckGoResponse {
            abstract_text: String::new(),
            abstract_source: String::new(),
            abstract_url: String::new(),
            heading: String::new(),
            answer: String::new(),
            answer_type: String::new(),
            definition: String::new(),
            definition_source: String::new(),
            definition_url: String::new(),
            related_topics: vec![],
            results: vec![
                api::ExternalResult {
                    first_url: "https://example.com/result1".to_string(),
                    result: "Result 1 HTML".to_string(),
                    text: "Result 1 Text".to_string(),
                },
                api::ExternalResult {
                    first_url: "https://example.com/result2".to_string(),
                    result: "Result 2 HTML".to_string(),
                    text: String::new(), // Empty text
                },
            ],
            image: String::new(),
            response_type: "E".to_string(),
        };

        let results = DuckDuckGoClient::convert_response(response, "test");
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].title, "Result 1 Text");
        assert_eq!(results[1].title, "External Result"); // Fallback title
    }

    #[test]
    fn test_convert_response_all_components() {
        // Test with multiple types of results
        let response = api::DuckDuckGoResponse {
            abstract_text: "Abstract text here".to_string(),
            abstract_source: "Source".to_string(),
            abstract_url: "https://source.com".to_string(),
            heading: "Heading".to_string(),
            answer: "Direct answer".to_string(),
            answer_type: "calc".to_string(),
            definition: "Definition text".to_string(),
            definition_source: "Dictionary".to_string(),
            definition_url: "https://dict.com".to_string(),
            related_topics: vec![api::RelatedTopic {
                text: "Related - Topic description".to_string(),
                first_url: "https://related.com".to_string(),
                icon: None,
                result: String::new(),
            }],
            results: vec![api::ExternalResult {
                first_url: "https://external.com".to_string(),
                result: String::new(),
                text: "External".to_string(),
            }],
            image: String::new(),
            response_type: "A".to_string(),
        };

        let results = DuckDuckGoClient::convert_response(response, "query");
        // Should have: abstract, definition, answer, related topic, external result
        assert_eq!(results.len(), 5);
        assert_eq!(results[0].position, 1);
        assert_eq!(results[1].position, 2);
        assert_eq!(results[2].position, 3);
        assert_eq!(results[3].position, 4);
        assert_eq!(results[4].position, 5);
    }

    #[test]
    fn test_convert_response_empty_heading_uses_query() {
        let response = api::DuckDuckGoResponse {
            abstract_text: "Some abstract".to_string(),
            abstract_source: "Source".to_string(),
            abstract_url: "https://url.com".to_string(),
            heading: String::new(), // Empty heading
            answer: String::new(),
            answer_type: String::new(),
            definition: String::new(),
            definition_source: String::new(),
            definition_url: String::new(),
            related_topics: vec![],
            results: vec![],
            image: String::new(),
            response_type: "A".to_string(),
        };

        let results = DuckDuckGoClient::convert_response(response, "my_query");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].title, "my_query"); // Uses query as fallback
    }

    #[test]
    fn test_urlencoding_simple() {
        let encoded = urlencoding::encode("hello world");
        assert_eq!(encoded, "hello+world");
    }

    #[test]
    fn test_urlencoding_special_chars() {
        let encoded = urlencoding::encode("test@example.com");
        assert!(encoded.contains("%40"));
    }

    #[test]
    fn test_urlencoding_preserves_allowed() {
        let encoded = urlencoding::encode("hello-world_test.file~name");
        assert_eq!(encoded, "hello-world_test.file~name");
    }

    #[test]
    fn test_urlencoding_unicode() {
        let encoded = urlencoding::encode("Ü");
        // Unicode characters get percent-encoded
        assert!(encoded.starts_with('%'));
    }

    #[test]
    fn test_client_new() {
        let config = WebSearchConfig::default();
        let client = DuckDuckGoClient::new(&config);
        assert!(client.is_ok());
    }

    #[test]
    fn test_client_provider_name() {
        let config = WebSearchConfig::default();
        let client = DuckDuckGoClient::new(&config).unwrap();
        assert_eq!(client.provider_name(), "duckduckgo");
    }

    #[test]
    fn test_build_url_special_chars() {
        let config = WebSearchConfig::default();
        let client = DuckDuckGoClient::new(&config).unwrap();
        let url = client.build_url("hello world");

        assert!(url.contains("q=hello+world"));
    }

    #[test]
    fn test_related_topic_without_url_skipped() {
        let response = api::DuckDuckGoResponse {
            abstract_text: String::new(),
            abstract_source: String::new(),
            abstract_url: String::new(),
            heading: String::new(),
            answer: String::new(),
            answer_type: String::new(),
            definition: String::new(),
            definition_source: String::new(),
            definition_url: String::new(),
            related_topics: vec![api::RelatedTopic {
                text: "Topic without URL".to_string(),
                first_url: String::new(), // Empty URL
                icon: None,
                result: String::new(),
            }],
            results: vec![],
            image: String::new(),
            response_type: "D".to_string(),
        };

        let results = DuckDuckGoClient::convert_response(response, "test");
        assert!(results.is_empty()); // Should be skipped
    }
}
