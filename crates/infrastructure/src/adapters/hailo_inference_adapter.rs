//! Hailo inference adapter - Implements InferencePort using ai_core

use std::time::Instant;

use ai_core::{HailoInferenceEngine, InferenceConfig, InferenceEngine, InferenceRequest};
use application::{
    error::ApplicationError,
    ports::{InferencePort, InferenceResult},
};
use async_trait::async_trait;
use domain::Conversation;
use tracing::{debug, instrument};

/// Adapter for Hailo-10H inference
#[derive(Debug)]
pub struct HailoInferenceAdapter {
    engine: HailoInferenceEngine,
    system_prompt: Option<String>,
}

impl HailoInferenceAdapter {
    /// Create a new adapter with the given configuration
    pub fn new(config: InferenceConfig) -> Result<Self, ApplicationError> {
        let engine = HailoInferenceEngine::new(config)
            .map_err(|e| ApplicationError::Inference(e.to_string()))?;

        Ok(Self {
            engine,
            system_prompt: None,
        })
    }

    /// Create with default Hailo-10H configuration
    pub fn with_defaults() -> Result<Self, ApplicationError> {
        Self::new(InferenceConfig::hailo_qwen())
    }

    /// Set the default system prompt
    pub fn with_system_prompt(mut self, prompt: impl Into<String>) -> Self {
        self.system_prompt = Some(prompt.into());
        self
    }

    /// Convert ai_core error to application error
    fn map_error(e: ai_core::InferenceError) -> ApplicationError {
        match e {
            ai_core::InferenceError::RateLimited => ApplicationError::RateLimited,
            ai_core::InferenceError::ConnectionFailed(msg) => {
                ApplicationError::ExternalService(format!("Hailo connection failed: {msg}"))
            },
            ai_core::InferenceError::Timeout(ms) => {
                ApplicationError::ExternalService(format!("Inference timeout after {ms}ms"))
            },
            other => ApplicationError::Inference(other.to_string()),
        }
    }
}

#[async_trait]
impl InferencePort for HailoInferenceAdapter {
    #[instrument(skip(self, message), fields(message_len = message.len()))]
    async fn generate(&self, message: &str) -> Result<InferenceResult, ApplicationError> {
        let start = Instant::now();

        let request = match &self.system_prompt {
            Some(system) => InferenceRequest::with_system(system, message),
            None => InferenceRequest::simple(message),
        };

        let response = self
            .engine
            .generate(request)
            .await
            .map_err(Self::map_error)?;

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

    #[instrument(skip(self, conversation), fields(conv_id = %conversation.id))]
    async fn generate_with_context(
        &self,
        conversation: &Conversation,
    ) -> Result<InferenceResult, ApplicationError> {
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
                content: system.to_string(),
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

        let response = self
            .engine
            .generate(request)
            .await
            .map_err(Self::map_error)?;

        let latency_ms = start.elapsed().as_millis() as u64;

        Ok(InferenceResult {
            content: response.content,
            model: response.model,
            tokens_used: response.usage.map(|u| u.total_tokens),
            latency_ms,
        })
    }

    #[instrument(skip(self, system_prompt, message))]
    async fn generate_with_system(
        &self,
        system_prompt: &str,
        message: &str,
    ) -> Result<InferenceResult, ApplicationError> {
        let start = Instant::now();

        let request = InferenceRequest::with_system(system_prompt, message);
        let response = self
            .engine
            .generate(request)
            .await
            .map_err(Self::map_error)?;

        let latency_ms = start.elapsed().as_millis() as u64;

        Ok(InferenceResult {
            content: response.content,
            model: response.model,
            tokens_used: response.usage.map(|u| u.total_tokens),
            latency_ms,
        })
    }

    async fn is_healthy(&self) -> bool {
        self.engine.health_check().await.unwrap_or(false)
    }

    fn current_model(&self) -> &str {
        self.engine.default_model()
    }
}
