//! Reconnecting Proton client wrapper
//!
//! Provides automatic reconnection with exponential backoff for Proton Bridge connections.

use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use tokio::time::sleep;
use tracing::{instrument, warn};

use crate::{
    EmailComposition, EmailSummary, ProtonBridgeClient, ProtonClient, ProtonConfig, ProtonError,
};

/// Configuration for reconnection behavior
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReconnectConfig {
    /// Initial delay before first retry (in milliseconds)
    #[serde(default = "default_initial_delay")]
    pub initial_delay_ms: u64,

    /// Maximum delay between retries (in milliseconds)
    #[serde(default = "default_max_delay")]
    pub max_delay_ms: u64,

    /// Maximum number of retry attempts (0 = infinite)
    #[serde(default = "default_max_retries")]
    pub max_retries: u32,

    /// Multiplier for exponential backoff
    #[serde(default = "default_backoff_multiplier")]
    pub backoff_multiplier: f64,

    /// Jitter factor (0.0 - 1.0) to add randomness to delays
    #[serde(default = "default_jitter")]
    pub jitter_factor: f64,
}

const fn default_initial_delay() -> u64 {
    1000 // 1 second
}

const fn default_max_delay() -> u64 {
    60000 // 1 minute
}

const fn default_max_retries() -> u32 {
    0 // Infinite retries
}

const fn default_backoff_multiplier() -> f64 {
    2.0
}

const fn default_jitter() -> f64 {
    0.1
}

impl Default for ReconnectConfig {
    fn default() -> Self {
        Self {
            initial_delay_ms: default_initial_delay(),
            max_delay_ms: default_max_delay(),
            max_retries: default_max_retries(),
            backoff_multiplier: default_backoff_multiplier(),
            jitter_factor: default_jitter(),
        }
    }
}

impl ReconnectConfig {
    /// Calculate delay for a given attempt number (0-based)
    #[allow(clippy::cast_precision_loss)] // Acceptable for delay calculations
    pub fn calculate_delay(&self, attempt: u32) -> Duration {
        // Use saturating conversion to avoid wrapping
        let exponent = attempt.min(30); // Cap exponent to avoid overflow
        let base_delay = self.initial_delay_ms as f64
            * self
                .backoff_multiplier
                .powi(i32::try_from(exponent).unwrap_or(30));
        let capped_delay = base_delay.min(self.max_delay_ms as f64);

        // Add jitter
        let jitter_range = capped_delay * self.jitter_factor;
        let jitter = if jitter_range > 0.0 {
            // Simple deterministic jitter based on attempt number
            let jitter_offset = (f64::from(attempt) * 0.7) % 1.0;
            (jitter_offset - 0.5) * 2.0 * jitter_range
        } else {
            0.0
        };

        let final_delay = (capped_delay + jitter).max(0.0);
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        Duration::from_millis(final_delay as u64)
    }

    /// Check if more retries are allowed
    pub const fn should_retry(&self, attempt: u32) -> bool {
        self.max_retries == 0 || attempt < self.max_retries
    }
}

/// Reconnecting wrapper for Proton Bridge client
///
/// Automatically reconnects with exponential backoff when connection errors occur.
/// This is a decorator pattern implementation that wraps any `ProtonBridgeClient`.
#[derive(Debug)]
pub struct ReconnectingProtonClient {
    inner: ProtonBridgeClient,
    config: ReconnectConfig,
    consecutive_failures: AtomicU32,
}

impl ReconnectingProtonClient {
    /// Create a new reconnecting client
    pub fn new(proton_config: ProtonConfig, reconnect_config: ReconnectConfig) -> Self {
        Self {
            inner: ProtonBridgeClient::new(proton_config),
            config: reconnect_config,
            consecutive_failures: AtomicU32::new(0),
        }
    }

    /// Create with default reconnection settings
    pub fn with_defaults(proton_config: ProtonConfig) -> Self {
        Self::new(proton_config, ReconnectConfig::default())
    }

    /// Get the current number of consecutive failures
    pub fn failure_count(&self) -> u32 {
        self.consecutive_failures.load(Ordering::Relaxed)
    }

    /// Reset the failure counter (call after successful operation)
    fn reset_failures(&self) {
        self.consecutive_failures.store(0, Ordering::Relaxed);
    }

    /// Increment and get failure count
    fn increment_failures(&self) -> u32 {
        self.consecutive_failures.fetch_add(1, Ordering::Relaxed) + 1
    }

