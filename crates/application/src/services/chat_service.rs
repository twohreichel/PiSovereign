//! Chat service - Simple conversation handling

use std::{fmt, sync::Arc, time::Instant};

use domain::{ChatMessage, Conversation, MessageMetadata};
use tracing::{debug, instrument};

use crate::{
    error::ApplicationError,
    ports::{InferencePort, InferenceStream},
};

/// Service for handling chat conversations
pub struct ChatService {
    inference: Arc<dyn InferencePort>,
    system_prompt: Option<String>,
}

impl fmt::Debug for ChatService {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ChatService")
            .field("system_prompt", &self.system_prompt)
            .finish_non_exhaustive()
    }
}

impl ChatService {
    /// Create a new chat service
    pub fn new(inference: Arc<dyn InferencePort>) -> Self {
        Self {
            inference,
            system_prompt: None,
        }
    }

    /// Create a chat service with a system prompt
    pub fn with_system_prompt(
        inference: Arc<dyn InferencePort>,
        prompt: impl Into<String>,
    ) -> Self {
        Self {
            inference,
            system_prompt: Some(prompt.into()),
        }
    }

    /// Handle a single chat message (stateless)
    #[instrument(skip(self, message), fields(message_len = message.len()))]
    pub async fn chat(&self, message: &str) -> Result<ChatMessage, ApplicationError> {
        let start = Instant::now();

        let result = match &self.system_prompt {
            Some(system) => self.inference.generate_with_system(system, message).await?,
            None => self.inference.generate(message).await?,
        };

        let latency = start.elapsed().as_millis() as u64;

        debug!(
            model = %result.model,
            tokens = ?result.tokens_used,
            latency_ms = latency,
            "Chat response generated"
        );

        let response = ChatMessage::assistant(&result.content).with_metadata(MessageMetadata {
            model: Some(result.model),
            tokens: result.tokens_used,
            latency_ms: Some(latency),
        });

        Ok(response)
    }

    /// Continue a conversation
    #[instrument(skip(self, conversation), fields(conv_id = %conversation.id, msg_count = conversation.message_count()))]
    pub async fn continue_conversation(
        &self,
        conversation: &Conversation,
    ) -> Result<ChatMessage, ApplicationError> {
        let start = Instant::now();

        let result = self.inference.generate_with_context(conversation).await?;

        let latency = start.elapsed().as_millis() as u64;

        debug!(
            model = %result.model,
            tokens = ?result.tokens_used,
            latency_ms = latency,
            "Conversation response generated"
        );

        let response = ChatMessage::assistant(&result.content).with_metadata(MessageMetadata {
            model: Some(result.model),
            tokens: result.tokens_used,
            latency_ms: Some(latency),
        });

        Ok(response)
    }

    /// Check if the underlying inference is healthy
    pub async fn is_healthy(&self) -> bool {
        self.inference.is_healthy().await
    }

    /// Get the current model name
    pub fn current_model(&self) -> String {
        self.inference.current_model()
    }

    /// List available models on the system
    pub async fn list_available_models(&self) -> Result<Vec<String>, ApplicationError> {
        self.inference.list_available_models().await
    }

    /// Handle a streaming chat message (stateless)
    ///
    /// Returns a stream of chunks that can be forwarded directly to SSE
    #[instrument(skip(self, message), fields(message_len = message.len()))]
    pub async fn chat_stream(&self, message: &str) -> Result<InferenceStream, ApplicationError> {
        match &self.system_prompt {
            Some(system) => {
                self.inference
                    .generate_stream_with_system(system, message)
                    .await
            },
            None => self.inference.generate_stream(message).await,
        }
    }
}
#[cfg(test)]
mod tests {
    use mockall::mock;

    use super::*;
    use crate::ports::InferenceResult;

    mock! {
        pub InferenceEngine {}

        #[async_trait::async_trait]
        impl InferencePort for InferenceEngine {
            async fn generate(&self, message: &str) -> Result<InferenceResult, ApplicationError>;
            async fn generate_with_context(&self, conversation: &Conversation) -> Result<InferenceResult, ApplicationError>;
            async fn generate_with_system(&self, system_prompt: &str, message: &str) -> Result<InferenceResult, ApplicationError>;
            async fn generate_stream(&self, message: &str) -> Result<InferenceStream, ApplicationError>;
            async fn generate_stream_with_system(&self, system_prompt: &str, message: &str) -> Result<InferenceStream, ApplicationError>;
            async fn is_healthy(&self) -> bool;
            fn current_model(&self) -> String;
            async fn list_available_models(&self) -> Result<Vec<String>, ApplicationError>;
            async fn switch_model(&self, model_name: &str) -> Result<(), ApplicationError>;
        }
    }

