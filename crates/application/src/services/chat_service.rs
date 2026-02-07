//! Chat service - Conversation handling with optional persistence
//!
//! This service provides both stateless single-message chat and stateful
//! conversation handling with automatic message truncation.

use std::{fmt, sync::Arc, time::Instant};

use domain::{ChatMessage, Conversation, ConversationId, MessageMetadata, MessageRole};
use tracing::{debug, info, instrument};

use crate::{
    error::ApplicationError,
    ports::{ConversationStore, InferencePort, InferenceStream},
};

/// Maximum number of messages to retain in a conversation (FIFO truncation).
/// System prompt is always preserved.
pub const MAX_CONVERSATION_MESSAGES: usize = 50;

/// Service for handling chat conversations
///
/// Supports both stateless single-message chat and stateful conversation handling
/// with automatic persistence and FIFO message truncation.
pub struct ChatService {
    inference: Arc<dyn InferencePort>,
    conversation_store: Option<Arc<dyn ConversationStore>>,
    system_prompt: Option<String>,
}

impl fmt::Debug for ChatService {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ChatService")
            .field("system_prompt", &self.system_prompt)
            .field("has_conversation_store", &self.conversation_store.is_some())
            .finish_non_exhaustive()
    }
}

impl ChatService {
    /// Create a new chat service (stateless mode)
    pub fn new(inference: Arc<dyn InferencePort>) -> Self {
        Self {
            inference,
            conversation_store: None,
            system_prompt: None,
        }
    }

    /// Create a chat service with conversation persistence
    pub fn with_conversation_store(
        inference: Arc<dyn InferencePort>,
        store: Arc<dyn ConversationStore>,
    ) -> Self {
        Self {
            inference,
            conversation_store: Some(store),
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
            conversation_store: None,
            system_prompt: Some(prompt.into()),
        }
    }

    /// Create a fully configured chat service with conversation store and system prompt
    pub fn with_all(
        inference: Arc<dyn InferencePort>,
        store: Arc<dyn ConversationStore>,
        system_prompt: impl Into<String>,
    ) -> Self {
        Self {
            inference,
            conversation_store: Some(store),
            system_prompt: Some(system_prompt.into()),
        }
    }

    /// Set the conversation store for an existing service
    pub fn set_conversation_store(&mut self, store: Arc<dyn ConversationStore>) {
        self.conversation_store = Some(store);
    }

    /// Set the system prompt for an existing service
    pub fn set_system_prompt(&mut self, prompt: impl Into<String>) {
        self.system_prompt = Some(prompt.into());
    }