    /// Check if we should retry based on error type
    const fn is_retryable(error: &ProtonError) -> bool {
        matches!(
            error,
            ProtonError::ConnectionFailed(_) | ProtonError::BridgeUnavailable(_)
        )
    }

    /// Handle result, updating failure counts
    fn handle_result<T>(&self, result: Result<T, ProtonError>) -> Result<T, ProtonError> {
        match &result {
            Ok(_) => self.reset_failures(),
            Err(_) => {
                self.increment_failures();
            },
        }
        result
    }

    /// Wait and retry for connection errors
    async fn wait_if_connection_error(&self, error: &ProtonError, attempt: u32) -> bool {
        if !Self::is_retryable(error) {
            return false;
        }

        if !self.config.should_retry(attempt) {
            warn!(
                attempt = attempt,
                max_retries = self.config.max_retries,
                "Max retries exceeded"
            );
            return false;
        }

        let delay = self.config.calculate_delay(attempt);
        #[allow(clippy::cast_possible_truncation)]
        let delay_ms = delay.as_millis() as u64;
        warn!(
            attempt = attempt,
            delay_ms = delay_ms,
            error = %error,
            "Connection failed, waiting before retry"
        );

        sleep(delay).await;
        true
    }
}

#[async_trait]
impl ProtonClient for ReconnectingProtonClient {
    #[instrument(skip(self))]
    async fn get_inbox(&self, count: u32) -> Result<Vec<EmailSummary>, ProtonError> {
        let mut attempt = 0u32;
        loop {
            let result = self.inner.get_inbox(count).await;
            if let Err(e) = &result {
                if self.wait_if_connection_error(e, attempt).await {
                    attempt += 1;
                    continue;
                }
            }
            return self.handle_result(result);
        }
    }

    #[instrument(skip(self))]
    async fn get_mailbox(
        &self,
        mailbox: &str,
        count: u32,
    ) -> Result<Vec<EmailSummary>, ProtonError> {
        let mut attempt = 0u32;
        loop {
            let result = self.inner.get_mailbox(mailbox, count).await;
            if let Err(e) = &result {
                if self.wait_if_connection_error(e, attempt).await {
                    attempt += 1;
                    continue;
                }
            }
            return self.handle_result(result);
        }
    }

    #[instrument(skip(self))]
    async fn get_unread_count(&self) -> Result<u32, ProtonError> {
        let mut attempt = 0u32;
        loop {
            let result = self.inner.get_unread_count().await;
            if let Err(e) = &result {
                if self.wait_if_connection_error(e, attempt).await {
                    attempt += 1;
                    continue;
                }
            }
            return self.handle_result(result);
        }
    }

    #[instrument(skip(self))]
    async fn mark_read(&self, email_id: &str) -> Result<(), ProtonError> {
        let mut attempt = 0u32;
        loop {
            let result = self.inner.mark_read(email_id).await;
            if let Err(e) = &result {
                if self.wait_if_connection_error(e, attempt).await {
                    attempt += 1;
                    continue;
                }
            }
            return self.handle_result(result);
        }
    }

    #[instrument(skip(self))]
    async fn mark_unread(&self, email_id: &str) -> Result<(), ProtonError> {
        let mut attempt = 0u32;
        loop {
            let result = self.inner.mark_unread(email_id).await;
            if let Err(e) = &result {
                if self.wait_if_connection_error(e, attempt).await {
                    attempt += 1;
                    continue;
                }
            }
            return self.handle_result(result);
        }
    }

    #[instrument(skip(self))]
    async fn delete(&self, email_id: &str) -> Result<(), ProtonError> {
        let mut attempt = 0u32;
        loop {
            let result = self.inner.delete(email_id).await;
            if let Err(e) = &result {
                if self.wait_if_connection_error(e, attempt).await {
                    attempt += 1;
                    continue;
                }
            }
            return self.handle_result(result);
        }
    }

    #[instrument(skip(self, email))]
    async fn send_email(&self, email: &EmailComposition) -> Result<String, ProtonError> {
        let mut attempt = 0u32;
        loop {
            let result = self.inner.send_email(email).await;
            if let Err(e) = &result {
                if self.wait_if_connection_error(e, attempt).await {
                    attempt += 1;
                    continue;
                }
            }
            return self.handle_result(result);
        }
    }

    #[instrument(skip(self))]
    async fn check_connection(&self) -> Result<bool, ProtonError> {
        self.handle_result(self.inner.check_connection().await)
    }

