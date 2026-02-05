//! Degraded inference adapter
//!
//! Provides graceful degradation when the primary inference backend (Hailo) is unavailable.
//! Falls back to cached responses or user-friendly error messages.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{Duration, Instant};

use async_trait::async_trait;
use domain::Conversation;
use futures::stream;
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use tracing::{debug, info, warn};

use application::{
    ApplicationError,
    ports::{InferencePort, InferenceResult, InferenceStream, StreamingChunk},
};

/// Configuration for degraded mode behavior
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DegradedModeConfig {
    /// Enable degraded mode (if false, errors are passed through)
    #[serde(default = "default_enabled")]
    pub enabled: bool,

    /// Default message to return when service is unavailable
    #[serde(default = "default_unavailable_message")]
    pub unavailable_message: String,

    /// Cooldown period before retrying the primary backend (seconds)
    #[serde(default = "default_retry_cooldown")]
    pub retry_cooldown_secs: u64,

    /// Number of consecutive failures before entering degraded mode
    #[serde(default = "default_failure_threshold")]
    pub failure_threshold: u32,

    /// Number of consecutive successes before exiting degraded mode
    #[serde(default = "default_success_threshold")]
    pub success_threshold: u32,
}

const fn default_enabled() -> bool {
    true
}

fn default_unavailable_message() -> String {
    "I'm currently experiencing technical difficulties. Please try again in a moment.".to_string()
}

const fn default_retry_cooldown() -> u64 {
    30
}

const fn default_failure_threshold() -> u32 {
    3
}

const fn default_success_threshold() -> u32 {
    2
}

impl Default for DegradedModeConfig {
    fn default() -> Self {
        Self {
            enabled: default_enabled(),
            unavailable_message: default_unavailable_message(),
            retry_cooldown_secs: default_retry_cooldown(),
            failure_threshold: default_failure_threshold(),
            success_threshold: default_success_threshold(),
        }
    }
}

/// Service status for tracking health
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum ServiceStatus {
    /// Service is healthy and responding
    #[default]
    Healthy,
    /// Service is degraded but functional
    Degraded,
    /// Service is unavailable
    Unavailable,
}

/// Statistics for degraded mode operation
#[derive(Debug, Clone, Default)]
pub struct DegradedModeStats {
    /// Total requests handled
    pub total_requests: u64,
    /// Requests handled in degraded mode
    pub degraded_requests: u64,
    /// Successful fallback responses
    pub fallback_responses: u64,
    /// Current status
    pub status: ServiceStatus,
}

/// Degraded mode inference adapter
///
/// Wraps a primary inference adapter and provides graceful degradation
/// when the primary backend becomes unavailable.
pub struct DegradedInferenceAdapter<I: InferencePort> {
    inner: Arc<I>,
    config: DegradedModeConfig,
    is_degraded: AtomicBool,
    consecutive_failures: AtomicU64,
    consecutive_successes: AtomicU64,
    last_failure_time: RwLock<Option<Instant>>,
    stats: RwLock<DegradedModeStats>,
}

impl<I: InferencePort> std::fmt::Debug for DegradedInferenceAdapter<I> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DegradedInferenceAdapter")
            .field("is_degraded", &self.is_degraded.load(Ordering::Relaxed))
            .field("config", &self.config)
            .finish_non_exhaustive()
    }
}

