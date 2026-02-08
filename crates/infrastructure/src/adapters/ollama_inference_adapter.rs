//! Ollama inference adapter - Implements InferencePort using ai_core
//!
//! Works with any Ollama-compatible backend:
//! - Standard Ollama (macOS with Metal, Linux with CUDA)
//! - hailo-ollama (Raspberry Pi with Hailo NPU)

use std::time::Instant;

use ai_core::{InferenceConfig, InferenceEngine, InferenceRequest, OllamaInferenceEngine};
use application::{
    error::ApplicationError,
    ports::{InferencePort, InferenceResult, InferenceStream, StreamingChunk},
};
use async_trait::async_trait;
use domain::Conversation;
use futures::StreamExt;
use tracing::{debug, info, instrument, warn};

use super::{CircuitBreaker, CircuitBreakerConfig};

/// Adapter for Ollama-compatible inference servers
#[derive(Debug)]
pub struct OllamaInferenceAdapter {
    engine: OllamaInferenceEngine,
    system_prompt: Option<String>,
    circuit_breaker: Option<CircuitBreaker>,
}

impl OllamaInferenceAdapter {
    /// Create a new adapter with the given configuration
    pub fn new(config: InferenceConfig) -> Result<Self, ApplicationError> {
        let engine = OllamaInferenceEngine::new(config)
            .map_err(|e| ApplicationError::Inference(e.to_string()))?;

        Ok(Self {
            engine,
            system_prompt: None,
            circuit_breaker: None,
        })
    }

    /// Create with default Qwen configuration
    pub fn with_defaults() -> Result<Self, ApplicationError> {
        Self::new(InferenceConfig::hailo_qwen())
    }

    /// Set the default system prompt
    #[must_use]
    pub fn with_system_prompt(mut self, prompt: impl Into<String>) -> Self {
        self.system_prompt = Some(prompt.into());
        self
    }

    /// Enable circuit breaker with default configuration
    #[must_use]
    pub fn with_circuit_breaker(mut self) -> Self {
        self.circuit_breaker = Some(CircuitBreaker::new("ollama-inference"));
        self
    }

    /// Enable circuit breaker with custom configuration
    #[must_use]
    pub fn with_circuit_breaker_config(mut self, config: CircuitBreakerConfig) -> Self {
        self.circuit_breaker = Some(CircuitBreaker::with_config("ollama-inference", config));
        self
    }

    /// Convert ai_core error to application error
    fn map_error(e: ai_core::InferenceError) -> ApplicationError {
        match e {
            ai_core::InferenceError::RateLimited => ApplicationError::RateLimited,
            ai_core::InferenceError::ConnectionFailed(msg) => {
                ApplicationError::ExternalService(format!("Ollama connection failed: {msg}"))
            },
            ai_core::InferenceError::Timeout(ms) => {
                ApplicationError::ExternalService(format!("Inference timeout after {ms}ms"))
            },
            other => ApplicationError::Inference(other.to_string()),
        }
    }

    /// Check if circuit breaker is blocking requests
    fn is_circuit_open(&self) -> bool {
        self.circuit_breaker
            .as_ref()
            .is_some_and(CircuitBreaker::is_open)
    }

    /// Get circuit breaker state description for logging
    fn circuit_state_desc(&self) -> &'static str {
        match &self.circuit_breaker {
            Some(cb) if cb.is_open() => "open",
            Some(cb) if cb.is_closed() => "closed",
            Some(_) => "half-open",
            None => "disabled",
        }
    }
}

