//! Generic retry logic with exponential backoff
//!
//! Provides a configurable retry mechanism for fallible operations,
//! with exponential backoff and jitter to prevent thundering herd.
//!
//! # Example
//!
//! ```rust,ignore
//! use infrastructure::retry::{RetryConfig, with_retry};
//!
//! let config = RetryConfig::default();
//! let result = with_retry(&config, || async {
//!     external_service.call().await
//! }).await;
//! ```

use rand::Rng;
use serde::{Deserialize, Serialize};
use std::future::Future;
use std::time::Duration;
use tracing::{debug, warn};

/// Configuration for retry behavior with exponential backoff
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetryConfig {
    /// Initial delay before first retry in milliseconds (default: 100ms)
    #[serde(default = "default_initial_delay")]
    pub initial_delay_ms: u64,

    /// Maximum delay between retries in milliseconds (default: 10000ms = 10s)
    #[serde(default = "default_max_delay")]
    pub max_delay_ms: u64,

    /// Multiplier for exponential backoff (default: 2.0)
    #[serde(default = "default_multiplier")]
    pub multiplier: f64,

    /// Maximum number of retry attempts (default: 3)
    #[serde(default = "default_max_retries")]
    pub max_retries: u32,

    /// Whether to add jitter to prevent thundering herd (default: true)
    #[serde(default = "default_true")]
    pub jitter_enabled: bool,

    /// Maximum jitter factor (0.0 to 1.0, default: 0.1 = 10%)
    #[serde(default = "default_jitter_factor")]
    pub jitter_factor: f64,
}

const fn default_initial_delay() -> u64 {
    100
}

const fn default_max_delay() -> u64 {
    10_000
}

const fn default_multiplier() -> f64 {
    2.0
}

const fn default_max_retries() -> u32 {
    3
}

const fn default_true() -> bool {
    true
}

const fn default_jitter_factor() -> f64 {
    0.1
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            initial_delay_ms: default_initial_delay(),
            max_delay_ms: default_max_delay(),
            multiplier: default_multiplier(),
            max_retries: default_max_retries(),
            jitter_enabled: default_true(),
            jitter_factor: default_jitter_factor(),
        }
    }
}

impl RetryConfig {
    /// Create a new retry configuration with custom parameters
    #[must_use]
    pub const fn new(
        initial_delay_ms: u64,
        max_delay_ms: u64,
        multiplier: f64,
        max_retries: u32,
    ) -> Self {
        Self {
            initial_delay_ms,
            max_delay_ms,
            multiplier,
            max_retries,
            jitter_enabled: true,
            jitter_factor: 0.1,
        }
    }

    /// Create a configuration optimized for fast retries (low latency operations)
    #[must_use]
    pub const fn fast() -> Self {
        Self {
            initial_delay_ms: 50,
            max_delay_ms: 1000,
            multiplier: 2.0,
            max_retries: 3,
            jitter_enabled: true,
            jitter_factor: 0.1,
        }
    }

    /// Create a configuration for slow/expensive operations
    #[must_use]
    pub const fn slow() -> Self {
        Self {
            initial_delay_ms: 500,
            max_delay_ms: 30_000,
            multiplier: 2.0,
            max_retries: 5,
            jitter_enabled: true,
            jitter_factor: 0.2,
        }
    }

    /// Create a configuration for critical operations (more retries, longer backoff)
    #[must_use]
    pub const fn critical() -> Self {
        Self {
            initial_delay_ms: 1000,
            max_delay_ms: 60_000,
            multiplier: 2.0,
            max_retries: 10,
            jitter_enabled: true,
            jitter_factor: 0.15,
        }
    }

    /// Disable jitter (not recommended for production)
    #[must_use]
    pub const fn without_jitter(mut self) -> Self {
        self.jitter_enabled = false;
        self
    }