    fn mock_inference_result(content: &str) -> InferenceResult {
        InferenceResult {
            content: content.to_string(),
            model: "test-model".to_string(),
            tokens_used: Some(42),
            latency_ms: 100,
        }
    }

    #[test]
    fn chat_service_new() {
        let mock = MockInferenceEngine::new();
        let service = ChatService::new(Arc::new(mock));
        let debug = format!("{service:?}");
        assert!(debug.contains("ChatService"));
    }

    #[test]
    fn chat_service_with_system_prompt() {
        let mock = MockInferenceEngine::new();
        let service = ChatService::with_system_prompt(Arc::new(mock), "You are helpful");
        let debug = format!("{service:?}");
        assert!(debug.contains("ChatService"));
    }

    #[test]
    fn chat_service_debug() {
        let mock = MockInferenceEngine::new();
        let service = ChatService::new(Arc::new(mock));
        let debug = format!("{service:?}");
        assert!(debug.contains("system_prompt"));
    }

    #[tokio::test]
    async fn chat_without_system_prompt() {
        let mut mock = MockInferenceEngine::new();
        mock.expect_generate()
            .returning(|_| Ok(mock_inference_result("Hello there!")));

        let service = ChatService::new(Arc::new(mock));
        let result = service.chat("Hi").await.unwrap();

        assert_eq!(result.content, "Hello there!");
    }

    #[tokio::test]
    async fn chat_with_system_prompt() {
        let mut mock = MockInferenceEngine::new();
        mock.expect_generate_with_system()
            .returning(|_, _| Ok(mock_inference_result("System response")));

        let service = ChatService::with_system_prompt(Arc::new(mock), "Be nice");
        let result = service.chat("Hello").await.unwrap();

        assert_eq!(result.content, "System response");
    }

    #[tokio::test]
    async fn chat_response_has_metadata() {
        let mut mock = MockInferenceEngine::new();
        mock.expect_generate()
            .returning(|_| Ok(mock_inference_result("Response")));

        let service = ChatService::new(Arc::new(mock));
        let result = service.chat("Test").await.unwrap();

        let metadata = result.metadata.unwrap();
        assert_eq!(metadata.model, Some("test-model".to_string()));
        assert_eq!(metadata.tokens, Some(42));
    }

    #[tokio::test]
    async fn continue_conversation() {
        let mut mock = MockInferenceEngine::new();
        mock.expect_generate_with_context()
            .returning(|_| Ok(mock_inference_result("Continued")));

        let service = ChatService::new(Arc::new(mock));
        let conv = Conversation::new();
        let result = service.continue_conversation(&conv).await.unwrap();

        assert_eq!(result.content, "Continued");
    }

    #[tokio::test]
    async fn continue_conversation_has_metadata() {
        let mut mock = MockInferenceEngine::new();
        mock.expect_generate_with_context()
            .returning(|_| Ok(mock_inference_result("Result")));

        let service = ChatService::new(Arc::new(mock));
        let conv = Conversation::new();
        let result = service.continue_conversation(&conv).await.unwrap();

        assert!(result.metadata.is_some());
    }

    #[tokio::test]
    async fn is_healthy_true() {
        let mut mock = MockInferenceEngine::new();
        mock.expect_is_healthy().returning(|| true);

        let service = ChatService::new(Arc::new(mock));
        assert!(service.is_healthy().await);
    }

    #[tokio::test]
    async fn is_healthy_false() {
        let mut mock = MockInferenceEngine::new();
        mock.expect_is_healthy().returning(|| false);

        let service = ChatService::new(Arc::new(mock));
        assert!(!service.is_healthy().await);
    }

    #[tokio::test]
    async fn current_model() {
        let mut mock = MockInferenceEngine::new();
        mock.expect_current_model()
            .returning(|| "qwen2.5-1.5b".to_string());

        let service = ChatService::new(Arc::new(mock));
        assert_eq!(service.current_model(), "qwen2.5-1.5b");
    }

    #[tokio::test]
    async fn chat_error_propagation() {
        let mut mock = MockInferenceEngine::new();
        mock.expect_generate()
            .returning(|_| Err(ApplicationError::Inference("Failed".to_string())));

        let service = ChatService::new(Arc::new(mock));
        let result = service.chat("Test").await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn continue_conversation_error_propagation() {
        let mut mock = MockInferenceEngine::new();
        mock.expect_generate_with_context()
            .returning(|_| Err(ApplicationError::RateLimited));

        let service = ChatService::new(Arc::new(mock));
        let conv = Conversation::new();
        let result = service.continue_conversation(&conv).await;

        assert!(result.is_err());
    }
}
