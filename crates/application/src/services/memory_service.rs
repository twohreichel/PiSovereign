//! Memory service for AI knowledge storage and retrieval
//!
//! Orchestrates:
//! - Memory storage with encryption
//! - Embedding generation for semantic search
//! - RAG-based context retrieval
//! - Memory importance scoring and decay

use std::sync::Arc;

use domain::{Memory, MemoryId, MemoryQuery, MemoryType, UserId};
use tracing::{debug, info, instrument, warn};

use crate::{
    error::ApplicationError,
    ports::{EmbeddingPort, EncryptionPort, MemoryStats, MemoryStore, SimilarMemory},
};

/// Configuration for memory service
#[derive(Debug, Clone)]
pub struct MemoryServiceConfig {
    /// Number of similar memories to retrieve for RAG
    pub rag_limit: usize,
    /// Minimum similarity threshold for RAG retrieval
    pub rag_threshold: f32,
    /// Similarity threshold for memory deduplication
    pub merge_threshold: f32,
    /// Minimum importance to keep memories (below this, decay removes them)
    pub min_importance: f32,
    /// Decay factor applied to importance over time
    pub decay_factor: f32,
    /// Whether to enable content encryption
    pub enable_encryption: bool,
}

impl Default for MemoryServiceConfig {
    fn default() -> Self {
        Self {
            rag_limit: 5,
            rag_threshold: 0.5,
            merge_threshold: 0.85,
            min_importance: 0.1,
            decay_factor: 0.95,
            enable_encryption: true,
        }
    }
}

/// Memory service for storing and retrieving AI knowledge
///
/// # Examples
///
/// ```ignore
/// let service = MemoryService::new(
///     store,
///     embedding_port,
///     encryption_port,
///     MemoryServiceConfig::default(),
/// );
///
/// // Store a new memory
/// let memory = service.store_fact(user_id, "Paris is the capital of France", 0.8).await?;
///
/// // Retrieve relevant context for RAG
/// let context = service.retrieve_context(&user_id, "What is the capital of France?").await?;
/// ```
pub struct MemoryService<S, E, C>
where
    S: MemoryStore,
    E: EmbeddingPort,
    C: EncryptionPort,
{
    store: Arc<S>,
    embedding: Arc<E>,
    encryption: Arc<C>,
    config: MemoryServiceConfig,
}

impl<S, E, C> Clone for MemoryService<S, E, C>
where
    S: MemoryStore,
    E: EmbeddingPort,
    C: EncryptionPort,
{
    fn clone(&self) -> Self {
        Self {
            store: Arc::clone(&self.store),
            embedding: Arc::clone(&self.embedding),
            encryption: Arc::clone(&self.encryption),
            config: self.config.clone(),
        }
    }
}

impl<S, E, C> std::fmt::Debug for MemoryService<S, E, C>
where
    S: MemoryStore,
    E: EmbeddingPort,
    C: EncryptionPort,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MemoryService")
            .field("config", &self.config)
            .finish_non_exhaustive()
    }
}