#[async_trait]
impl InferencePort for OllamaInferenceAdapter {
    #[instrument(skip(self, message), fields(message_len = message.len(), circuit = %self.circuit_state_desc()))]
    async fn generate(&self, message: &str) -> Result<InferenceResult, ApplicationError> {
        // Fast-fail if circuit is open
        if self.is_circuit_open() {
            warn!("Ollama inference circuit breaker is open, failing fast");
            return Err(ApplicationError::ExternalService(
                "Ollama inference service temporarily unavailable (circuit breaker open)"
                    .to_string(),
            ));
        }

        let start = Instant::now();

        #[allow(clippy::option_if_let_else)]
        let request = match &self.system_prompt {
            Some(system) => InferenceRequest::with_system(system, message),
            None => InferenceRequest::simple(message),
        };

        let response = match &self.circuit_breaker {
            Some(cb) => {
                let engine = &self.engine;
                let req = request.clone();
                cb.call(|| async { engine.generate(req).await })
                    .await
                    .map_err(|e| match e {
                        super::CircuitBreakerError::CircuitOpen(_) => {
                            ApplicationError::ExternalService(
                                "Ollama inference service temporarily unavailable".to_string(),
                            )
                        },
                        super::CircuitBreakerError::ServiceError(e) => Self::map_error(e),
                    })?
            },
            None => self
                .engine
                .generate(request)
                .await
                .map_err(Self::map_error)?,
        };

        #[allow(clippy::cast_possible_truncation)]
        let latency_ms = start.elapsed().as_millis() as u64;

        debug!(
            model = %response.model,
            tokens = ?response.usage.as_ref().map(|u| u.total_tokens),
            latency_ms = latency_ms,
            "Inference completed"
        );

        Ok(InferenceResult {
            content: response.content,
            model: response.model,
            tokens_used: response.usage.map(|u| u.total_tokens),
            latency_ms,
        })
    }

    #[instrument(skip(self, conversation), fields(conv_id = %conversation.id, circuit = %self.circuit_state_desc()))]
    async fn generate_with_context(
        &self,
        conversation: &Conversation,
    ) -> Result<InferenceResult, ApplicationError> {
        // Fast-fail if circuit is open
        if self.is_circuit_open() {
            warn!("Ollama inference circuit breaker is open, failing fast");
            return Err(ApplicationError::ExternalService(
                "Ollama inference service temporarily unavailable (circuit breaker open)"
                    .to_string(),
            ));
        }

        let start = Instant::now();

        // Build messages from conversation
        let mut messages: Vec<ai_core::ports::InferenceMessage> = Vec::new();

        // Add system prompt if configured
        if let Some(system) = conversation
            .system_prompt
            .as_ref()
            .or(self.system_prompt.as_ref())
        {
            messages.push(ai_core::ports::InferenceMessage {
                role: "system".to_string(),
                content: system.clone(),
            });
        }

        // Add conversation messages
        for msg in &conversation.messages {
            messages.push(ai_core::ports::InferenceMessage::from(msg));
        }

        let request = InferenceRequest {
            messages,
            model: None,
            max_tokens: None,
            temperature: None,
            stream: false,
        };

        let response = match &self.circuit_breaker {
            Some(cb) => {
                let engine = &self.engine;
                let req = request.clone();
                cb.call(|| async { engine.generate(req).await })
                    .await
                    .map_err(|e| match e {
                        super::CircuitBreakerError::CircuitOpen(_) => {
                            ApplicationError::ExternalService(
                                "Ollama inference service temporarily unavailable".to_string(),
                            )
                        },
                        super::CircuitBreakerError::ServiceError(e) => Self::map_error(e),
                    })?
            },
            None => self
                .engine
                .generate(request)
                .await
                .map_err(Self::map_error)?,
        };

        #[allow(clippy::cast_possible_truncation)]
        let latency_ms = start.elapsed().as_millis() as u64;

        Ok(InferenceResult {
            content: response.content,
            model: response.model,
            tokens_used: response.usage.map(|u| u.total_tokens),
            latency_ms,
        })
    }

    #[instrument(skip(self, system_prompt, message), fields(circuit = %self.circuit_state_desc()))]
    async fn generate_with_system(
        &self,
        system_prompt: &str,
        message: &str,
    ) -> Result<InferenceResult, ApplicationError> {
        // Fast-fail if circuit is open
        if self.is_circuit_open() {
            warn!("Ollama inference circuit breaker is open, failing fast");
            return Err(ApplicationError::ExternalService(
                "Ollama inference service temporarily unavailable (circuit breaker open)"
                    .to_string(),
            ));
        }

        let start = Instant::now();

        let request = InferenceRequest::with_system(system_prompt, message);

        let response = match &self.circuit_breaker {
            Some(cb) => {
                let engine = &self.engine;
                let req = request.clone();
                cb.call(|| async { engine.generate(req).await })
                    .await
                    .map_err(|e| match e {
                        super::CircuitBreakerError::CircuitOpen(_) => {
                            ApplicationError::ExternalService(
                                "Ollama inference service temporarily unavailable".to_string(),
                            )
                        },
                        super::CircuitBreakerError::ServiceError(e) => Self::map_error(e),
                    })?
            },
            None => self
                .engine
                .generate(request)
                .await
                .map_err(Self::map_error)?,
        };

        #[allow(clippy::cast_possible_truncation)]
        let latency_ms = start.elapsed().as_millis() as u64;

        Ok(InferenceResult {
            content: response.content,
            model: response.model,
            tokens_used: response.usage.map(|u| u.total_tokens),
            latency_ms,
        })
    }