impl<I: InferencePort + 'static> DegradedInferenceAdapter<I> {
    /// Create a new degraded inference adapter
    pub fn new(inner: Arc<I>, config: DegradedModeConfig) -> Self {
        Self {
            inner,
            config,
            is_degraded: AtomicBool::new(false),
            consecutive_failures: AtomicU64::new(0),
            consecutive_successes: AtomicU64::new(0),
            last_failure_time: RwLock::new(None),
            stats: RwLock::new(DegradedModeStats::default()),
        }
    }

    /// Create with default configuration
    pub fn with_defaults(inner: Arc<I>) -> Self {
        Self::new(inner, DegradedModeConfig::default())
    }

    /// Check if currently in degraded mode
    pub fn is_degraded(&self) -> bool {
        self.is_degraded.load(Ordering::Relaxed)
    }

    /// Get current service status
    pub fn status(&self) -> ServiceStatus {
        self.stats.read().status
    }

    /// Get degraded mode statistics
    pub fn stats(&self) -> DegradedModeStats {
        self.stats.read().clone()
    }

    /// Check if we should retry the primary backend
    fn should_retry_primary(&self) -> bool {
        if !self.is_degraded() {
            return true;
        }

        let last_failure = self.last_failure_time.read();
        last_failure.is_none_or(|time| {
            let elapsed = time.elapsed();
            elapsed >= Duration::from_secs(self.config.retry_cooldown_secs)
        })
    }

    /// Record a successful operation
    fn record_success(&self) {
        self.consecutive_failures.store(0, Ordering::Relaxed);
        let successes = self.consecutive_successes.fetch_add(1, Ordering::Relaxed) + 1;

        if self.is_degraded() && successes >= u64::from(self.config.success_threshold) {
            info!(
                "Exiting degraded mode after {} consecutive successes",
                successes
            );
            self.is_degraded.store(false, Ordering::Relaxed);
            self.consecutive_successes.store(0, Ordering::Relaxed);
            self.stats.write().status = ServiceStatus::Healthy;
        }

        // Update stats
        self.stats.write().total_requests += 1;
    }

    /// Record a failed operation
    fn record_failure(&self) {
        self.consecutive_successes.store(0, Ordering::Relaxed);
        let failures = self.consecutive_failures.fetch_add(1, Ordering::Relaxed) + 1;
        *self.last_failure_time.write() = Some(Instant::now());

        if !self.is_degraded() && failures >= u64::from(self.config.failure_threshold) {
            warn!(
                "Entering degraded mode after {} consecutive failures",
                failures
            );
            self.is_degraded.store(true, Ordering::Relaxed);
            self.stats.write().status = ServiceStatus::Degraded;
        }

        // Update stats
        let mut stats = self.stats.write();
        stats.total_requests += 1;
        stats.degraded_requests += 1;
    }

    /// Generate a fallback response
    fn fallback_response(&self) -> InferenceResult {
        self.stats.write().fallback_responses += 1;
        InferenceResult {
            content: self.config.unavailable_message.clone(),
            model: "fallback".to_string(),
            tokens_used: None,
            latency_ms: 0,
        }
    }

    /// Generate a fallback streaming response
    fn fallback_stream(&self) -> InferenceStream {
        let message = self.config.unavailable_message.clone();
        self.stats.write().fallback_responses += 1;
        Box::pin(stream::once(async move {
            Ok(StreamingChunk {
                content: message,
                done: true,
                model: Some("fallback".to_string()),
            })
        }))
    }

    /// Handle result from primary backend
    fn handle_result<T>(
        &self,
        result: Result<T, ApplicationError>,
        fallback: impl FnOnce() -> T,
    ) -> Result<T, ApplicationError> {
        match result {
            Ok(value) => {
                self.record_success();
                Ok(value)
            },
            Err(e) => {
                self.record_failure();

                if self.config.enabled && self.is_degraded() {
                    debug!("Using fallback response due to degraded mode");
                    Ok(fallback())
                } else {
                    Err(e)
                }
            },
        }
    }
}

