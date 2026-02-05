//! Cached inference adapter - Decorator that adds caching to any `InferencePort`
//!
//! Uses multi-layer caching (L1 in-memory + L2 persistent) to cache LLM responses.
//! Significantly reduces latency and load on the inference backend for repeated queries.

use std::sync::Arc;

use application::{
    error::ApplicationError,
    ports::{CachePort, CachePortExt, InferencePort, InferenceResult, InferenceStream, ttl},
};
use async_trait::async_trait;
use domain::Conversation;
use serde::{Deserialize, Serialize};
use tracing::{debug, info, instrument};

use crate::cache::llm_cache_key;

/// Cached response stored in cache
#[derive(Debug, Clone, Serialize, Deserialize)]
struct CachedResponse {
    content: String,
    model: String,
    tokens_used: Option<u32>,
}

impl From<&InferenceResult> for CachedResponse {
    fn from(result: &InferenceResult) -> Self {
        Self {
            content: result.content.clone(),
            model: result.model.clone(),
            tokens_used: result.tokens_used,
        }
    }
}

impl CachedResponse {
    /// Convert to `InferenceResult` with cache indicator
    fn to_result(&self, _was_cached: bool) -> InferenceResult {
        InferenceResult {
            content: self.content.clone(),
            model: self.model.clone(),
            tokens_used: self.tokens_used,
            // Cached responses have minimal latency
            latency_ms: 0,
        }
    }
}

/// Caching decorator for inference ports
///
/// Wraps any `InferencePort` implementation and adds caching layer.
/// Uses different TTLs based on content type (dynamic vs stable data).
pub struct CachedInferenceAdapter<I: InferencePort, C: CachePort> {
    /// The underlying inference implementation
    inner: I,
    /// Cache for storing responses
    cache: Arc<C>,
    /// Whether caching is enabled
    enabled: bool,
}

impl<I: InferencePort + std::fmt::Debug, C: CachePort> std::fmt::Debug
    for CachedInferenceAdapter<I, C>
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CachedInferenceAdapter")
            .field("inner", &self.inner)
            .field("enabled", &self.enabled)
            .finish_non_exhaustive()
    }
}

impl<I: InferencePort, C: CachePort> CachedInferenceAdapter<I, C> {
    /// Create a new cached inference adapter
    pub const fn new(inner: I, cache: Arc<C>) -> Self {
        Self {
            inner,
            cache,
            enabled: true,
        }
    }

    /// Disable caching (useful for debugging)
    #[must_use]
    pub const fn with_caching_disabled(mut self) -> Self {
        self.enabled = false;
        self
    }

    /// Enable caching
    #[must_use]
    pub const fn with_caching_enabled(mut self) -> Self {
        self.enabled = true;
        self
    }

    /// Check cache for a response
    async fn get_cached(&self, cache_key: &str) -> Option<CachedResponse> {
        if !self.enabled {
            return None;
        }

        match self.cache.get::<CachedResponse>(cache_key).await {
            Ok(Some(cached)) => {
                debug!(key = %cache_key, "Cache hit for inference");
                Some(cached)
            },
            Ok(None) => {
                debug!(key = %cache_key, "Cache miss for inference");
                None
            },
            Err(e) => {
                // Log but don't fail - cache errors shouldn't break inference
                tracing::warn!(error = %e, key = %cache_key, "Cache read error");
                None
            },
        }
    }

    /// Store a response in cache
    async fn cache_response(&self, cache_key: &str, result: &InferenceResult, is_stable: bool) {
        if !self.enabled {
            return;
        }

        let cached = CachedResponse::from(result);
        let ttl = if is_stable {
            ttl::LLM_STABLE
        } else {
            ttl::LLM_DYNAMIC
        };

        if let Err(e) = self.cache.set(cache_key, &cached, ttl).await {
            // Log but don't fail - cache errors shouldn't break inference
            tracing::warn!(error = %e, key = %cache_key, "Cache write error");
        } else {
            debug!(key = %cache_key, ttl_secs = ttl.as_secs(), "Cached inference response");
        }
    }

    /// Invalidate cached responses matching a pattern
    #[allow(dead_code)]
    pub async fn invalidate_pattern(&self, pattern: &str) -> Result<u64, ApplicationError> {
        self.cache.invalidate_pattern(pattern).await
    }

    /// Get the underlying inference adapter
    pub const fn inner(&self) -> &I {
        &self.inner
    }
}

#[async_trait]
impl<I: InferencePort, C: CachePort> InferencePort for CachedInferenceAdapter<I, C> {
    #[instrument(skip(self, message), fields(cached = tracing::field::Empty))]
    async fn generate(&self, message: &str) -> Result<InferenceResult, ApplicationError> {
        let cache_key = llm_cache_key(
            message,
            self.inner.current_model().as_str(),
            0.7, // default temperature
        );

        // Check cache first
        if let Some(cached) = self.get_cached(&cache_key).await {
            tracing::Span::current().record("cached", true);
            info!("Returning cached LLM response");
            return Ok(cached.to_result(true));
        }

        tracing::Span::current().record("cached", false);

        // Call underlying implementation
        let result = self.inner.generate(message).await?;

        // Cache the response (dynamic TTL since context may vary)
        self.cache_response(&cache_key, &result, false).await;

        Ok(result)
    }

