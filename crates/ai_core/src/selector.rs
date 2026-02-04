//! Dynamic model selection based on task complexity
//!
//! Provides intelligent routing of inference requests to appropriate models
//! based on task complexity, resource availability, and performance requirements.

use std::sync::Arc;

use async_trait::async_trait;
use tracing::{debug, instrument};

use crate::{
    error::InferenceError,
    ports::{InferenceEngine, InferenceRequest, InferenceResponse, StreamingResponse},
};

/// Configuration for model selection
#[derive(Debug, Clone)]
pub struct ModelSelectorConfig {
    /// Model to use for simple/fast tasks
    pub small_model: String,
    /// Model to use for complex/quality tasks
    pub large_model: String,
    /// Threshold for word count to use large model
    pub complexity_word_threshold: usize,
    /// Keywords that trigger large model usage
    pub complexity_keywords: Vec<String>,
    /// Maximum prompt length for small model (characters)
    pub small_model_max_prompt_chars: usize,
}

impl Default for ModelSelectorConfig {
    fn default() -> Self {
        Self {
            small_model: "qwen2.5-1.5b-instruct".to_string(),
            large_model: "qwen2.5-7b-instruct".to_string(),
            complexity_word_threshold: 100,
            complexity_keywords: vec![
                "analyze".to_string(),
                "explain".to_string(),
                "compare".to_string(),
                "summarize".to_string(),
                "code".to_string(),
                "implement".to_string(),
                "debug".to_string(),
                "refactor".to_string(),
                "translate".to_string(),
                "research".to_string(),
            ],
            small_model_max_prompt_chars: 500,
        }
    }
}

/// Task complexity level
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskComplexity {
    /// Simple tasks - quick responses, basic questions
    Simple,
    /// Complex tasks - analysis, code generation, long-form content
    Complex,
}

impl std::fmt::Display for TaskComplexity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Simple => write!(f, "simple"),
            Self::Complex => write!(f, "complex"),
        }
    }
}

/// Model selector that routes requests based on complexity
#[derive(Debug)]
pub struct ModelSelector<E: InferenceEngine> {
    engine: Arc<E>,
    config: ModelSelectorConfig,
}

impl<E: InferenceEngine> ModelSelector<E> {
    /// Create a new model selector
    #[allow(clippy::missing_const_for_fn)]
    pub fn new(engine: Arc<E>, config: ModelSelectorConfig) -> Self {
        Self { engine, config }
    }

    /// Create with default configuration
    pub fn with_defaults(engine: Arc<E>) -> Self {
        Self::new(engine, ModelSelectorConfig::default())
    }

    /// Analyze the complexity of a request
    #[must_use]
    pub fn analyze_complexity(&self, request: &InferenceRequest) -> TaskComplexity {
        // Combine all message content for analysis
        let full_text: String = request
            .messages
            .iter()
            .map(|m| m.content.as_str())
            .collect::<Vec<_>>()
            .join(" ");

        let full_text_lower = full_text.to_lowercase();

        // Check total length
        if full_text.len() > self.config.small_model_max_prompt_chars {
            debug!(
                chars = full_text.len(),
                threshold = self.config.small_model_max_prompt_chars,
                "Request exceeds small model char limit"
            );
            return TaskComplexity::Complex;
        }

        // Check word count
        let word_count = full_text.split_whitespace().count();
        if word_count > self.config.complexity_word_threshold {
            debug!(
                words = word_count,
                threshold = self.config.complexity_word_threshold,
                "Request exceeds word threshold"
            );
            return TaskComplexity::Complex;
        }

        // Check for complexity keywords
        for keyword in &self.config.complexity_keywords {
            if full_text_lower.contains(keyword) {
                debug!(keyword = %keyword, "Found complexity keyword");
                return TaskComplexity::Complex;
            }
        }

        TaskComplexity::Simple
    }

    /// Select the appropriate model based on complexity
    #[must_use]
    pub fn select_model(&self, complexity: TaskComplexity) -> &str {
        match complexity {
            TaskComplexity::Simple => &self.config.small_model,
            TaskComplexity::Complex => &self.config.large_model,
        }
    }

    /// Apply the selected model to the request
    #[must_use]
    pub fn apply_model(&self, request: InferenceRequest) -> InferenceRequest {
        // If request already has a model, respect it
        if request.model.is_some() {
            return request;
        }

        let complexity = self.analyze_complexity(&request);
        let model = self.select_model(complexity);
        debug!(
            complexity = %complexity,
            model = %model,
            "Selected model for request"
        );
        request.with_model(model.to_string())
    }