    /// Calculate the delay for a given attempt number (0-indexed)
    ///
    /// Uses exponential backoff: delay = initial_delay * multiplier^attempt
    /// Capped at max_delay, with optional jitter to prevent thundering herd.
    #[must_use]
    #[allow(
        clippy::cast_precision_loss,
        clippy::cast_possible_wrap,
        clippy::cast_sign_loss,
        clippy::cast_possible_truncation
    )]
    pub fn delay_for_attempt(&self, attempt: u32) -> Duration {
        // Safe casts: initial_delay_ms and max_delay_ms are u64, converting to f64
        // loses precision for very large values but this is acceptable for delays
        let base_delay = (self.initial_delay_ms as f64) * self.multiplier.powi(attempt as i32);
        let capped_delay = base_delay.min(self.max_delay_ms as f64);

        let final_delay = if self.jitter_enabled {
            let jitter_range = capped_delay * self.jitter_factor;
            let jitter = rand::rng().random_range(-jitter_range..=jitter_range);
            (capped_delay + jitter).max(0.0)
        } else {
            capped_delay
        };

        // Safe: final_delay is capped and non-negative
        Duration::from_millis(final_delay as u64)
    }
}

/// Trait for errors that can be checked for retryability
pub trait Retryable {
    /// Returns true if this error is retryable
    fn is_retryable(&self) -> bool;
}

// Implement Retryable for ApplicationError
impl Retryable for application::ApplicationError {
    fn is_retryable(&self) -> bool {
        Self::is_retryable(self)
    }
}

/// Retry result containing either success or the last error
#[derive(Debug)]
pub struct RetryResult<T, E> {
    /// The result of the operation
    pub result: Result<T, E>,
    /// Number of attempts made (1 = no retries, 2 = one retry, etc.)
    pub attempts: u32,
    /// Total time spent including retries
    pub total_duration: Duration,
}

impl<T, E> RetryResult<T, E> {
    /// Check if the operation succeeded
    #[must_use]
    pub const fn is_ok(&self) -> bool {
        self.result.is_ok()
    }

    /// Check if the operation failed
    #[must_use]
    pub const fn is_err(&self) -> bool {
        self.result.is_err()
    }

    /// Unwrap the result
    ///
    /// # Panics
    ///
    /// Panics if the result is an `Err`.
    #[allow(clippy::unwrap_used)]
    pub fn unwrap(self) -> T
    where
        E: std::fmt::Debug,
    {
        self.result.unwrap()
    }

    /// Convert to standard Result, discarding metadata
    pub fn into_result(self) -> Result<T, E> {
        self.result
    }
}

/// Execute an async operation with retry logic
///
/// Retries the operation according to the configuration when it fails
/// with a retryable error.
///
/// # Arguments
///
/// * `config` - Retry configuration
/// * `operation` - Async function to execute
///
/// # Returns
///
/// `RetryResult` containing the final result and metadata about retry attempts
#[allow(clippy::cast_possible_truncation)]
pub async fn with_retry<F, Fut, T, E>(config: &RetryConfig, mut operation: F) -> RetryResult<T, E>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<T, E>>,
    E: Retryable + std::fmt::Display,
{
    let start = std::time::Instant::now();
    let mut attempts = 0u32;

    loop {
        attempts += 1;
        let result = operation().await;

        match result {
            Ok(value) => {
                if attempts > 1 {
                    debug!(
                        attempts = attempts,
                        duration_ms = start.elapsed().as_millis() as u64,
                        "Operation succeeded after retries"
                    );
                }
                return RetryResult {
                    result: Ok(value),
                    attempts,
                    total_duration: start.elapsed(),
                };
            },
            Err(err) => {
                let retry_attempt = attempts - 1; // 0-indexed for delay calculation

                if !err.is_retryable() {
                    debug!(
                        attempts = attempts,
                        error = %err,
                        "Operation failed with non-retryable error"
                    );
                    return RetryResult {
                        result: Err(err),
                        attempts,
                        total_duration: start.elapsed(),
                    };
                }

                if retry_attempt >= config.max_retries {
                    warn!(
                        attempts = attempts,
                        max_retries = config.max_retries,
                        error = %err,
                        "Operation failed after max retries"
                    );
                    return RetryResult {
                        result: Err(err),
                        attempts,
                        total_duration: start.elapsed(),
                    };
                }

                let delay = config.delay_for_attempt(retry_attempt);
                warn!(
                    attempt = attempts,
                    max_retries = config.max_retries,
                    delay_ms = delay.as_millis() as u64,
                    error = %err,
                    "Operation failed, retrying"
                );

                tokio::time::sleep(delay).await;
            },
        }
    }
}