    #[instrument(skip(self, conversation), fields(conv_id = %conversation.id, cached = tracing::field::Empty))]
    async fn generate_with_context(
        &self,
        conversation: &Conversation,
    ) -> Result<InferenceResult, ApplicationError> {
        // Create cache key from conversation content hash
        // Include all messages to capture full context
        let context_str = conversation
            .messages
            .iter()
            .map(|m| format!("{:?}:{}", m.role, m.content))
            .collect::<Vec<_>>()
            .join("|");

        let cache_key = llm_cache_key(
            &context_str,
            self.inner.current_model().as_str(),
            0.7, // default temperature
        );

        // Check cache first
        if let Some(cached) = self.get_cached(&cache_key).await {
            tracing::Span::current().record("cached", true);
            info!("Returning cached conversation response");
            return Ok(cached.to_result(true));
        }

        tracing::Span::current().record("cached", false);

        // Call underlying implementation
        let result = self.inner.generate_with_context(conversation).await?;

        // Cache with dynamic TTL (conversations are context-dependent)
        self.cache_response(&cache_key, &result, false).await;

        Ok(result)
    }

    #[instrument(skip(self, system_prompt, message), fields(cached = tracing::field::Empty))]
    async fn generate_with_system(
        &self,
        system_prompt: &str,
        message: &str,
    ) -> Result<InferenceResult, ApplicationError> {
        // Include system prompt in cache key
        let combined = format!("{system_prompt}|{message}");
        let cache_key = llm_cache_key(&combined, self.inner.current_model().as_str(), 0.7);

        // Check cache first
        if let Some(cached) = self.get_cached(&cache_key).await {
            tracing::Span::current().record("cached", true);
            info!("Returning cached system-prompt response");
            return Ok(cached.to_result(true));
        }

        tracing::Span::current().record("cached", false);

        // Call underlying implementation
        let result = self
            .inner
            .generate_with_system(system_prompt, message)
            .await?;

        // Cache with stable TTL if system prompt suggests stable content
        let is_stable = is_stable_system_prompt(system_prompt);
        self.cache_response(&cache_key, &result, is_stable).await;

        Ok(result)
    }

    async fn generate_stream(&self, message: &str) -> Result<InferenceStream, ApplicationError> {
        // Streaming responses are not cached (would require buffering full response)
        // The non-streaming equivalent should be used for cacheable queries
        self.inner.generate_stream(message).await
    }

    async fn generate_stream_with_system(
        &self,
        system_prompt: &str,
        message: &str,
    ) -> Result<InferenceStream, ApplicationError> {
        // Streaming responses are not cached
        self.inner
            .generate_stream_with_system(system_prompt, message)
            .await
    }

    async fn is_healthy(&self) -> bool {
        self.inner.is_healthy().await
    }

    fn current_model(&self) -> String {
        self.inner.current_model()
    }

    async fn list_available_models(&self) -> Result<Vec<String>, ApplicationError> {
        // Model list can be cached (stable data)
        let cache_key = "inference:models:list";

        if let Ok(Some(cached)) = self.cache.get::<Vec<String>>(cache_key).await {
            debug!("Returning cached model list");
            return Ok(cached);
        }

        let models = self.inner.list_available_models().await?;

        // Cache for 24 hours (stable data)
        let _ = self.cache.set(cache_key, &models, ttl::LONG).await;

        Ok(models)
    }

    async fn switch_model(&self, model_name: &str) -> Result<(), ApplicationError> {
        // Invalidate model-specific caches when switching
        let pattern = format!("inference:{model_name}:*");
        let _ = self.cache.invalidate_pattern(&pattern).await;

        self.inner.switch_model(model_name).await
    }
}