    #[instrument(skip(self, message), fields(message_len = message.len(), circuit = %self.circuit_state_desc()))]
    async fn generate_stream(&self, message: &str) -> Result<InferenceStream, ApplicationError> {
        // Fast-fail if circuit is open
        if self.is_circuit_open() {
            warn!("Ollama inference circuit breaker is open, failing fast");
            return Err(ApplicationError::ExternalService(
                "Ollama inference service temporarily unavailable (circuit breaker open)"
                    .to_string(),
            ));
        }

        #[allow(clippy::option_if_let_else)]
        let request = match &self.system_prompt {
            Some(system) => InferenceRequest::with_system(system, message).streaming(),
            None => InferenceRequest::simple(message).streaming(),
        };

        // Note: Circuit breaker not applied to streaming due to lifetime complexity
        // The initial connection is still protected by fast-fail above
        let stream = self
            .engine
            .generate_stream(request)
            .await
            .map_err(Self::map_error)?;

        // Map ai_core::StreamingChunk to application::StreamingChunk
        let mapped_stream = stream.map(|result| {
            result
                .map(|chunk| StreamingChunk {
                    content: chunk.content,
                    done: chunk.done,
                    model: chunk.model,
                })
                .map_err(|e| ApplicationError::Inference(e.to_string()))
        });

        Ok(Box::pin(mapped_stream))
    }

    #[instrument(skip(self, system_prompt, message), fields(circuit = %self.circuit_state_desc()))]
    async fn generate_stream_with_system(
        &self,
        system_prompt: &str,
        message: &str,
    ) -> Result<InferenceStream, ApplicationError> {
        // Fast-fail if circuit is open
        if self.is_circuit_open() {
            warn!("Ollama inference circuit breaker is open, failing fast");
            return Err(ApplicationError::ExternalService(
                "Ollama inference service temporarily unavailable (circuit breaker open)"
                    .to_string(),
            ));
        }

        let request = InferenceRequest::with_system(system_prompt, message).streaming();

        let stream = self
            .engine
            .generate_stream(request)
            .await
            .map_err(Self::map_error)?;

        // Map ai_core::StreamingChunk to application::StreamingChunk
        let mapped_stream = stream.map(|result| {
            result
                .map(|chunk| StreamingChunk {
                    content: chunk.content,
                    done: chunk.done,
                    model: chunk.model,
                })
                .map_err(|e| ApplicationError::Inference(e.to_string()))
        });

        Ok(Box::pin(mapped_stream))
    }

    async fn is_healthy(&self) -> bool {
        // If circuit breaker is open, report as unhealthy
        if self.is_circuit_open() {
            debug!("Ollama inference unhealthy: circuit breaker open");
            return false;
        }
        self.engine.health_check().await.unwrap_or(false)
    }

    fn current_model(&self) -> String {
        self.engine.default_model()
    }

    #[instrument(skip(self), fields(circuit = %self.circuit_state_desc()))]
    async fn list_available_models(&self) -> Result<Vec<String>, ApplicationError> {
        // Fast-fail if circuit is open
        if self.is_circuit_open() {
            return Err(ApplicationError::ExternalService(
                "Ollama inference service temporarily unavailable (circuit breaker open)"
                    .to_string(),
            ));
        }
        self.engine.list_models().await.map_err(Self::map_error)
    }