    #[instrument(skip(self))]
    async fn list_mailboxes(&self) -> Result<Vec<String>, ProtonError> {
        let mut attempt = 0u32;
        loop {
            let result = self.inner.list_mailboxes().await;
            if let Err(e) = &result {
                if self.wait_if_connection_error(e, attempt).await {
                    attempt += 1;
                    continue;
                }
            }
            return self.handle_result(result);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_reconnect_config_default() {
        let config = ReconnectConfig::default();
        assert_eq!(config.initial_delay_ms, 1000);
        assert_eq!(config.max_delay_ms, 60000);
        assert_eq!(config.max_retries, 0);
        assert!((config.backoff_multiplier - 2.0).abs() < f64::EPSILON);
        assert!((config.jitter_factor - 0.1).abs() < f64::EPSILON);
    }

    #[test]
    fn test_calculate_delay_exponential() {
        let config = ReconnectConfig {
            initial_delay_ms: 1000,
            max_delay_ms: 60000,
            backoff_multiplier: 2.0,
            jitter_factor: 0.0, // No jitter for predictable test
            ..Default::default()
        };

        // Attempt 0: 1000ms
        let delay0 = config.calculate_delay(0);
        assert_eq!(delay0.as_millis(), 1000);

        // Attempt 1: 2000ms
        let delay1 = config.calculate_delay(1);
        assert_eq!(delay1.as_millis(), 2000);

        // Attempt 2: 4000ms
        let delay2 = config.calculate_delay(2);
        assert_eq!(delay2.as_millis(), 4000);

        // Attempt 3: 8000ms
        let delay3 = config.calculate_delay(3);
        assert_eq!(delay3.as_millis(), 8000);
    }

    #[test]
    fn test_calculate_delay_capped() {
        let config = ReconnectConfig {
            initial_delay_ms: 1000,
            max_delay_ms: 5000,
            backoff_multiplier: 2.0,
            jitter_factor: 0.0,
            ..Default::default()
        };

        // Should be capped at max_delay_ms
        let delay = config.calculate_delay(10);
        assert_eq!(delay.as_millis(), 5000);
    }

    #[test]
    fn test_calculate_delay_with_jitter() {
        let config = ReconnectConfig {
            initial_delay_ms: 1000,
            max_delay_ms: 60000,
            backoff_multiplier: 2.0,
            jitter_factor: 0.2,
            ..Default::default()
        };

        // With jitter, delay should be within +/- 20% of base
        let delay = config.calculate_delay(0);
        // Delay values in tests are small enough that precision loss is negligible
        #[allow(clippy::cast_precision_loss)]
        let delay_ms = delay.as_millis() as f64;

        // Base is 1000, jitter range is 200, so should be between 800-1200
        assert!((800.0..=1200.0).contains(&delay_ms));
    }

    #[test]
    fn test_is_retryable() {
        assert!(ReconnectingProtonClient::is_retryable(
            &ProtonError::ConnectionFailed("test".to_string())
        ));
        assert!(ReconnectingProtonClient::is_retryable(
            &ProtonError::BridgeUnavailable("test".to_string())
        ));
        assert!(!ReconnectingProtonClient::is_retryable(
            &ProtonError::AuthenticationFailed
        ));
        assert!(!ReconnectingProtonClient::is_retryable(
            &ProtonError::MailboxNotFound("test".to_string())
        ));
    }

    #[test]
    fn test_config_serialization() {
        let config = ReconnectConfig {
            initial_delay_ms: 500,
            max_delay_ms: 30000,
            max_retries: 5,
            backoff_multiplier: 1.5,
            jitter_factor: 0.15,
        };

        let json = serde_json::to_string(&config).unwrap();
        let parsed: ReconnectConfig = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.initial_delay_ms, 500);
        assert_eq!(parsed.max_delay_ms, 30000);
        assert_eq!(parsed.max_retries, 5);
        assert!((parsed.backoff_multiplier - 1.5).abs() < f64::EPSILON);
        assert!((parsed.jitter_factor - 0.15).abs() < f64::EPSILON);
    }

    #[test]
    fn test_should_retry_with_max() {
        let config = ReconnectConfig {
            max_retries: 3,
            ..Default::default()
        };

        assert!(config.should_retry(0));
        assert!(config.should_retry(1));
        assert!(config.should_retry(2));
        assert!(!config.should_retry(3));
        assert!(!config.should_retry(4));
    }

    #[test]
    fn test_should_retry_infinite() {
        let config = ReconnectConfig {
            max_retries: 0, // Infinite
            ..Default::default()
        };

        assert!(config.should_retry(0));
        assert!(config.should_retry(100));
        assert!(config.should_retry(u32::MAX - 1));
    }

    #[test]
    fn test_reconnect_config_clone() {
        let config = ReconnectConfig {
            initial_delay_ms: 500,
            max_delay_ms: 10000,
            max_retries: 3,
            backoff_multiplier: 1.5,
            jitter_factor: 0.2,
        };
        #[allow(clippy::redundant_clone)]
        let cloned = config.clone();
        assert_eq!(config.initial_delay_ms, cloned.initial_delay_ms);
        assert_eq!(config.max_delay_ms, cloned.max_delay_ms);
        assert_eq!(config.max_retries, cloned.max_retries);
    }

    #[test]
    fn test_reconnect_config_debug() {
        let config = ReconnectConfig::default();
        let debug = format!("{config:?}");
        assert!(debug.contains("ReconnectConfig"));
        assert!(debug.contains("initial_delay_ms"));
    }

    #[test]
    fn test_calculate_delay_high_exponent() {
        let config = ReconnectConfig {
            initial_delay_ms: 100,
            max_delay_ms: 1000,
            backoff_multiplier: 2.0,
            jitter_factor: 0.0,
            ..Default::default()
        };

        // Very high attempt should still be capped
        let delay = config.calculate_delay(100);
        assert_eq!(delay.as_millis(), 1000);
    }

    #[test]
    fn test_calculate_delay_zero_jitter() {
        let config = ReconnectConfig {
            initial_delay_ms: 1000,
            max_delay_ms: 60000,
            backoff_multiplier: 2.0,
            jitter_factor: 0.0,
            ..Default::default()
        };

        // Without jitter, should be deterministic
        let delay1 = config.calculate_delay(0);
        let delay2 = config.calculate_delay(0);
        assert_eq!(delay1, delay2);
    }

    #[test]
    fn test_is_retryable_all_error_types() {
        // Retryable errors
        assert!(ReconnectingProtonClient::is_retryable(
            &ProtonError::ConnectionFailed("network error".to_string())
        ));
        assert!(ReconnectingProtonClient::is_retryable(
            &ProtonError::BridgeUnavailable("bridge down".to_string())
        ));

        // Non-retryable errors
        assert!(!ReconnectingProtonClient::is_retryable(
            &ProtonError::AuthenticationFailed
        ));
        assert!(!ReconnectingProtonClient::is_retryable(
            &ProtonError::MailboxNotFound("inbox".to_string())
        ));
        assert!(!ReconnectingProtonClient::is_retryable(
            &ProtonError::MessageNotFound("msg-123".to_string())
        ));
        assert!(!ReconnectingProtonClient::is_retryable(
            &ProtonError::SmtpError("invalid recipient".to_string())
        ));
    }

    #[test]
    fn test_config_deserialization_partial() {
        // Only some fields specified
        let json = r#"{"initial_delay_ms":2000}"#;
        let config: ReconnectConfig = serde_json::from_str(json).unwrap();

        assert_eq!(config.initial_delay_ms, 2000);
        // Defaults for the rest
        assert_eq!(config.max_delay_ms, 60000);
        assert_eq!(config.max_retries, 0);
    }

    #[test]
    fn test_should_retry_boundary() {
        let config = ReconnectConfig {
            max_retries: 1,
            ..Default::default()
        };

        // Only attempt 0 should be allowed
        assert!(config.should_retry(0));
        assert!(!config.should_retry(1));
    }

    #[test]
    fn test_calculate_delay_different_multipliers() {
        let config_2x = ReconnectConfig {
            initial_delay_ms: 100,
            max_delay_ms: 10000,
            backoff_multiplier: 2.0,
            jitter_factor: 0.0,
            ..Default::default()
        };

        let config_3x = ReconnectConfig {
            initial_delay_ms: 100,
            max_delay_ms: 10000,
            backoff_multiplier: 3.0,
            jitter_factor: 0.0,
            ..Default::default()
        };

        // At attempt 2: 2x = 400ms, 3x = 900ms
        let delay_2x = config_2x.calculate_delay(2);
        let delay_3x = config_3x.calculate_delay(2);

        assert_eq!(delay_2x.as_millis(), 400);
        assert_eq!(delay_3x.as_millis(), 900);
    }
}