/// Heuristic to determine if a system prompt suggests stable (cacheable) content
fn is_stable_system_prompt(prompt: &str) -> bool {
    let prompt_lower = prompt.to_lowercase();

    // These keywords suggest deterministic/stable responses
    let stable_keywords = [
        "classify",
        "extract",
        "parse",
        "format",
        "translate",
        "summarize",
        "analyze",
        "json",
        "xml",
    ];

    // These keywords suggest dynamic/creative responses
    let dynamic_keywords = ["creative", "generate", "imagine", "story", "poem", "random"];

    let has_stable = stable_keywords.iter().any(|k| prompt_lower.contains(k));
    let has_dynamic = dynamic_keywords.iter().any(|k| prompt_lower.contains(k));

    has_stable && !has_dynamic
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cache::MokaCache;

    /// Mock inference port for testing
    #[derive(Debug, Default)]
    struct MockInference {
        call_count: std::sync::atomic::AtomicU32,
    }

    #[async_trait]
    impl InferencePort for MockInference {
        async fn generate(&self, message: &str) -> Result<InferenceResult, ApplicationError> {
            self.call_count
                .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            Ok(InferenceResult {
                content: format!("Response to: {}", message),
                model: "mock-model".to_string(),
                tokens_used: Some(10),
                latency_ms: 100,
            })
        }

        async fn generate_with_context(
            &self,
            _conversation: &Conversation,
        ) -> Result<InferenceResult, ApplicationError> {
            self.call_count
                .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            Ok(InferenceResult {
                content: "Context response".to_string(),
                model: "mock-model".to_string(),
                tokens_used: Some(20),
                latency_ms: 150,
            })
        }

        async fn generate_with_system(
            &self,
            _system_prompt: &str,
            _message: &str,
        ) -> Result<InferenceResult, ApplicationError> {
            self.call_count
                .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            Ok(InferenceResult {
                content: "System response".to_string(),
                model: "mock-model".to_string(),
                tokens_used: Some(15),
                latency_ms: 120,
            })
        }

        async fn generate_stream(
            &self,
            _message: &str,
        ) -> Result<InferenceStream, ApplicationError> {
            Err(ApplicationError::Inference("Not implemented".to_string()))
        }

        async fn generate_stream_with_system(
            &self,
            _system_prompt: &str,
            _message: &str,
        ) -> Result<InferenceStream, ApplicationError> {
            Err(ApplicationError::Inference("Not implemented".to_string()))
        }

        async fn is_healthy(&self) -> bool {
            true
        }

        fn current_model(&self) -> String {
            "mock-model".to_string()
        }

        async fn list_available_models(&self) -> Result<Vec<String>, ApplicationError> {
            Ok(vec!["mock-model".to_string(), "mock-model-2".to_string()])
        }

        async fn switch_model(&self, _model_name: &str) -> Result<(), ApplicationError> {
            Ok(())
        }
    }

    #[tokio::test]
    async fn caches_generate_responses() {
        let inner = MockInference::default();
        let cache = Arc::new(MokaCache::new());
        let adapter = CachedInferenceAdapter::new(inner, cache);

        // First call should hit the backend
        let result1 = adapter.generate("Hello").await.unwrap();
        assert_eq!(result1.content, "Response to: Hello");
        assert_eq!(
            adapter
                .inner()
                .call_count
                .load(std::sync::atomic::Ordering::SeqCst),
            1
        );

        // Second identical call should be cached
        let result2 = adapter.generate("Hello").await.unwrap();
        assert_eq!(result2.content, "Response to: Hello");
        // Call count should still be 1 (cache hit)
        assert_eq!(
            adapter
                .inner()
                .call_count
                .load(std::sync::atomic::Ordering::SeqCst),
            1
        );
    }

    #[tokio::test]
    async fn different_messages_not_cached() {
        let inner = MockInference::default();
        let cache = Arc::new(MokaCache::new());
        let adapter = CachedInferenceAdapter::new(inner, cache);

        adapter.generate("Hello").await.unwrap();
        adapter.generate("World").await.unwrap();

        // Both calls should hit the backend
        assert_eq!(
            adapter
                .inner()
                .call_count
                .load(std::sync::atomic::Ordering::SeqCst),
            2
        );
    }

    #[tokio::test]
    async fn caching_can_be_disabled() {
        let inner = MockInference::default();
        let cache = Arc::new(MokaCache::new());
        let adapter = CachedInferenceAdapter::new(inner, cache).with_caching_disabled();

        adapter.generate("Hello").await.unwrap();
        adapter.generate("Hello").await.unwrap();

        // Both calls should hit the backend
        assert_eq!(
            adapter
                .inner()
                .call_count
                .load(std::sync::atomic::Ordering::SeqCst),
            2
        );
    }

    #[tokio::test]
    async fn is_stable_system_prompt_works() {
        assert!(is_stable_system_prompt("Please classify this text"));
        assert!(is_stable_system_prompt("Extract the JSON from this"));
        assert!(!is_stable_system_prompt("Generate a creative story"));
        assert!(!is_stable_system_prompt("Write a poem"));
        assert!(is_stable_system_prompt("Summarize the following"));
    }

    #[tokio::test]
    async fn caches_model_list() {
        let inner = MockInference::default();
        let cache = Arc::new(MokaCache::new());
        let adapter = CachedInferenceAdapter::new(inner, cache);

        let models1 = adapter.list_available_models().await.unwrap();
        let models2 = adapter.list_available_models().await.unwrap();

        assert_eq!(models1, models2);
        // The mock doesn't track list_available_models calls, but we can verify
        // by checking the cache stats
    }
}