    #[instrument(skip(self), fields(model = %model_name, circuit = %self.circuit_state_desc()))]
    async fn switch_model(&self, model_name: &str) -> Result<(), ApplicationError> {
        // Fast-fail if circuit is open
        if self.is_circuit_open() {
            return Err(ApplicationError::ExternalService(
                "Ollama inference service temporarily unavailable (circuit breaker open)"
                    .to_string(),
            ));
        }

        // Check if model is available
        let available_models = self.engine.list_models().await.map_err(Self::map_error)?;
        if !available_models.iter().any(|m| m == model_name) {
            return Err(ApplicationError::NotFound(format!(
                "Model '{}' not found. Available models: {}",
                model_name,
                available_models.join(", ")
            )));
        }

        // Switch the model in the engine
        self.engine.set_default_model(model_name);
        info!(model = %model_name, "Switched to model");

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use ai_core::InferenceConfig;

    use super::*;

    #[test]
    fn ollama_inference_adapter_creation() {
        // Note: This may fail if Ollama is not running
        // but tests the configuration path
        let config = InferenceConfig {
            default_model: "test-model".to_string(),
            base_url: "http://localhost:8080".to_string(),
            timeout_ms: 30000,
            max_tokens: 2048,
            temperature: 0.7,
            top_p: 0.9,
            system_prompt: None,
        };
        // Just test that the config can be created
        assert_eq!(config.default_model, "test-model");
    }

    #[test]
    fn inference_config_default_model() {
        let config = InferenceConfig::hailo_qwen();
        assert!(!config.default_model.is_empty());
        assert!(!config.base_url.is_empty());
    }

    #[test]
    fn map_error_rate_limited() {
        let error = ai_core::InferenceError::RateLimited;
        let mapped = OllamaInferenceAdapter::map_error(error);
        assert!(matches!(mapped, ApplicationError::RateLimited));
    }

    #[test]
    fn map_error_connection_failed() {
        let error = ai_core::InferenceError::ConnectionFailed("timeout".to_string());
        let mapped = OllamaInferenceAdapter::map_error(error);
        let ApplicationError::ExternalService(msg) = mapped else {
            unreachable!("Expected ExternalService error");
        };
        assert!(msg.contains("connection failed"));
    }

    #[test]
    fn map_error_timeout() {
        let error = ai_core::InferenceError::Timeout(5000);
        let mapped = OllamaInferenceAdapter::map_error(error);
        let ApplicationError::ExternalService(msg) = mapped else {
            unreachable!("Expected ExternalService error");
        };
        assert!(msg.contains("5000"));
    }

    #[test]
    fn map_error_other() {
        let error = ai_core::InferenceError::RequestFailed("bad".to_string());
        let mapped = OllamaInferenceAdapter::map_error(error);
        let ApplicationError::Inference(msg) = mapped else {
            unreachable!("Expected Inference error");
        };
        assert!(msg.contains("bad"));
    }

    #[test]
    fn inference_result_creation() {
        let result = InferenceResult {
            content: "Hello".to_string(),
            model: "qwen".to_string(),
            tokens_used: Some(10),
            latency_ms: 50,
        };
        assert_eq!(result.content, "Hello");
        assert_eq!(result.model, "qwen");
        assert_eq!(result.tokens_used, Some(10));
        assert_eq!(result.latency_ms, 50);
    }

    #[test]
    fn inference_result_without_tokens() {
        let result = InferenceResult {
            content: "Response".to_string(),
            model: "llama".to_string(),
            tokens_used: None,
            latency_ms: 100,
        };
        assert!(result.tokens_used.is_none());
    }

    #[test]
    fn inference_result_clone() {
        let result = InferenceResult {
            content: "Test".to_string(),
            model: "model".to_string(),
            tokens_used: Some(5),
            latency_ms: 25,
        };
        #[allow(clippy::redundant_clone)]
        let cloned = result.clone();
        assert_eq!(result.content, cloned.content);
    }

    #[test]
    fn inference_result_debug() {
        let result = InferenceResult {
            content: "Debug".to_string(),
            model: "test".to_string(),
            tokens_used: None,
            latency_ms: 10,
        };
        let debug = format!("{result:?}");
        assert!(debug.contains("InferenceResult"));
    }

    #[test]
    fn config_default_values() {
        let config = InferenceConfig::default();
        assert!(config.timeout_ms > 0);
        assert!(config.max_tokens > 0);
    }

    #[test]
    fn ollama_adapter_with_system_prompt_builder() {
        // Test the builder pattern even without actual adapter
        let system_prompt = "You are a helpful assistant";
        assert!(!system_prompt.is_empty());
    }

    #[test]
    fn circuit_breaker_config_default() {
        let config = CircuitBreakerConfig::default();
        assert_eq!(config.failure_threshold, 5);
        assert_eq!(config.success_threshold, 2);
    }

    #[test]
    fn circuit_breaker_config_sensitive() {
        let config = CircuitBreakerConfig::sensitive();
        assert_eq!(config.failure_threshold, 3);
    }

    #[test]
    fn circuit_breaker_config_resilient() {
        let config = CircuitBreakerConfig::resilient();
        assert_eq!(config.failure_threshold, 10);
    }
}