    /// Get the configuration
    #[must_use]
    pub const fn config(&self) -> &ModelSelectorConfig {
        &self.config
    }

    /// Update configuration
    pub fn set_config(&mut self, config: ModelSelectorConfig) {
        self.config = config;
    }

    /// Force use of small model
    #[must_use]
    pub fn force_small_model(&self, request: InferenceRequest) -> InferenceRequest {
        request.with_model(self.config.small_model.clone())
    }

    /// Force use of large model
    #[must_use]
    pub fn force_large_model(&self, request: InferenceRequest) -> InferenceRequest {
        request.with_model(self.config.large_model.clone())
    }
}

#[async_trait]
impl<E: InferenceEngine + 'static> InferenceEngine for ModelSelector<E> {
    #[instrument(skip(self, request), fields(selected_model))]
    async fn generate(
        &self,
        request: InferenceRequest,
    ) -> Result<InferenceResponse, InferenceError> {
        let request = self.apply_model(request);
        tracing::Span::current().record(
            "selected_model",
            request.model.as_deref().unwrap_or("default"),
        );
        self.engine.generate(request).await
    }

    async fn generate_stream(
        &self,
        request: InferenceRequest,
    ) -> Result<StreamingResponse, InferenceError> {
        let request = self.apply_model(request);
        self.engine.generate_stream(request).await
    }

    async fn health_check(&self) -> Result<bool, InferenceError> {
        self.engine.health_check().await
    }

    async fn list_models(&self) -> Result<Vec<String>, InferenceError> {
        self.engine.list_models().await
    }

    fn default_model(&self) -> String {
        self.engine.default_model()
    }

    fn set_default_model(&self, model_name: &str) {
        self.engine.set_default_model(model_name);
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::*;
    use crate::ports::{InferenceMessage, TokenUsage};

    /// Mock inference engine for testing
    struct MockInferenceEngine {
        call_count: AtomicUsize,
    }

    impl MockInferenceEngine {
        fn new() -> Self {
            Self {
                call_count: AtomicUsize::new(0),
            }
        }
    }

    #[async_trait]
    #[allow(clippy::unnecessary_literal_bound)]
    impl InferenceEngine for MockInferenceEngine {
        async fn generate(
            &self,
            request: InferenceRequest,
        ) -> Result<InferenceResponse, InferenceError> {
            self.call_count.fetch_add(1, Ordering::SeqCst);
            Ok(InferenceResponse {
                content: "Mock response".to_string(),
                model: request.model.unwrap_or_else(|| "default".to_string()),
                usage: Some(TokenUsage {
                    prompt_tokens: 10,
                    completion_tokens: 5,
                    total_tokens: 15,
                }),
                finish_reason: Some("stop".to_string()),
            })
        }

        async fn generate_stream(
            &self,
            _request: InferenceRequest,
        ) -> Result<StreamingResponse, InferenceError> {
            Err(InferenceError::StreamError(
                "Streaming not implemented".to_string(),
            ))
        }

        async fn health_check(&self) -> Result<bool, InferenceError> {
            Ok(true)
        }

        async fn list_models(&self) -> Result<Vec<String>, InferenceError> {
            Ok(vec![
                "qwen2.5-1.5b-instruct".to_string(),
                "qwen2.5-7b-instruct".to_string(),
            ])
        }

        fn default_model(&self) -> String {
            "qwen2.5-1.5b-instruct".to_string()
        }

        fn set_default_model(&self, _model_name: &str) {
            // No-op for tests
        }
    }

    // === Configuration Tests ===

    #[test]
    fn default_config_has_reasonable_values() {
        let config = ModelSelectorConfig::default();
        assert!(!config.small_model.is_empty());
        assert!(!config.large_model.is_empty());
        assert!(config.complexity_word_threshold > 0);
        assert!(!config.complexity_keywords.is_empty());
        assert!(config.small_model_max_prompt_chars > 0);
    }

    #[test]
    fn config_default_models_are_different() {
        let config = ModelSelectorConfig::default();
        assert_ne!(config.small_model, config.large_model);
    }

    // === Complexity Analysis Tests ===

    #[test]
    fn simple_request_is_simple() {
        let engine = Arc::new(MockInferenceEngine::new());
        let selector = ModelSelector::with_defaults(engine);

        let request = InferenceRequest::simple("Hello, how are you?");
        let complexity = selector.analyze_complexity(&request);

        assert_eq!(complexity, TaskComplexity::Simple);
    }

    #[test]
    fn long_prompt_is_complex() {
        let engine = Arc::new(MockInferenceEngine::new());
        let selector = ModelSelector::with_defaults(engine);

        // Create a request longer than small_model_max_prompt_chars
        let long_text = "a".repeat(600);
        let request = InferenceRequest::simple(long_text);
        let complexity = selector.analyze_complexity(&request);

        assert_eq!(complexity, TaskComplexity::Complex);
    }

    #[test]
    fn many_words_is_complex() {
        let engine = Arc::new(MockInferenceEngine::new());
        let config = ModelSelectorConfig {
            complexity_word_threshold: 10,
            small_model_max_prompt_chars: 10000,
            ..Default::default()
        };
        let selector = ModelSelector::new(engine, config);

        let request =
            InferenceRequest::simple("one two three four five six seven eight nine ten eleven");
        let complexity = selector.analyze_complexity(&request);

        assert_eq!(complexity, TaskComplexity::Complex);
    }

    #[test]
    fn analyze_keyword_triggers_complex() {
        let engine = Arc::new(MockInferenceEngine::new());
        let selector = ModelSelector::with_defaults(engine);

        let request = InferenceRequest::simple("Please analyze this data");
        let complexity = selector.analyze_complexity(&request);

        assert_eq!(complexity, TaskComplexity::Complex);
    }

    #[test]
    fn code_keyword_triggers_complex() {
        let engine = Arc::new(MockInferenceEngine::new());
        let selector = ModelSelector::with_defaults(engine);

        let request = InferenceRequest::simple("Write code for a sort algorithm");
        let complexity = selector.analyze_complexity(&request);

        assert_eq!(complexity, TaskComplexity::Complex);
    }

    #[test]
    fn summarize_keyword_triggers_complex() {
        let engine = Arc::new(MockInferenceEngine::new());
        let selector = ModelSelector::with_defaults(engine);

        let request = InferenceRequest::simple("Summarize the article");
        let complexity = selector.analyze_complexity(&request);

        assert_eq!(complexity, TaskComplexity::Complex);
    }

    #[test]
    fn complexity_keywords_are_case_insensitive() {
        let engine = Arc::new(MockInferenceEngine::new());
        let selector = ModelSelector::with_defaults(engine);

        let request = InferenceRequest::simple("ANALYZE this DATA");
        let complexity = selector.analyze_complexity(&request);

        assert_eq!(complexity, TaskComplexity::Complex);
    }

    // === Model Selection Tests ===

    #[test]
    fn simple_complexity_uses_small_model() {
        let engine = Arc::new(MockInferenceEngine::new());
        let selector = ModelSelector::with_defaults(engine);

        let model = selector.select_model(TaskComplexity::Simple);
        assert!(model.contains("1.5b"));
    }

    #[test]
    fn complex_complexity_uses_large_model() {
        let engine = Arc::new(MockInferenceEngine::new());
        let selector = ModelSelector::with_defaults(engine);

        let model = selector.select_model(TaskComplexity::Complex);
        assert!(model.contains("7b"));
    }

    #[test]
    fn apply_model_respects_existing_model() {
        let engine = Arc::new(MockInferenceEngine::new());
        let selector = ModelSelector::with_defaults(engine);

        let request = InferenceRequest::simple("analyze this").with_model("custom-model");
        let modified = selector.apply_model(request);

        assert_eq!(modified.model.as_deref(), Some("custom-model"));
    }

    #[test]
    fn apply_model_sets_model_based_on_complexity() {
        let engine = Arc::new(MockInferenceEngine::new());
        let selector = ModelSelector::with_defaults(engine);

        let simple_request = InferenceRequest::simple("Hello");
        let simple_modified = selector.apply_model(simple_request);
        assert!(simple_modified.model.unwrap().contains("1.5b"));

        let complex_request = InferenceRequest::simple("analyze this data");
        let complex_modified = selector.apply_model(complex_request);
        assert!(complex_modified.model.unwrap().contains("7b"));
    }

    #[test]
    fn force_small_model_overrides_complexity() {
        let engine = Arc::new(MockInferenceEngine::new());
        let selector = ModelSelector::with_defaults(engine);

        // Even a complex request can be forced to small model
        let request = InferenceRequest::simple("analyze this complex data");
        let modified = selector.force_small_model(request);

        assert!(modified.model.unwrap().contains("1.5b"));
    }

    #[test]
    fn force_large_model_overrides_complexity() {
        let engine = Arc::new(MockInferenceEngine::new());
        let selector = ModelSelector::with_defaults(engine);

        // Even a simple request can be forced to large model
        let request = InferenceRequest::simple("Hi");
        let modified = selector.force_large_model(request);

        assert!(modified.model.unwrap().contains("7b"));
    }

    // === Configuration Update Tests ===

    #[test]
    fn config_can_be_updated() {
        let engine = Arc::new(MockInferenceEngine::new());
        let mut selector = ModelSelector::with_defaults(engine);

        let new_config = ModelSelectorConfig {
            small_model: "tiny-model".to_string(),
            large_model: "huge-model".to_string(),
            ..Default::default()
        };
        selector.set_config(new_config);

        assert_eq!(selector.config().small_model, "tiny-model");
        assert_eq!(selector.config().large_model, "huge-model");
    }

    // === Async Tests ===

    #[tokio::test]
    async fn generate_applies_model_selection() {
        let engine = Arc::new(MockInferenceEngine::new());
        let selector = ModelSelector::with_defaults(engine);

        // Simple request
        let simple_request = InferenceRequest::simple("Hello");
        let response = selector.generate(simple_request).await.unwrap();
        assert!(response.model.contains("1.5b"));

        // Complex request
        let complex_request = InferenceRequest::simple("analyze this");
        let response = selector.generate(complex_request).await.unwrap();
        assert!(response.model.contains("7b"));
    }

    #[tokio::test]
    async fn health_check_delegates_to_engine() {
        let engine = Arc::new(MockInferenceEngine::new());
        let selector = ModelSelector::with_defaults(engine);

        let healthy = selector.health_check().await.unwrap();
        assert!(healthy);
    }

    #[tokio::test]
    async fn list_models_delegates_to_engine() {
        let engine = Arc::new(MockInferenceEngine::new());
        let selector = ModelSelector::with_defaults(engine);

        let models = selector.list_models().await.unwrap();
        assert_eq!(models.len(), 2);
    }

    #[test]
    fn default_model_delegates_to_engine() {
        let engine = Arc::new(MockInferenceEngine::new());
        let selector = ModelSelector::with_defaults(engine);

        assert_eq!(selector.default_model(), "qwen2.5-1.5b-instruct");
    }

    // === Display Tests ===

    #[test]
    fn task_complexity_display() {
        assert_eq!(format!("{}", TaskComplexity::Simple), "simple");
        assert_eq!(format!("{}", TaskComplexity::Complex), "complex");
    }

    // === Multi-message Tests ===

    #[test]
    fn analyzes_all_messages_in_request() {
        let engine = Arc::new(MockInferenceEngine::new());
        let selector = ModelSelector::with_defaults(engine);

        let request = InferenceRequest {
            messages: vec![
                InferenceMessage {
                    role: "system".to_string(),
                    content: "You are helpful".to_string(),
                },
                InferenceMessage {
                    role: "user".to_string(),
                    content: "Please analyze this".to_string(),
                },
            ],
            model: None,
            max_tokens: None,
            temperature: None,
            stream: false,
        };

        let complexity = selector.analyze_complexity(&request);
        assert_eq!(complexity, TaskComplexity::Complex);
    }

    #[test]
    fn combined_message_length_affects_complexity() {
        let engine = Arc::new(MockInferenceEngine::new());
        let config = ModelSelectorConfig {
            small_model_max_prompt_chars: 30,
            complexity_keywords: vec![],
            ..Default::default()
        };
        let selector = ModelSelector::new(engine, config);

        // Each message is short, but combined they exceed threshold
        let request = InferenceRequest {
            messages: vec![
                InferenceMessage {
                    role: "system".to_string(),
                    content: "Short system".to_string(),
                },
                InferenceMessage {
                    role: "user".to_string(),
                    content: "Short user message".to_string(),
                },
            ],
            model: None,
            max_tokens: None,
            temperature: None,
            stream: false,
        };

        let complexity = selector.analyze_complexity(&request);
        assert_eq!(complexity, TaskComplexity::Complex);
    }
}