impl<S, E, C> MemoryService<S, E, C>
where
    S: MemoryStore,
    E: EmbeddingPort,
    C: EncryptionPort,
{
    /// Create a new memory service
    #[must_use]
    pub const fn new(
        store: Arc<S>,
        embedding: Arc<E>,
        encryption: Arc<C>,
        config: MemoryServiceConfig,
    ) -> Self {
        Self {
            store,
            embedding,
            encryption,
            config,
        }
    }

    /// Store a new memory with automatic embedding and optional encryption
    ///
    /// # Arguments
    ///
    /// * `memory` - The memory to store
    ///
    /// # Returns
    ///
    /// The stored memory with its ID
    #[instrument(skip(self, memory), fields(memory_id, user_id = %memory.user_id))]
    pub async fn store(&self, memory: Memory) -> Result<Memory, ApplicationError> {
        let mut memory = memory;

        // Generate embedding for semantic search
        let embedding = self.embedding.embed(&memory.content).await?;
        memory.embedding = Some(embedding);

        // Encrypt content if enabled
        if self.config.enable_encryption && self.encryption.is_enabled() {
            let encrypted = self.encryption.encrypt_string(&memory.content).await?;
            memory.content = encrypted;

            let encrypted_summary = self.encryption.encrypt_string(&memory.summary).await?;
            memory.summary = encrypted_summary;
        }

        // Check for similar memories to potentially merge
        if let Some(ref emb) = memory.embedding {
            let similar = self
                .store
                .search_similar(&memory.user_id, emb, 1, self.config.merge_threshold)
                .await?;

            if let Some(existing) = similar.first() {
                if existing.similarity >= self.config.merge_threshold {
                    info!(
                        existing_id = %existing.memory.id,
                        similarity = existing.similarity,
                        "Found similar memory, merging"
                    );
                    return self.merge_memories(&existing.memory, &memory).await;
                }
            }
        }

        // Save the new memory
        self.store.save(&memory).await?;
        debug!(memory_id = %memory.id, "Stored new memory");

        Ok(memory)
    }

    /// Retrieve relevant context for RAG
    ///
    /// Returns the most relevant memories for the given query.
    ///
    /// # Arguments
    ///
    /// * `user_id` - User to retrieve memories for
    /// * `query` - The query to find relevant context for
    ///
    /// # Returns
    ///
    /// List of similar memories with decrypted content
    #[instrument(skip(self, query), fields(user_id = %user_id))]
    pub async fn retrieve_context(
        &self,
        user_id: &UserId,
        query: &str,
    ) -> Result<Vec<SimilarMemory>, ApplicationError> {
        // Generate embedding for the query
        let query_embedding = self.embedding.embed(query).await?;

        // Search for similar memories
        let similar = self
            .store
            .search_similar(
                user_id,
                &query_embedding,
                self.config.rag_limit,
                self.config.rag_threshold,
            )
            .await?;

        // Decrypt content and record access
        let mut results = Vec::with_capacity(similar.len());
        for mut sim in similar {
            // Record that this memory was accessed (for decay calculation)
            if let Err(e) = self.store.record_access(&sim.memory.id).await {
                warn!(memory_id = %sim.memory.id, error = %e, "Failed to record access");
            }

            // Decrypt content if encryption is enabled
            if self.config.enable_encryption && self.encryption.is_enabled() {
                if let Ok(decrypted) = self.encryption.decrypt_string(&sim.memory.content).await {
                    sim.memory.content = decrypted;
                }
                if let Ok(decrypted) = self.encryption.decrypt_string(&sim.memory.summary).await {
                    sim.memory.summary = decrypted;
                }
            }

            results.push(sim);
        }

        debug!(count = results.len(), "Retrieved context for RAG");

        Ok(results)
    }

    /// Store a fact memory
    #[instrument(skip(self, content))]
    pub async fn store_fact(
        &self,
        user_id: UserId,
        content: &str,
        importance: f32,
    ) -> Result<Memory, ApplicationError> {
        let memory = Memory::new(
            user_id,
            content.to_string(),
            summarize(content),
            MemoryType::Fact,
        )
        .with_importance(importance);
        self.store(memory).await
    }

    /// Store a user preference
    #[instrument(skip(self, content))]
    pub async fn store_preference(
        &self,
        user_id: UserId,
        content: &str,
        importance: f32,
    ) -> Result<Memory, ApplicationError> {
        let memory = Memory::new(
            user_id,
            content.to_string(),
            summarize(content),
            MemoryType::Preference,
        )
        .with_importance(importance);
        self.store(memory).await
    }

    /// Store a correction/feedback
    #[instrument(skip(self, content))]
    pub async fn store_correction(
        &self,
        user_id: UserId,
        content: &str,
        importance: f32,
    ) -> Result<Memory, ApplicationError> {
        let memory = Memory::new(
            user_id,
            content.to_string(),
            summarize(content),
            MemoryType::Correction,
        )
        .with_importance(importance);
        self.store(memory).await
    }

    /// Store a tool result
    #[instrument(skip(self, content))]
    pub async fn store_tool_result(
        &self,
        user_id: UserId,
        content: &str,
        importance: f32,
    ) -> Result<Memory, ApplicationError> {
        let memory = Memory::new(
            user_id,
            content.to_string(),
            summarize(content),
            MemoryType::ToolResult,
        )
        .with_importance(importance);
        self.store(memory).await
    }

    /// Store conversation context
    #[instrument(skip(self, content))]
    pub async fn store_context(
        &self,
        user_id: UserId,
        conversation_id: domain::value_objects::ConversationId,
        content: &str,
        importance: f32,
    ) -> Result<Memory, ApplicationError> {
        let memory = Memory::new(
            user_id,
            content.to_string(),
            summarize(content),
            MemoryType::Context,
        )
        .with_importance(importance)
        .with_conversation(conversation_id);
        self.store(memory).await
    }

    /// Get a specific memory by ID
    #[instrument(skip(self))]
    pub async fn get(&self, id: &MemoryId) -> Result<Option<Memory>, ApplicationError> {
        let memory = self.store.get(id).await?;

        // Decrypt if needed
        if let Some(mut mem) = memory {
            if self.config.enable_encryption && self.encryption.is_enabled() {
                if let Ok(decrypted) = self.encryption.decrypt_string(&mem.content).await {
                    mem.content = decrypted;
                }
                if let Ok(decrypted) = self.encryption.decrypt_string(&mem.summary).await {
                    mem.summary = decrypted;
                }
            }
            Ok(Some(mem))
        } else {
            Ok(None)
        }
    }

    /// Delete a memory
    #[instrument(skip(self))]
    pub async fn delete(&self, id: &MemoryId) -> Result<(), ApplicationError> {
        self.store.delete(id).await
    }

    /// List memories with optional filtering
    #[instrument(skip(self))]
    pub async fn list(&self, query: MemoryQuery) -> Result<Vec<Memory>, ApplicationError> {
        let memories = self.store.list(&query).await?;

        // Decrypt all memories if needed
        if self.config.enable_encryption && self.encryption.is_enabled() {
            let mut decrypted = Vec::with_capacity(memories.len());
            for mut mem in memories {
                if let Ok(d) = self.encryption.decrypt_string(&mem.content).await {
                    mem.content = d;
                }
                if let Ok(d) = self.encryption.decrypt_string(&mem.summary).await {
                    mem.summary = d;
                }
                decrypted.push(mem);
            }
            Ok(decrypted)
        } else {
            Ok(memories)
        }
    }

    /// Apply decay to all memories
    ///
    /// Reduces importance over time based on access patterns.
    /// Returns IDs of memories that fell below threshold.
    #[instrument(skip(self))]
    pub async fn apply_decay(&self) -> Result<Vec<MemoryId>, ApplicationError> {
        let affected = self.store.apply_decay(self.config.decay_factor).await?;
        debug!(count = affected.len(), "Applied decay to memories");
        Ok(affected)
    }

    /// Cleanup memories with very low importance
    #[instrument(skip(self))]
    pub async fn cleanup_low_importance(&self) -> Result<usize, ApplicationError> {
        let deleted = self
            .store
            .cleanup_below_threshold(self.config.min_importance)
            .await?;
        info!(deleted, "Cleaned up low importance memories");
        Ok(deleted)
    }

    /// Get memory statistics for a user
    pub async fn stats(&self, user_id: &UserId) -> Result<MemoryStats, ApplicationError> {
        self.store.stats(user_id).await
    }

    /// Format context for injection into prompts
    ///
    /// Takes retrieved memories and formats them for use in AI prompts.
    #[must_use]
    pub fn format_context_for_prompt(memories: &[SimilarMemory]) -> String {
        if memories.is_empty() {
            return String::new();
        }

        let mut context = String::from("Relevant context from memory:\n");
        for (i, sim) in memories.iter().enumerate() {
            context.push_str(&format!(
                "{}. [{}] (relevance: {:.0}%): {}\n",
                i + 1,
                sim.memory.memory_type,
                sim.similarity * 100.0,
                sim.memory.summary
            ));
        }
        context
    }

    /// Merge two similar memories
    async fn merge_memories(
        &self,
        existing: &Memory,
        new: &Memory,
    ) -> Result<Memory, ApplicationError> {
        // Update the existing memory with combined information
        let mut merged = existing.clone();

        // Decrypt existing content if needed
        if self.config.enable_encryption && self.encryption.is_enabled() {
            if let Ok(decrypted) = self.encryption.decrypt_string(&merged.content).await {
                merged.content = decrypted;
            }
            if let Ok(decrypted) = self.encryption.decrypt_string(&merged.summary).await {
                merged.summary = decrypted;
            }
        }

        // Combine content (existing + new context)
        merged.content = format!("{}\n\nAdditional context: {}", merged.content, new.content);

        // Keep the higher importance
        if new.importance > merged.importance {
            merged.importance = new.importance;
        }

        // Merge tags
        for tag in &new.tags {
            if !merged.tags.contains(tag) {
                merged.tags.push(tag.clone());
            }
        }

        // Update embedding with new combined content
        let embedding = self.embedding.embed(&merged.content).await?;
        merged.embedding = Some(embedding);

        // Re-encrypt if needed
        if self.config.enable_encryption && self.encryption.is_enabled() {
            let encrypted = self.encryption.encrypt_string(&merged.content).await?;
            merged.content = encrypted;

            let encrypted_summary = self.encryption.encrypt_string(&merged.summary).await?;
            merged.summary = encrypted_summary;
        }

        self.store.update(&merged).await?;
        debug!(memory_id = %merged.id, "Merged memories");

        Ok(merged)
    }
}

