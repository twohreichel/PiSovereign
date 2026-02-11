//! Transit error types

use thiserror::Error;

/// Errors that can occur during transit operations
#[derive(Debug, Error)]
pub enum TransitError {
    /// Connection to the transit service failed
    #[error("Connection failed: {0}")]
    ConnectionFailed(String),

    /// HTTP request to transit service failed
    #[error("Request failed: {0}")]
    RequestFailed(String),

    /// Failed to parse response from transit service
    #[error("Parse error: {0}")]
    ParseError(String),

    /// Rate limit exceeded
    #[error("Rate limit exceeded, retry after {retry_after_secs:?} seconds")]
    RateLimitExceeded {
        /// Seconds to wait before retrying (if provided by API)
        retry_after_secs: Option<u64>,
    },

    /// No routes found between origin and destination
    #[error("No routes found from {from} to {to}")]
    NoRoutesFound {
        /// Origin description
        from: String,
        /// Destination description
        to: String,
    },

    /// Invalid location provided
    #[error("Invalid location: {0}")]
    InvalidLocation(String),

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

impl TransitError {
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_retryable_errors() {
        assert!(TransitError::ConnectionFailed("test".to_string()).is_retryable());
        assert!(TransitError::RequestFailed("test".to_string()).is_retryable());
        assert!(TransitError::ServiceUnavailable("test".to_string()).is_retryable());
        assert!(TransitError::Timeout { timeout_secs: 30 }.is_retryable());
        assert!(
            TransitError::RateLimitExceeded {
                retry_after_secs: Some(60)
            }
            .is_retryable()
        );
    }

    #[test]
    fn test_non_retryable_errors() {
        assert!(!TransitError::InvalidLocation("test".to_string()).is_retryable());
        assert!(!TransitError::ParseError("test".to_string()).is_retryable());
        assert!(!TransitError::ConfigurationError("test".to_string()).is_retryable());
        assert!(
            !TransitError::NoRoutesFound {
                from: "A".to_string(),
                to: "B".to_string(),
            }
            .is_retryable()
        );
    }

    #[test]
    fn test_error_display() {
        let err = TransitError::NoRoutesFound {
            from: "Berlin Hbf".to_string(),
            to: "München Hbf".to_string(),
        };
        assert!(err.to_string().contains("Berlin Hbf"));
        assert!(err.to_string().contains("München Hbf"));

        let err = TransitError::RateLimitExceeded {
            retry_after_secs: Some(60),
        };
        assert!(err.to_string().contains("60"));

        let err = TransitError::Timeout { timeout_secs: 10 };
        assert!(err.to_string().contains("10"));
    }
}