#[async_trait]
impl<I: InferencePort + 'static> InferencePort for DegradedInferenceAdapter<I> {
    async fn generate(&self, message: &str) -> Result<InferenceResult, ApplicationError> {
        if !self.should_retry_primary() {
            debug!("Skipping primary backend due to cooldown");
            return Ok(self.fallback_response());
        }

        let result = self.inner.generate(message).await;
        self.handle_result(result, || self.fallback_response())
    }

    async fn generate_with_context(
        &self,
        conversation: &Conversation,
    ) -> Result<InferenceResult, ApplicationError> {
        if !self.should_retry_primary() {
            return Ok(self.fallback_response());
        }

        let result = self.inner.generate_with_context(conversation).await;
        self.handle_result(result, || self.fallback_response())
    }

    async fn generate_with_system(
        &self,
        system_prompt: &str,
        message: &str,
    ) -> Result<InferenceResult, ApplicationError> {
        if !self.should_retry_primary() {
            return Ok(self.fallback_response());
        }

        let result = self
            .inner
            .generate_with_system(system_prompt, message)
            .await;
        self.handle_result(result, || self.fallback_response())
    }

    async fn generate_stream(&self, message: &str) -> Result<InferenceStream, ApplicationError> {
        if !self.should_retry_primary() {
            return Ok(self.fallback_stream());
        }

        let result = self.inner.generate_stream(message).await;
        self.handle_result(result, || self.fallback_stream())
    }

    async fn generate_stream_with_system(
        &self,
        system_prompt: &str,
        message: &str,
    ) -> Result<InferenceStream, ApplicationError> {
        if !self.should_retry_primary() {
            return Ok(self.fallback_stream());
        }

        let result = self
            .inner
            .generate_stream_with_system(system_prompt, message)
            .await;
        self.handle_result(result, || self.fallback_stream())
    }

    async fn is_healthy(&self) -> bool {
        if self.is_degraded() {
            // In degraded mode, check periodically
            if self.should_retry_primary() {
                let healthy = self.inner.is_healthy().await;
                if healthy {
                    self.record_success();
                } else {
                    self.record_failure();
                }
                return healthy;
            }
            return false;
        }

        self.inner.is_healthy().await
    }

    fn current_model(&self) -> String {
        if self.is_degraded() {
            "fallback (degraded)".to_string()
        } else {
            self.inner.current_model()
        }
    }

    async fn list_available_models(&self) -> Result<Vec<String>, ApplicationError> {
        if !self.should_retry_primary() {
            return Ok(vec!["fallback".to_string()]);
        }

        match self.inner.list_available_models().await {
            Ok(models) => {
                self.record_success();
                Ok(models)
            },
            Err(e) => {
                self.record_failure();
                if self.config.enabled && self.is_degraded() {
                    Ok(vec!["fallback".to_string()])
                } else {
                    Err(e)
                }
            },
        }
    }

    async fn switch_model(&self, model_name: &str) -> Result<(), ApplicationError> {
        if !self.should_retry_primary() {
            return Err(ApplicationError::Inference(
                "Service unavailable in degraded mode".to_string(),
            ));
        }

        match self.inner.switch_model(model_name).await {
            Ok(()) => {
                self.record_success();
                Ok(())
            },
            Err(e) => {
                self.record_failure();
                Err(e)
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::StreamExt;

    /// Mock inference port for testing
    struct MockInference {
        should_fail: AtomicBool,
    }

    impl MockInference {
        fn new() -> Self {
            Self {
                should_fail: AtomicBool::new(false),
            }
        }

        fn set_fail(&self, fail: bool) {
            self.should_fail.store(fail, Ordering::Relaxed);
        }
    }

    #[async_trait]
    impl InferencePort for MockInference {
        async fn generate(&self, _message: &str) -> Result<InferenceResult, ApplicationError> {
            if self.should_fail.load(Ordering::Relaxed) {
                Err(ApplicationError::Inference("Mock failure".to_string()))
            } else {
                Ok(InferenceResult {
                    content: "Mock response".to_string(),
                    model: "mock-model".to_string(),
                    tokens_used: Some(10),
                    latency_ms: 100,
                })
            }
        }

        async fn generate_with_context(
            &self,
            _conversation: &Conversation,
        ) -> Result<InferenceResult, ApplicationError> {
            self.generate("").await
        }

        async fn generate_with_system(
            &self,
            _system_prompt: &str,
            _message: &str,
        ) -> Result<InferenceResult, ApplicationError> {
            self.generate("").await
        }

        async fn generate_stream(
            &self,
            _message: &str,
        ) -> Result<InferenceStream, ApplicationError> {
            if self.should_fail.load(Ordering::Relaxed) {
                Err(ApplicationError::Inference("Mock failure".to_string()))
            } else {
                Ok(Box::pin(stream::once(async {
                    Ok(StreamingChunk {
                        content: "Mock stream".to_string(),
                        done: true,
                        model: Some("mock-model".to_string()),
                    })
                })))
            }
        }

        async fn generate_stream_with_system(
            &self,
            _system_prompt: &str,
            _message: &str,
        ) -> Result<InferenceStream, ApplicationError> {
            self.generate_stream("").await
        }

        async fn is_healthy(&self) -> bool {
            !self.should_fail.load(Ordering::Relaxed)
        }

        fn current_model(&self) -> String {
            "mock-model".to_string()
        }

        async fn list_available_models(&self) -> Result<Vec<String>, ApplicationError> {
            if self.should_fail.load(Ordering::Relaxed) {
                Err(ApplicationError::Inference("Mock failure".to_string()))
            } else {
                Ok(vec!["mock-model".to_string()])
            }
        }

        async fn switch_model(&self, _model_name: &str) -> Result<(), ApplicationError> {
            if self.should_fail.load(Ordering::Relaxed) {
                Err(ApplicationError::Inference("Mock failure".to_string()))
            } else {
                Ok(())
            }
        }
    }

    #[test]
    fn test_config_default() {
        let config = DegradedModeConfig::default();
        assert!(config.enabled);
        assert_eq!(config.retry_cooldown_secs, 30);
        assert_eq!(config.failure_threshold, 3);
        assert_eq!(config.success_threshold, 2);
    }

    #[tokio::test]
    async fn test_healthy_passthrough() {
        let mock = Arc::new(MockInference::new());
        let adapter = DegradedInferenceAdapter::with_defaults(mock);

        let result = adapter.generate("test").await.unwrap();
        assert_eq!(result.content, "Mock response");
        assert_eq!(result.model, "mock-model");
        assert!(!adapter.is_degraded());
    }

    #[tokio::test]
    async fn test_enters_degraded_mode_after_failures() {
        let mock = Arc::new(MockInference::new());
        let config = DegradedModeConfig {
            failure_threshold: 2,
            ..Default::default()
        };
        let adapter = DegradedInferenceAdapter::new(Arc::clone(&mock), config);

        mock.set_fail(true);

        // First failure
        let _ = adapter.generate("test").await;
        assert!(!adapter.is_degraded());

        // Second failure - should enter degraded mode
        let _ = adapter.generate("test").await;
        assert!(adapter.is_degraded());
    }

    #[tokio::test]
    async fn test_fallback_response_in_degraded_mode() {
        let mock = Arc::new(MockInference::new());
        let config = DegradedModeConfig {
            failure_threshold: 1,
            unavailable_message: "Service down".to_string(),
            ..Default::default()
        };
        let adapter = DegradedInferenceAdapter::new(Arc::clone(&mock), config);

        // Force into degraded mode
        mock.set_fail(true);
        let _ = adapter.generate("test").await;

        // Should return fallback
        let result = adapter.generate("test").await.unwrap();
        assert_eq!(result.content, "Service down");
        assert_eq!(result.model, "fallback");
    }

    #[tokio::test]
    async fn test_exits_degraded_mode_after_successes() {
        let mock = Arc::new(MockInference::new());
        let config = DegradedModeConfig {
            failure_threshold: 1,
            success_threshold: 2,
            retry_cooldown_secs: 0, // No cooldown for testing
            ..Default::default()
        };
        let adapter = DegradedInferenceAdapter::new(Arc::clone(&mock), config);

        // Enter degraded mode
        mock.set_fail(true);
        let _ = adapter.generate("test").await;
        assert!(adapter.is_degraded());

        // Recover
        mock.set_fail(false);
        let _ = adapter.generate("test").await;
        assert!(adapter.is_degraded()); // Still degraded

        let _ = adapter.generate("test").await;
        assert!(!adapter.is_degraded()); // Exited after 2 successes
    }

    #[tokio::test]
    async fn test_streaming_fallback() {
        let mock = Arc::new(MockInference::new());
        let config = DegradedModeConfig {
            failure_threshold: 1,
            unavailable_message: "Stream unavailable".to_string(),
            retry_cooldown_secs: 0,
            ..Default::default()
        };
        let adapter = DegradedInferenceAdapter::new(Arc::clone(&mock), config);

        // Enter degraded mode
        mock.set_fail(true);
        let _ = adapter.generate_stream("test").await;

        // Get fallback stream
        let stream = adapter.generate_stream("test").await.unwrap();
        let chunks: Vec<_> = stream.collect().await;

        assert_eq!(chunks.len(), 1);
        let chunk = chunks[0].as_ref().unwrap();
        assert_eq!(chunk.content, "Stream unavailable");
        assert!(chunk.done);
    }

    #[test]
    fn test_service_status_default() {
        assert_eq!(ServiceStatus::default(), ServiceStatus::Healthy);
    }

    #[tokio::test]
    async fn test_stats_tracking() {
        let mock = Arc::new(MockInference::new());
        let adapter = DegradedInferenceAdapter::with_defaults(Arc::clone(&mock));

        // Successful requests
        let _ = adapter.generate("test").await;
        let _ = adapter.generate("test").await;

        let stats = adapter.stats();
        assert_eq!(stats.total_requests, 2);
        assert_eq!(stats.degraded_requests, 0);
        assert_eq!(stats.fallback_responses, 0);
    }
}