    /// Handle a single chat message (stateless)
    #[instrument(skip(self, message), fields(message_len = message.len()))]
    pub async fn chat(&self, message: &str) -> Result<ChatMessage, ApplicationError> {
        let start = Instant::now();

        let result = match &self.system_prompt {
            Some(system) => self.inference.generate_with_system(system, message).await?,
            None => self.inference.generate(message).await?,
        };

        #[allow(clippy::cast_possible_truncation)]
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

        #[allow(clippy::cast_possible_truncation)]
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

    /// Handle a chat message with optional conversation context.
    ///
    /// If `conversation_id` is provided and the conversation exists, continues it.
    /// If `conversation_id` is provided but doesn't exist, creates a new conversation with that ID.
    /// If `conversation_id` is `None`, generates a new UUID and creates a new conversation.
    ///
    /// Messages are automatically truncated using FIFO when exceeding [`MAX_CONVERSATION_MESSAGES`].
    /// The system prompt (if any) is always preserved during truncation.
    ///
    /// Returns a tuple of (response message, conversation_id).
    #[instrument(skip(self, message, conversation_id), fields(message_len = message.len(), conv_id = ?conversation_id))]
    pub async fn chat_with_context(
        &self,
        message: &str,
        conversation_id: Option<&str>,
    ) -> Result<(ChatMessage, ConversationId), ApplicationError> {
        let store = self.conversation_store.as_ref().ok_or_else(|| {
            ApplicationError::Configuration(
                "Conversation store not configured for contextual chat".to_string(),
            )
        })?;

        // Resolve or create conversation
        let (mut conversation, is_new) = if let Some(id_str) = conversation_id {
            let conv_id = ConversationId::parse(id_str).map_err(|e| {
                ApplicationError::InvalidOperation(format!("Invalid conversation ID: {e}"))
            })?;
            store.get(&conv_id).await?.map_or_else(
                || {
                    // Create new conversation with the provided ID
                    let mut conv = self
                        .system_prompt
                        .as_ref()
                        .map_or_else(Conversation::new, Conversation::with_system_prompt);
                    // Override the auto-generated ID with the provided one
                    conv.id = conv_id;
                    (conv, true)
                },
                |conv| (conv, false),
            )
        } else {
            // Create new conversation with auto-generated ID
            let conv = self
                .system_prompt
                .as_ref()
                .map_or_else(Conversation::new, Conversation::with_system_prompt);
            (conv, true)
        };

        let conv_id = conversation.id;

        // Add user message
        conversation.add_user_message(message);

        // Apply FIFO truncation if needed (preserve system prompt)
        Self::truncate_conversation(&mut conversation);

        // Generate response
        let start = Instant::now();
        let result = self.inference.generate_with_context(&conversation).await?;

        #[allow(clippy::cast_possible_truncation)]
        let latency = start.elapsed().as_millis() as u64;

        debug!(
            model = %result.model,
            tokens = ?result.tokens_used,
            latency_ms = latency,
            conv_id = %conv_id,
            is_new = is_new,
            msg_count = conversation.message_count(),
            "Contextual chat response generated"
        );

        // Create response message
        let response = ChatMessage::assistant(&result.content).with_metadata(MessageMetadata {
            model: Some(result.model),
            tokens: result.tokens_used,
            latency_ms: Some(latency),
        });

        // Add assistant response to conversation
        conversation.add_message(response.clone());

        // Persist conversation
        if is_new {
            store.save(&conversation).await?;
            info!(conv_id = %conv_id, "New conversation created");
        } else {
            store.update(&conversation).await?;
        }

        Ok((response, conv_id))
    }

    /// Apply FIFO truncation to a conversation.
    ///
    /// Removes the oldest messages (excluding system role messages) when the
    /// conversation exceeds [`MAX_CONVERSATION_MESSAGES`].
    fn truncate_conversation(conversation: &mut Conversation) {
        let total = conversation.messages.len();
        if total <= MAX_CONVERSATION_MESSAGES {
            return;
        }

        let to_remove = total - MAX_CONVERSATION_MESSAGES;

        // Separate system messages from others (system messages are preserved)
        let (system_msgs, mut other_msgs): (Vec<_>, Vec<_>) = conversation
            .messages
            .drain(..)
            .partition(|m| m.role == MessageRole::System);

        // Remove oldest non-system messages
        let removed = other_msgs.drain(..to_remove.min(other_msgs.len())).count();

        // Reconstruct: system messages first, then remaining messages
        conversation.messages = system_msgs;
        conversation.messages.extend(other_msgs);

        debug!(
            conv_id = %conversation.id,
            removed = removed,
            remaining = conversation.messages.len(),
            "Conversation truncated (FIFO)"
        );
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

    // Mock ConversationStore for testing chat_with_context
    mock! {
        pub ConvStore {}

        #[async_trait::async_trait]
        impl ConversationStore for ConvStore {
            async fn save(&self, conversation: &Conversation) -> Result<(), ApplicationError>;
            async fn get(&self, id: &ConversationId) -> Result<Option<Conversation>, ApplicationError>;
            async fn update(&self, conversation: &Conversation) -> Result<(), ApplicationError>;
            async fn delete(&self, id: &ConversationId) -> Result<(), ApplicationError>;
            async fn add_message(&self, conversation_id: &ConversationId, message: &ChatMessage) -> Result<(), ApplicationError>;
            async fn list_recent(&self, limit: usize) -> Result<Vec<Conversation>, ApplicationError>;
            async fn search(&self, query: &str, limit: usize) -> Result<Vec<Conversation>, ApplicationError>;
            async fn cleanup_older_than(&self, cutoff: chrono::DateTime<chrono::Utc>) -> Result<usize, ApplicationError>;
        }
    }

    #[tokio::test]
    async fn chat_with_context_creates_new_conversation() {
        let mut mock_inference = MockInferenceEngine::new();
        mock_inference
            .expect_generate_with_context()
            .returning(|_| Ok(mock_inference_result("Hello!")));

        let mut mock_store = MockConvStore::new();
        mock_store.expect_save().returning(|_| Ok(()));

        let service =
            ChatService::with_conversation_store(Arc::new(mock_inference), Arc::new(mock_store));

        let (response, conv_id) = service.chat_with_context("Hi", None).await.unwrap();

        assert_eq!(response.content, "Hello!");
        assert!(!conv_id.to_string().is_empty());
    }

    #[tokio::test]
    async fn chat_with_context_continues_existing_conversation() {
        let existing_conv = Conversation::new();
        let existing_id = existing_conv.id;
        let id_str = existing_id.to_string();
        let cloned_conv = existing_conv.clone();

        let mut mock_inference = MockInferenceEngine::new();
        mock_inference
            .expect_generate_with_context()
            .returning(|_| Ok(mock_inference_result("Continued!")));

        let mut mock_store = MockConvStore::new();
        mock_store
            .expect_get()
            .returning(move |_| Ok(Some(cloned_conv.clone())));
        mock_store.expect_update().returning(|_| Ok(()));

        let service =
            ChatService::with_conversation_store(Arc::new(mock_inference), Arc::new(mock_store));

        let (response, returned_id) = service
            .chat_with_context("Continue", Some(&id_str))
            .await
            .unwrap();

        assert_eq!(response.content, "Continued!");
        assert_eq!(returned_id.to_string(), id_str);
    }

    #[tokio::test]
    async fn chat_with_context_creates_conversation_with_provided_id_if_not_found() {
        let new_id = ConversationId::new();
        let id_str = new_id.to_string();

        let mut mock_inference = MockInferenceEngine::new();
        mock_inference
            .expect_generate_with_context()
            .returning(|_| Ok(mock_inference_result("New with ID!")));

        let mut mock_store = MockConvStore::new();
        mock_store.expect_get().returning(|_| Ok(None));
        mock_store.expect_save().returning(|_| Ok(()));

        let service =
            ChatService::with_conversation_store(Arc::new(mock_inference), Arc::new(mock_store));

        let (response, returned_id) = service
            .chat_with_context("Hello", Some(&id_str))
            .await
            .unwrap();

        assert_eq!(response.content, "New with ID!");
        assert_eq!(returned_id.to_string(), id_str);
    }

    #[tokio::test]
    async fn chat_with_context_fails_without_store() {
        let mock_inference = MockInferenceEngine::new();
        let service = ChatService::new(Arc::new(mock_inference));

        let result = service.chat_with_context("Hi", None).await;

        assert!(matches!(result, Err(ApplicationError::Configuration(_))));
    }

    #[tokio::test]
    async fn chat_with_context_fails_with_invalid_id() {
        let mock_inference = MockInferenceEngine::new();
        let mock_store = MockConvStore::new();

        let service =
            ChatService::with_conversation_store(Arc::new(mock_inference), Arc::new(mock_store));

        let result = service.chat_with_context("Hi", Some("invalid-uuid")).await;

        assert!(matches!(result, Err(ApplicationError::InvalidOperation(_))));
    }

    #[test]
    fn truncate_conversation_does_nothing_under_limit() {
        let mut conv = Conversation::new();
        for i in 0..MAX_CONVERSATION_MESSAGES {
            conv.add_user_message(format!("Message {i}"));
        }
        let original_count = conv.message_count();

        ChatService::truncate_conversation(&mut conv);

        assert_eq!(conv.message_count(), original_count);
    }

    #[test]
    fn truncate_conversation_removes_oldest_messages() {
        let mut conv = Conversation::new();
        // Add more than the limit
        for i in 0..(MAX_CONVERSATION_MESSAGES + 10) {
            conv.add_user_message(format!("Message {i}"));
        }

        ChatService::truncate_conversation(&mut conv);

        assert_eq!(conv.message_count(), MAX_CONVERSATION_MESSAGES);
        // Oldest messages should be removed, so first remaining is "Message 10"
        assert_eq!(conv.messages[0].content, "Message 10");
    }

    #[test]
    fn truncate_conversation_preserves_system_messages() {
        let mut conv = Conversation::new();
        // Add a system message first (this is how LLM APIs typically include system prompts)
        conv.add_message(ChatMessage::system("I am a helpful assistant"));
        // Add more user/assistant messages than the limit
        // 1 system + 55 user = 56 total, should truncate to 50 total
        for i in 0..55 {
            conv.add_user_message(format!("User message {i}"));
        }
        assert_eq!(conv.message_count(), 56);

        ChatService::truncate_conversation(&mut conv);

        // System message should still be first (preserved during truncation)
        assert_eq!(conv.messages[0].role, MessageRole::System);
        assert_eq!(conv.messages[0].content, "I am a helpful assistant");
        // Total should be MAX_CONVERSATION_MESSAGES (50)
        // 1 system + 49 user messages = 50 total
        assert_eq!(conv.message_count(), MAX_CONVERSATION_MESSAGES);
        // The oldest user messages should be removed, so first user message is "User message 6"
        // (removed messages 0-5, kept 6-54 = 49 messages)
        assert_eq!(conv.messages[1].content, "User message 6");
    }

    #[test]
    fn chat_service_with_conversation_store() {
        let mock_inference = MockInferenceEngine::new();
        let mock_store = MockConvStore::new();

        let service =
            ChatService::with_conversation_store(Arc::new(mock_inference), Arc::new(mock_store));

        let debug = format!("{service:?}");
        assert!(debug.contains("has_conversation_store"));
        assert!(debug.contains("true"));
    }

    #[test]
    fn chat_service_with_all() {
        let mock_inference = MockInferenceEngine::new();
        let mock_store = MockConvStore::new();

        let service = ChatService::with_all(
            Arc::new(mock_inference),
            Arc::new(mock_store),
            "System prompt",
        );

        let debug = format!("{service:?}");
        assert!(debug.contains("has_conversation_store"));
        assert!(debug.contains("system_prompt"));
    }

    #[test]
    fn set_conversation_store_updates_service() {
        let mock_inference = MockInferenceEngine::new();
        let mut service = ChatService::new(Arc::new(mock_inference));

        let debug_before = format!("{service:?}");
        assert!(debug_before.contains("false")); // has_conversation_store: false

        let mock_store = MockConvStore::new();
        service.set_conversation_store(Arc::new(mock_store));

        let debug_after = format!("{service:?}");
        assert!(debug_after.contains("true")); // has_conversation_store: true
    }

    #[test]
    fn set_system_prompt_updates_service() {
        let mock_inference = MockInferenceEngine::new();
        let mut service = ChatService::new(Arc::new(mock_inference));

        service.set_system_prompt("New prompt");

        let debug = format!("{service:?}");
        assert!(debug.contains("system_prompt"));
    }
}
