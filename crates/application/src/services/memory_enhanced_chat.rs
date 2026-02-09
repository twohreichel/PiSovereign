//! Memory-enhanced chat service
//!
//! Wraps ChatService with automatic memory retrieval (RAG) and learning.
//! - Retrieves relevant context before generating responses
//! - Stores interactions as memories after responses

use std::sync::Arc;

use domain::{ChatMessage, Conversation, Memory, MemoryType, UserId};
use tracing::{debug, instrument, warn};

use crate::{
    error::ApplicationError,
    ports::{EmbeddingPort, EncryptionPort, InferencePort, MemoryStore, SimilarMemory},
    services::MemoryService,
};

/// Configuration for memory-enhanced chat
#[derive(Debug, Clone)]
pub struct MemoryEnhancedChatConfig {
    /// Whether to automatically retrieve relevant context (RAG)
    pub enable_rag: bool,
    /// Whether to automatically learn from interactions
    pub enable_learning: bool,
    /// System prompt prefix (memory context is injected after this)
    pub system_prompt: Option<String>,
    /// Minimum message length to consider for learning
    pub min_learning_length: usize,
    /// Default importance for learned memories
    pub default_importance: f32,
}

impl Default for MemoryEnhancedChatConfig {
    fn default() -> Self {
        Self {
            enable_rag: true,
            enable_learning: true,
            system_prompt: None,
            min_learning_length: 20,
            default_importance: 0.5,
        }
    }
}

/// Chat service enhanced with memory capabilities
///
/// Provides automatic context retrieval (RAG) and learning from interactions.
///
/// # Examples
///
/// ```ignore
/// let chat = MemoryEnhancedChat::new(
///     inference,
///     memory_service,
///     MemoryEnhancedChatConfig::default(),
/// );
///
/// // Chat with automatic RAG and learning
/// let response = chat.chat(&user_id, "What is the capital of France?").await?;
/// ```
pub struct MemoryEnhancedChat<S, E, C>
where
    S: MemoryStore,
    E: EmbeddingPort,
    C: EncryptionPort,
{
    inference: Arc<dyn InferencePort>,
    memory_service: MemoryService<S, E, C>,
    config: MemoryEnhancedChatConfig,
}

impl<S, E, C> Clone for MemoryEnhancedChat<S, E, C>
where
    S: MemoryStore,
    E: EmbeddingPort,
    C: EncryptionPort,
{
    fn clone(&self) -> Self {
        Self {
            inference: Arc::clone(&self.inference),
            memory_service: self.memory_service.clone(),
            config: self.config.clone(),
        }
    }
}

impl<S, E, C> std::fmt::Debug for MemoryEnhancedChat<S, E, C>
where
    S: MemoryStore,
    E: EmbeddingPort,
    C: EncryptionPort,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MemoryEnhancedChat")
            .field("config", &self.config)
            .finish_non_exhaustive()
    }
}