/// Create a simple summary from content
fn summarize(content: &str) -> String {
    const MAX_SUMMARY_LEN: usize = 200;

    if content.len() <= MAX_SUMMARY_LEN {
        content.to_string()
    } else {
        format!("{}...", &content[..MAX_SUMMARY_LEN - 3])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ports::{MockEmbeddingPort, MockMemoryStore, NoOpEncryption};

    fn setup_service() -> MemoryService<MockMemoryStore, MockEmbeddingPort, NoOpEncryption> {
        let store = Arc::new(MockMemoryStore::new());
        let embedding = Arc::new(MockEmbeddingPort::new());
        let encryption = Arc::new(NoOpEncryption);
        let config = MemoryServiceConfig {
            enable_encryption: false,
            ..Default::default()
        };
        MemoryService::new(store, embedding, encryption, config)
    }

    #[test]
    fn test_config_default() {
        let config = MemoryServiceConfig::default();
        assert_eq!(config.rag_limit, 5);
        assert!((config.rag_threshold - 0.5).abs() < 0.001);
        assert!((config.merge_threshold - 0.85).abs() < 0.001);
        assert!((config.min_importance - 0.1).abs() < 0.001);
        assert!((config.decay_factor - 0.95).abs() < 0.001);
        assert!(config.enable_encryption);
    }

    #[test]
    fn test_summarize_short() {
        let content = "Short content";
        assert_eq!(summarize(content), content);
    }

    #[test]
    fn test_summarize_long() {
        let content = "a".repeat(300);
        let summary = summarize(&content);
        assert!(summary.len() <= 200);
        assert!(summary.ends_with("..."));
    }

    #[test]
    fn test_format_context_empty() {
        let context = MemoryService::<MockMemoryStore, MockEmbeddingPort, NoOpEncryption>::format_context_for_prompt(&[]);
        assert!(context.is_empty());
    }

    #[test]
    fn test_format_context_with_memories() {
        let memory = Memory::new(
            UserId::new(),
            "Paris is the capital of France".to_string(),
            "Paris is the capital of France".to_string(),
            MemoryType::Fact,
        )
        .with_importance(0.8);
        let similar = vec![SimilarMemory {
            memory,
            similarity: 0.95,
        }];

        let context = MemoryService::<MockMemoryStore, MockEmbeddingPort, NoOpEncryption>::format_context_for_prompt(&similar);
        assert!(context.contains("Paris"));
        assert!(context.contains("95%"));
        assert!(context.contains("Fact"));
    }

    #[test]
    fn test_service_debug() {
        let service = setup_service();
        let debug = format!("{service:?}");
        assert!(debug.contains("MemoryService"));
        assert!(debug.contains("config"));
    }

    #[test]
    fn test_service_clone() {
        let service = setup_service();
        let cloned = service.clone();
        assert_eq!(cloned.config.rag_limit, service.config.rag_limit);
    }
}
