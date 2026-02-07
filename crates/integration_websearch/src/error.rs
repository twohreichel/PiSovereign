//! Web search error types

use thiserror::Error;

/// Errors that can occur during web search operations
#[derive(Debug, Error)]
pub enum WebSearchError {
    /// Connection to the search service failed
    #[error("Connection failed: {0}")]
    ConnectionFailed(String),

    /// HTTP request to search service failed
    #[error("Request failed: {0}")]
    RequestFailed(String),

    /// Failed to parse response from search service
    #[error("Parse error: {0}")]
    ParseError(String),

    /// Search query is invalid or empty
    #[error("Invalid query: {0}")]
    InvalidQuery(String),

    /// API key is missing or invalid
    #[error("Authentication failed: {0}")]
    AuthenticationFailed(String),

    /// Rate limit exceeded
    #[error("Rate limit exceeded, retry after {retry_after_secs:?} seconds")]
    RateLimitExceeded {
        /// Seconds to wait before retrying (if provided by API)
        retry_after_secs: Option<u64>,
    },

    /// Search returned no results
    #[error("No results found for query: {query}")]
    NoResults {
        /// The search query that returned no results
        query: String,
    },

    /// Service is temporarily unavailable
    #[error("Service unavailable: {0}")]
    ServiceUnavailable(String),

    /// Configuration error
    #[error("Configuration error: {0}")]
    ConfigurationError(String),

    /// Request timeout
    #[error("Request timed out after {timeout_secs} seconds")]
    Timeout {
        /// The timeout duration in seconds
        timeout_secs: u64,
    },
}

impl WebSearchError {
    /// Returns true if this error is retryable
    #[must_use]
    pub const fn is_retryable(&self) -> bool {
        matches!(
            self,
            Self::ConnectionFailed(_)
                | Self::RequestFailed(_)
                | Self::ServiceUnavailable(_)
                | Self::Timeout { .. }
                | Self::RateLimitExceeded { .. }
        )
    }

    /// Returns true if fallback should be attempted
    #[must_use]
    pub const fn should_fallback(&self) -> bool {
        matches!(
            self,
            Self::ConnectionFailed(_)
                | Self::RequestFailed(_)
                | Self::ServiceUnavailable(_)
                | Self::Timeout { .. }
                | Self::RateLimitExceeded { .. }
                | Self::NoResults { .. }
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_retryable_errors() {
        assert!(WebSearchError::ConnectionFailed("test".to_string()).is_retryable());
        assert!(WebSearchError::RequestFailed("test".to_string()).is_retryable());
        assert!(WebSearchError::ServiceUnavailable("test".to_string()).is_retryable());
        assert!(WebSearchError::Timeout { timeout_secs: 30 }.is_retryable());
        assert!(
            WebSearchError::RateLimitExceeded {
                retry_after_secs: Some(60)
            }
            .is_retryable()
        );

        assert!(!WebSearchError::InvalidQuery("test".to_string()).is_retryable());
        assert!(!WebSearchError::AuthenticationFailed("test".to_string()).is_retryable());
        assert!(!WebSearchError::ParseError("test".to_string()).is_retryable());
    }

    #[test]
    fn test_should_fallback() {
        assert!(WebSearchError::ConnectionFailed("test".to_string()).should_fallback());
        assert!(
            WebSearchError::NoResults {
                query: "test".to_string()
            }
            .should_fallback()
        );

        assert!(!WebSearchError::InvalidQuery("test".to_string()).should_fallback());
        assert!(!WebSearchError::AuthenticationFailed("test".to_string()).should_fallback());
    }

    #[test]
    fn test_error_display() {
        let err = WebSearchError::RateLimitExceeded {
            retry_after_secs: Some(60),
        };
        assert!(err.to_string().contains("60"));

        let err = WebSearchError::NoResults {
            query: "test query".to_string(),
        };
        assert!(err.to_string().contains("test query"));
    }
}