impl<S, E, C> MemoryEnhancedChat<S, E, C>
where
    S: MemoryStore,
    E: EmbeddingPort,
    C: EncryptionPort,
{
    /// Create a new memory-enhanced chat service
    pub fn new(
        inference: Arc<dyn InferencePort>,
        memory_service: MemoryService<S, E, C>,
        config: MemoryEnhancedChatConfig,
    ) -> Self {
        Self {
            inference,
            memory_service,
            config,
        }
    }

    /// Chat with memory-enhanced context
    ///
    /// 1. Retrieves relevant context from memory (RAG)
    /// 2. Generates response with injected context
    /// 3. Stores the interaction as a memory
    #[instrument(skip(self, message), fields(user_id = %user_id, message_len = message.len()))]
    pub async fn chat(
        &self,
        user_id: &UserId,
        message: &str,
    ) -> Result<ChatMessage, ApplicationError> {
        // Step 1: Retrieve relevant context (RAG)
        let context = if self.config.enable_rag {
            self.retrieve_context(user_id, message).await?
        } else {
            Vec::new()
        };

        // Step 2: Build system prompt with memory context
        let system_prompt = self.build_system_prompt(&context);

        // Step 3: Generate response
        let response = self
            .inference
            .generate_with_system(&system_prompt, message)
            .await?;

        let response_message = ChatMessage::assistant(&response.content);

        // Step 4: Learn from the interaction
        if self.config.enable_learning {
            self.learn_from_interaction(user_id, message, &response.content)
                .await?;
        }

        debug!(
            context_count = context.len(),
            response_len = response.content.len(),
            "Memory-enhanced chat completed"
        );

        Ok(response_message)
    }

    /// Chat within a conversation context with memory enhancement
    #[instrument(skip(self, conversation, message), fields(user_id = %user_id, conv_id = %conversation.id))]
    pub async fn chat_in_conversation(
        &self,
        user_id: &UserId,
        conversation: &Conversation,
        message: &str,
    ) -> Result<ChatMessage, ApplicationError> {
        // Retrieve context
        let context = if self.config.enable_rag {
            self.retrieve_context(user_id, message).await?
        } else {
            Vec::new()
        };

        // Build enhanced conversation with memory context
        let mut enhanced_conv = conversation.clone();

        // Add memory context as a system message if we have relevant memories
        if !context.is_empty() {
            let context_text = MemoryService::<S, E, C>::format_context_for_prompt(&context);
            let system_with_context = match &self.config.system_prompt {
                Some(s) => format!("{s}\n\n{context_text}"),
                None => context_text,
            };
            enhanced_conv.system_prompt = Some(system_with_context);
        }

        // Add the user message
        let user_msg = ChatMessage::user(message);
        enhanced_conv.add_message(user_msg);

        // Generate response
        let response = self.inference.generate_with_context(&enhanced_conv).await?;
        let response_message = ChatMessage::assistant(&response.content);

        // Learn from interaction
        if self.config.enable_learning {
            self.learn_from_interaction(user_id, message, &response.content)
                .await?;
        }

        Ok(response_message)
    }

    /// Retrieve relevant context for a query
    async fn retrieve_context(
        &self,
        user_id: &UserId,
        query: &str,
    ) -> Result<Vec<SimilarMemory>, ApplicationError> {
        self.memory_service.retrieve_context(user_id, query).await
    }

    /// Build system prompt with memory context
    fn build_system_prompt(&self, memories: &[SimilarMemory]) -> String {
        let base = self
            .config
            .system_prompt
            .clone()
            .unwrap_or_else(|| "You are a helpful AI assistant.".to_string());

        if memories.is_empty() {
            return base;
        }

        let context = MemoryService::<S, E, C>::format_context_for_prompt(memories);
        format!("{base}\n\n{context}")
    }

    /// Learn from a user-assistant interaction
    async fn learn_from_interaction(
        &self,
        user_id: &UserId,
        question: &str,
        answer: &str,
    ) -> Result<(), ApplicationError> {
        // Only learn from meaningful interactions
        if question.len() < self.config.min_learning_length
            || answer.len() < self.config.min_learning_length
        {
            debug!("Interaction too short to learn from");
            return Ok(());
        }

        // Create a memory from the Q&A pair
        let content = format!("Q: {question}\nA: {answer}");
        let summary = Self::create_summary(question, answer);

        let memory = Memory::new(*user_id, content, summary, MemoryType::Context)
            .with_importance(self.config.default_importance);

        match self.memory_service.store(memory).await {
            Ok(mem) => {
                debug!(memory_id = %mem.id, "Learned from interaction");
            },
            Err(e) => {
                warn!(error = %e, "Failed to store interaction memory");
                // Don't fail the chat just because learning failed
            },
        }

        Ok(())
    }

    /// Create a summary for the Q&A interaction
    fn create_summary(question: &str, answer: &str) -> String {
        // Use first part of question + answer snippet as summary
        let max_question_len = 100;
        let max_answer_len = 50;

        let q_part = if question.len() <= max_question_len {
            question.to_string()
        } else {
            format!("{}...", &question[..max_question_len - 3])
        };

        let a_part = if answer.len() <= max_answer_len {
            answer.to_string()
        } else {
            format!("{}...", &answer[..max_answer_len - 3])
        };

        format!("Q: {q_part} → {a_part}")
    }

    /// Manually store a fact in memory
    pub async fn remember_fact(
        &self,
        user_id: &UserId,
        fact: &str,
        importance: f32,
    ) -> Result<Memory, ApplicationError> {
        self.memory_service
            .store_fact(*user_id, fact, importance)
            .await
    }

    /// Manually store a user preference
    pub async fn remember_preference(
        &self,
        user_id: &UserId,
        preference: &str,
        importance: f32,
    ) -> Result<Memory, ApplicationError> {
        self.memory_service
            .store_preference(*user_id, preference, importance)
            .await
    }

    /// Manually store a correction
    pub async fn remember_correction(
        &self,
        user_id: &UserId,
        correction: &str,
        importance: f32,
    ) -> Result<Memory, ApplicationError> {
        self.memory_service
            .store_correction(*user_id, correction, importance)
            .await
    }

    /// Get memory statistics for a user
    pub async fn memory_stats(
        &self,
        user_id: &UserId,
    ) -> Result<crate::ports::MemoryStats, ApplicationError> {
        self.memory_service.stats(user_id).await
    }

    /// Apply memory decay
    pub async fn apply_memory_decay(&self) -> Result<Vec<domain::MemoryId>, ApplicationError> {
        self.memory_service.apply_decay().await
    }

    /// Cleanup low-importance memories
    pub async fn cleanup_memories(&self) -> Result<usize, ApplicationError> {
        self.memory_service.cleanup_low_importance().await
    }

    /// Check if inference is healthy
    pub async fn is_healthy(&self) -> bool {
        self.inference.is_healthy().await
    }

    /// Get current model name
    pub fn current_model(&self) -> String {
        self.inference.current_model()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ports::{MockEmbeddingPort, MockMemoryStore, NoOpEncryption, SimilarMemory};
    use crate::services::MemoryServiceConfig;

    #[test]
    fn test_config_default() {
        let config = MemoryEnhancedChatConfig::default();
        assert!(config.enable_rag);
        assert!(config.enable_learning);
        assert!(config.system_prompt.is_none());
        assert_eq!(config.min_learning_length, 20);
        assert!((config.default_importance - 0.5).abs() < 0.001);
    }

    #[test]
    fn test_config_custom() {
        let config = MemoryEnhancedChatConfig {
            enable_rag: false,
            enable_learning: true,
            system_prompt: Some("Custom prompt".to_string()),
            min_learning_length: 50,
            default_importance: 0.7,
        };
        assert!(!config.enable_rag);
        assert!(config.enable_learning);
        assert_eq!(config.system_prompt.as_deref(), Some("Custom prompt"));
        assert_eq!(config.min_learning_length, 50);
    }

    #[test]
    fn test_config_clone() {
        let config = MemoryEnhancedChatConfig {
            enable_rag: true,
            enable_learning: false,
            system_prompt: Some("Test".to_string()),
            min_learning_length: 30,
            default_importance: 0.6,
        };
        let cloned = config.clone();
        assert_eq!(cloned.enable_rag, config.enable_rag);
        assert_eq!(cloned.enable_learning, config.enable_learning);
        assert_eq!(cloned.system_prompt, config.system_prompt);
        assert_eq!(cloned.min_learning_length, config.min_learning_length);
    }

    #[test]
    fn test_config_debug() {
        let config = MemoryEnhancedChatConfig::default();
        let debug = format!("{config:?}");
        assert!(debug.contains("MemoryEnhancedChatConfig"));
        assert!(debug.contains("enable_rag"));
        assert!(debug.contains("enable_learning"));
    }

    #[test]
    fn test_create_summary_short_question_and_answer() {
        let summary =
            MemoryEnhancedChat::<MockMemoryStore, MockEmbeddingPort, NoOpEncryption>::create_summary(
                "What is Rust?",
                "A systems programming language.",
            );
        assert!(summary.contains("What is Rust?"));
        assert!(summary.contains("A systems programming language."));
        assert!(summary.contains("→"));
    }

    #[test]
    fn test_create_summary_long_question() {
        let long_question = "a".repeat(150);
        let summary =
            MemoryEnhancedChat::<MockMemoryStore, MockEmbeddingPort, NoOpEncryption>::create_summary(
                &long_question,
                "Short answer.",
            );
        assert!(summary.contains("..."));
        // Question part should be truncated
        assert!(summary.len() < 200);
    }

    #[test]
    fn test_create_summary_long_answer() {
        let long_answer = "b".repeat(100);
        let summary =
            MemoryEnhancedChat::<MockMemoryStore, MockEmbeddingPort, NoOpEncryption>::create_summary(
                "Short question?",
                &long_answer,
            );
        assert!(summary.contains("..."));
        // Includes Q: prefix
        assert!(summary.starts_with("Q:"));
    }

    #[test]
    fn test_create_summary_both_long() {
        let long_question = "q".repeat(200);
        let long_answer = "a".repeat(200);
        let summary =
            MemoryEnhancedChat::<MockMemoryStore, MockEmbeddingPort, NoOpEncryption>::create_summary(
                &long_question,
                &long_answer,
            );
        // Both should be truncated
        assert!(summary.contains("..."));
        assert!(summary.starts_with("Q: "));
    }

    #[test]
    fn test_create_summary_exact_max_question_length() {
        let exact_question = "x".repeat(100);
        let summary =
            MemoryEnhancedChat::<MockMemoryStore, MockEmbeddingPort, NoOpEncryption>::create_summary(
                &exact_question,
                "answer",
            );
        // At exactly 100 chars, should not truncate question
        assert!(summary.contains(&"x".repeat(100)));
        assert!(!summary.contains("...→")); // No truncation marker before arrow
    }

    #[test]
    fn test_create_summary_exact_max_answer_length() {
        let exact_answer = "y".repeat(50);
        let summary =
            MemoryEnhancedChat::<MockMemoryStore, MockEmbeddingPort, NoOpEncryption>::create_summary(
                "question",
                &exact_answer,
            );
        // At exactly 50 chars, should not truncate answer
        assert!(summary.contains(&"y".repeat(50)));
    }

    // Helper to create a test memory service
    fn setup_memory_service() -> MemoryService<MockMemoryStore, MockEmbeddingPort, NoOpEncryption> {
        let store = Arc::new(MockMemoryStore::new());
        let embedding = Arc::new(MockEmbeddingPort::new());
        let encryption = Arc::new(NoOpEncryption);
        let config = MemoryServiceConfig {
            enable_encryption: false,
            ..Default::default()
        };
        MemoryService::new(store, embedding, encryption, config)
    }

    // Helper mock for InferencePort
    struct MockInference {
        model_name: String,
        healthy: bool,
    }

    impl MockInference {
        fn new() -> Self {
            Self {
                model_name: "test-model".to_string(),
                healthy: true,
            }
        }

        fn unhealthy() -> Self {
            Self {
                model_name: "test-model".to_string(),
                healthy: false,
            }
        }
    }

    #[async_trait::async_trait]
    impl InferencePort for MockInference {
        async fn generate(
            &self,
            _message: &str,
        ) -> Result<crate::ports::InferenceResult, ApplicationError> {
            Ok(crate::ports::InferenceResult {
                content: "Generated response".to_string(),
                model: self.model_name.clone(),
                tokens_used: Some(10),
                latency_ms: 50,
            })
        }

        async fn generate_with_context(
            &self,
            _conversation: &Conversation,
        ) -> Result<crate::ports::InferenceResult, ApplicationError> {
            Ok(crate::ports::InferenceResult {
                content: "Context response".to_string(),
                model: self.model_name.clone(),
                tokens_used: Some(15),
                latency_ms: 60,
            })
        }

        async fn generate_with_system(
            &self,
            _system_prompt: &str,
            _message: &str,
        ) -> Result<crate::ports::InferenceResult, ApplicationError> {
            Ok(crate::ports::InferenceResult {
                content: "System response".to_string(),
                model: self.model_name.clone(),
                tokens_used: Some(20),
                latency_ms: 70,
            })
        }

        async fn generate_stream(
            &self,
            _message: &str,
        ) -> Result<crate::ports::InferenceStream, ApplicationError> {
            Err(ApplicationError::Internal(
                "streaming not implemented in mock".to_string(),
            ))
        }

        async fn generate_stream_with_system(
            &self,
            _system_prompt: &str,
            _message: &str,
        ) -> Result<crate::ports::InferenceStream, ApplicationError> {
            Err(ApplicationError::Internal(
                "streaming not implemented in mock".to_string(),
            ))
        }

        async fn is_healthy(&self) -> bool {
            self.healthy
        }

        fn current_model(&self) -> String {
            self.model_name.clone()
        }

        async fn list_available_models(&self) -> Result<Vec<String>, ApplicationError> {
            Ok(vec![self.model_name.clone()])
        }

        async fn switch_model(&self, _model_name: &str) -> Result<(), ApplicationError> {
            Ok(())
        }
    }

    #[test]
    fn test_memory_enhanced_chat_new() {
        let inference = Arc::new(MockInference::new());
        let memory_service = setup_memory_service();
        let config = MemoryEnhancedChatConfig::default();

        let chat = MemoryEnhancedChat::new(inference, memory_service, config);
        let debug = format!("{chat:?}");
        assert!(debug.contains("MemoryEnhancedChat"));
    }

    #[test]
    fn test_memory_enhanced_chat_clone() {
        let inference = Arc::new(MockInference::new());
        let memory_service = setup_memory_service();
        let config = MemoryEnhancedChatConfig::default();

        let chat = MemoryEnhancedChat::new(inference, memory_service, config);
        let cloned = chat.clone();

        // Both should have same config
        assert_eq!(cloned.config.enable_rag, chat.config.enable_rag);
        assert_eq!(cloned.config.enable_learning, chat.config.enable_learning);
    }

    #[test]
    fn test_memory_enhanced_chat_debug() {
        let inference = Arc::new(MockInference::new());
        let memory_service = setup_memory_service();
        let config = MemoryEnhancedChatConfig {
            enable_rag: false,
            enable_learning: true,
            system_prompt: Some("Debug test".to_string()),
            min_learning_length: 25,
            default_importance: 0.6,
        };

        let chat = MemoryEnhancedChat::new(inference, memory_service, config);
        let debug = format!("{chat:?}");

        assert!(debug.contains("MemoryEnhancedChat"));
        assert!(debug.contains("config"));
    }

    #[test]
    fn test_current_model() {
        let inference = Arc::new(MockInference::new());
        let memory_service = setup_memory_service();
        let config = MemoryEnhancedChatConfig::default();

        let chat = MemoryEnhancedChat::new(inference, memory_service, config);
        assert_eq!(chat.current_model(), "test-model");
    }

    #[tokio::test]
    async fn test_is_healthy_true() {
        let inference = Arc::new(MockInference::new());
        let memory_service = setup_memory_service();
        let config = MemoryEnhancedChatConfig::default();

        let chat = MemoryEnhancedChat::new(inference, memory_service, config);
        assert!(chat.is_healthy().await);
    }

    #[tokio::test]
    async fn test_is_healthy_false() {
        let inference = Arc::new(MockInference::unhealthy());
        let memory_service = setup_memory_service();
        let config = MemoryEnhancedChatConfig::default();

        let chat = MemoryEnhancedChat::new(inference, memory_service, config);
        assert!(!chat.is_healthy().await);
    }

    #[test]
    fn test_build_system_prompt_without_memories() {
        let inference = Arc::new(MockInference::new());
        let memory_service = setup_memory_service();
        let config = MemoryEnhancedChatConfig {
            system_prompt: Some("You are helpful.".to_string()),
            ..Default::default()
        };

        let chat = MemoryEnhancedChat::new(inference, memory_service, config);
        let prompt = chat.build_system_prompt(&[]);

        assert_eq!(prompt, "You are helpful.");
    }

    #[test]
    fn test_build_system_prompt_default_without_memories() {
        let inference = Arc::new(MockInference::new());
        let memory_service = setup_memory_service();
        let config = MemoryEnhancedChatConfig::default();

        let chat = MemoryEnhancedChat::new(inference, memory_service, config);
        let prompt = chat.build_system_prompt(&[]);

        assert_eq!(prompt, "You are a helpful AI assistant.");
    }

    #[test]
    fn test_build_system_prompt_with_memories() {
        let inference = Arc::new(MockInference::new());
        let memory_service = setup_memory_service();
        let config = MemoryEnhancedChatConfig {
            system_prompt: Some("Be helpful.".to_string()),
            ..Default::default()
        };

        let chat = MemoryEnhancedChat::new(inference, memory_service, config);

        let user_id = UserId::new();
        let memory = Memory::new(
            user_id,
            "User prefers concise answers".to_string(),
            "User prefers concise answers".to_string(),
            MemoryType::Preference,
        )
        .with_importance(0.8);

        let similar = vec![SimilarMemory::new(memory, 0.95)];
        let prompt = chat.build_system_prompt(&similar);

        assert!(prompt.contains("Be helpful."));
        assert!(prompt.contains("concise answers"));
    }

    #[test]
    fn test_build_system_prompt_default_with_memories() {
        let inference = Arc::new(MockInference::new());
        let memory_service = setup_memory_service();
        let config = MemoryEnhancedChatConfig::default();

        let chat = MemoryEnhancedChat::new(inference, memory_service, config);

        let user_id = UserId::new();
        let memory = Memory::new(
            user_id,
            "Important fact".to_string(),
            "Important fact".to_string(),
            MemoryType::Fact,
        );

        let similar = vec![SimilarMemory::new(memory, 0.9)];
        let prompt = chat.build_system_prompt(&similar);

        assert!(prompt.contains("You are a helpful AI assistant."));
        assert!(prompt.contains("Important fact"));
    }

    #[test]
    fn test_config_all_disabled() {
        let config = MemoryEnhancedChatConfig {
            enable_rag: false,
            enable_learning: false,
            system_prompt: None,
            min_learning_length: 0,
            default_importance: 0.0,
        };
        assert!(!config.enable_rag);
        assert!(!config.enable_learning);
        assert!(config.system_prompt.is_none());
    }

    #[test]
    fn test_config_high_importance() {
        let config = MemoryEnhancedChatConfig {
            default_importance: 1.0,
            ..Default::default()
        };
        assert!((config.default_importance - 1.0).abs() < 0.001);
    }

    #[test]
    fn test_config_low_learning_threshold() {
        let config = MemoryEnhancedChatConfig {
            min_learning_length: 1,
            ..Default::default()
        };
        assert_eq!(config.min_learning_length, 1);
    }
}