/// Execute an async operation with retry logic, returning only the Result
///
/// This is a convenience wrapper around `with_retry` that discards metadata.
pub async fn retry<F, Fut, T, E>(config: &RetryConfig, operation: F) -> Result<T, E>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<T, E>>,
    E: Retryable + std::fmt::Display,
{
    with_retry(config, operation).await.into_result()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicU32, Ordering};

    #[derive(Debug, Clone)]
    struct TestError {
        message: String,
        retryable: bool,
    }

    impl std::fmt::Display for TestError {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "{}", self.message)
        }
    }

    impl Retryable for TestError {
        fn is_retryable(&self) -> bool {
            self.retryable
        }
    }

    #[test]
    fn config_default_values() {
        let config = RetryConfig::default();
        assert_eq!(config.initial_delay_ms, 100);
        assert_eq!(config.max_delay_ms, 10_000);
        assert!((config.multiplier - 2.0).abs() < f64::EPSILON);
        assert_eq!(config.max_retries, 3);
        assert!(config.jitter_enabled);
    }

    #[test]
    fn config_fast_preset() {
        let config = RetryConfig::fast();
        assert_eq!(config.initial_delay_ms, 50);
        assert_eq!(config.max_delay_ms, 1000);
        assert_eq!(config.max_retries, 3);
    }

    #[test]
    fn config_slow_preset() {
        let config = RetryConfig::slow();
        assert_eq!(config.initial_delay_ms, 500);
        assert_eq!(config.max_delay_ms, 30_000);
        assert_eq!(config.max_retries, 5);
    }

    #[test]
    fn config_critical_preset() {
        let config = RetryConfig::critical();
        assert_eq!(config.initial_delay_ms, 1000);
        assert_eq!(config.max_delay_ms, 60_000);
        assert_eq!(config.max_retries, 10);
    }

    #[test]
    fn config_without_jitter() {
        let config = RetryConfig::default().without_jitter();
        assert!(!config.jitter_enabled);
    }

    #[test]
    fn delay_calculation_without_jitter() {
        let config = RetryConfig::default().without_jitter();

        assert_eq!(config.delay_for_attempt(0).as_millis(), 100);
        assert_eq!(config.delay_for_attempt(1).as_millis(), 200);
        assert_eq!(config.delay_for_attempt(2).as_millis(), 400);
        assert_eq!(config.delay_for_attempt(3).as_millis(), 800);
    }

    #[test]
    fn delay_capped_at_max() {
        let config = RetryConfig::new(1000, 2000, 2.0, 5).without_jitter();

        assert_eq!(config.delay_for_attempt(0).as_millis(), 1000);
        assert_eq!(config.delay_for_attempt(1).as_millis(), 2000);
        assert_eq!(config.delay_for_attempt(2).as_millis(), 2000); // Capped
        assert_eq!(config.delay_for_attempt(10).as_millis(), 2000); // Still capped
    }

    #[test]
    fn delay_with_jitter_varies() {
        let config = RetryConfig::default();

        // With jitter, delays should vary
        let delays: Vec<_> = (0..10).map(|_| config.delay_for_attempt(0)).collect();
        let all_same = delays.windows(2).all(|w| w[0] == w[1]);
        // Very unlikely that all 10 delays are exactly the same with jitter
        assert!(!all_same || !config.jitter_enabled);
    }

    #[test]
    fn config_serialization() {
        let config = RetryConfig::default();
        let json = serde_json::to_string(&config).unwrap();
        assert!(json.contains("initial_delay_ms"));
        assert!(json.contains("max_delay_ms"));
        assert!(json.contains("multiplier"));
        assert!(json.contains("max_retries"));
    }

    #[test]
    fn config_deserialization() {
        let json = r#"{"initial_delay_ms":200,"max_retries":5}"#;
        let config: RetryConfig = serde_json::from_str(json).unwrap();
        assert_eq!(config.initial_delay_ms, 200);
        assert_eq!(config.max_retries, 5);
        // Defaults for unspecified fields
        assert_eq!(config.max_delay_ms, 10_000);
    }

    #[tokio::test]
    async fn with_retry_succeeds_first_try() {
        let config = RetryConfig::default();
        let call_count = Arc::new(AtomicU32::new(0));

        let result = with_retry(&config, || {
            let count = Arc::clone(&call_count);
            async move {
                count.fetch_add(1, Ordering::SeqCst);
                Ok::<_, TestError>(42)
            }
        })
        .await;

        assert!(result.is_ok());
        assert_eq!(result.attempts, 1);
        assert_eq!(result.result.unwrap(), 42);
        assert_eq!(call_count.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn with_retry_succeeds_after_retries() {
        let config = RetryConfig::fast().without_jitter();
        let call_count = Arc::new(AtomicU32::new(0));

        let result = with_retry(&config, || {
            let count = Arc::clone(&call_count);
            async move {
                let calls = count.fetch_add(1, Ordering::SeqCst) + 1;
                if calls < 3 {
                    Err(TestError {
                        message: "temporary failure".to_string(),
                        retryable: true,
                    })
                } else {
                    Ok(42)
                }
            }
        })
        .await;

        assert!(result.is_ok());
        assert_eq!(result.attempts, 3);
        assert_eq!(result.result.unwrap(), 42);
        assert_eq!(call_count.load(Ordering::SeqCst), 3);
    }

    #[tokio::test]
    async fn with_retry_fails_non_retryable() {
        let config = RetryConfig::default();
        let call_count = Arc::new(AtomicU32::new(0));

        let result = with_retry(&config, || {
            let count = Arc::clone(&call_count);
            async move {
                count.fetch_add(1, Ordering::SeqCst);
                Err::<i32, _>(TestError {
                    message: "permanent failure".to_string(),
                    retryable: false,
                })
            }
        })
        .await;

        assert!(result.is_err());
        assert_eq!(result.attempts, 1);
        assert_eq!(call_count.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn with_retry_fails_after_max_retries() {
        let config = RetryConfig::new(10, 100, 2.0, 2).without_jitter();
        let call_count = Arc::new(AtomicU32::new(0));

        let result = with_retry(&config, || {
            let count = Arc::clone(&call_count);
            async move {
                count.fetch_add(1, Ordering::SeqCst);
                Err::<i32, _>(TestError {
                    message: "always fails".to_string(),
                    retryable: true,
                })
            }
        })
        .await;

        assert!(result.is_err());
        // 1 initial + 2 retries = 3 attempts
        assert_eq!(result.attempts, 3);
        assert_eq!(call_count.load(Ordering::SeqCst), 3);
    }

    #[tokio::test]
    async fn retry_convenience_function() {
        let config = RetryConfig::fast().without_jitter();

        let result: Result<i32, TestError> = retry(&config, || async { Ok(42) }).await;

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 42);
    }

    #[test]
    fn retry_result_is_ok() {
        let result: RetryResult<i32, TestError> = RetryResult {
            result: Ok(42),
            attempts: 1,
            total_duration: Duration::from_millis(10),
        };
        assert!(result.is_ok());
        assert!(!result.is_err());
    }

    #[test]
    fn retry_result_is_err() {
        let result: RetryResult<i32, TestError> = RetryResult {
            result: Err(TestError {
                message: "fail".to_string(),
                retryable: false,
            }),
            attempts: 1,
            total_duration: Duration::from_millis(10),
        };
        assert!(!result.is_ok());
        assert!(result.is_err());
    }

    #[test]
    fn retry_result_into_result() {
        let result: RetryResult<i32, TestError> = RetryResult {
            result: Ok(42),
            attempts: 2,
            total_duration: Duration::from_millis(100),
        };
        let inner = result.into_result();
        assert_eq!(inner.unwrap(), 42);
    }

    #[test]
    fn config_new_custom() {
        let config = RetryConfig::new(200, 5000, 1.5, 4);
        assert_eq!(config.initial_delay_ms, 200);
        assert_eq!(config.max_delay_ms, 5000);
        assert!((config.multiplier - 1.5).abs() < f64::EPSILON);
        assert_eq!(config.max_retries, 4);
        assert!(config.jitter_enabled); // Default true in new()
    }

    #[test]
    fn config_clone() {
        let config = RetryConfig::critical();
        #[allow(clippy::redundant_clone)]
        let cloned = config.clone();
        assert_eq!(config.initial_delay_ms, cloned.initial_delay_ms);
        assert_eq!(config.max_delay_ms, cloned.max_delay_ms);
        assert_eq!(config.max_retries, cloned.max_retries);
    }

    #[test]
    fn config_debug() {
        let config = RetryConfig::default();
        let debug = format!("{config:?}");
        assert!(debug.contains("RetryConfig"));
        assert!(debug.contains("initial_delay_ms"));
        assert!(debug.contains("max_delay_ms"));
    }

    #[test]
    fn retry_result_debug() {
        let result: RetryResult<i32, TestError> = RetryResult {
            result: Ok(42),
            attempts: 1,
            total_duration: Duration::from_millis(10),
        };
        let debug = format!("{result:?}");
        assert!(debug.contains("RetryResult"));
        assert!(debug.contains("attempts"));
    }

    #[test]
    fn retry_result_unwrap_success() {
        let result: RetryResult<i32, TestError> = RetryResult {
            result: Ok(42),
            attempts: 1,
            total_duration: Duration::from_millis(10),
        };
        assert_eq!(result.unwrap(), 42);
    }

    #[test]
    #[should_panic(expected = "called `Result::unwrap()` on an `Err` value")]
    fn retry_result_unwrap_failure() {
        let result: RetryResult<i32, TestError> = RetryResult {
            result: Err(TestError {
                message: "fail".to_string(),
                retryable: false,
            }),
            attempts: 1,
            total_duration: Duration::from_millis(10),
        };
        let _ = result.unwrap();
    }

    #[test]
    fn delay_with_high_attempt() {
        // Test overflow protection with high attempt numbers
        let config = RetryConfig::new(100, 1000, 2.0, 100).without_jitter();
        // Even with a high attempt, delay should be capped
        let delay = config.delay_for_attempt(50);
        assert_eq!(delay.as_millis(), 1000);
    }

    #[test]
    fn delay_with_jitter_in_range() {
        let config = RetryConfig {
            initial_delay_ms: 1000,
            max_delay_ms: 1000,
            multiplier: 1.0,
            max_retries: 3,
            jitter_enabled: true,
            jitter_factor: 0.1, // 10% jitter
        };

        // Take multiple samples and verify they're within expected range
        for _ in 0..20 {
            let delay_ms = config.delay_for_attempt(0).as_millis();
            // With 10% jitter on 1000ms, should be between 900 and 1100
            assert!(
                (900..=1100).contains(&delay_ms),
                "delay_ms={delay_ms} out of range"
            );
        }
    }

    #[test]
    fn test_error_retryable_impl() {
        let retryable_err = TestError {
            message: "temp".to_string(),
            retryable: true,
        };
        assert!(retryable_err.is_retryable());

        let non_retryable_err = TestError {
            message: "perm".to_string(),
            retryable: false,
        };
        assert!(!non_retryable_err.is_retryable());
    }

    #[test]
    fn test_error_display() {
        let err = TestError {
            message: "test error".to_string(),
            retryable: true,
        };
        assert_eq!(format!("{err}"), "test error");
    }

    #[tokio::test]
    async fn retry_with_zero_max_retries() {
        let config = RetryConfig::new(10, 100, 2.0, 0).without_jitter();
        let call_count = Arc::new(AtomicU32::new(0));

        let result = with_retry(&config, || {
            let count = Arc::clone(&call_count);
            async move {
                count.fetch_add(1, Ordering::SeqCst);
                Err::<i32, _>(TestError {
                    message: "always fails".to_string(),
                    retryable: true,
                })
            }
        })
        .await;

        assert!(result.is_err());
        // With max_retries=0, should only try once
        assert_eq!(result.attempts, 1);
        assert_eq!(call_count.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn retry_tracks_duration() {
        let config = RetryConfig::new(50, 100, 2.0, 1).without_jitter();
        let call_count = Arc::new(AtomicU32::new(0));

        let result = with_retry(&config, || {
            let count = Arc::clone(&call_count);
            async move {
                let calls = count.fetch_add(1, Ordering::SeqCst) + 1;
                if calls < 2 {
                    Err(TestError {
                        message: "fail once".to_string(),
                        retryable: true,
                    })
                } else {
                    Ok(42)
                }
            }
        })
        .await;

        assert!(result.is_ok());
        // Should have some duration due to the retry delay
        assert!(result.total_duration.as_millis() >= 40); // At least some delay
    }
}
