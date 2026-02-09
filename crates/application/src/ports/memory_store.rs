//! Memory storage port - Interface for persisting and retrieving AI memories
//!
//! This port defines how the application layer interacts with memory storage,
//! supporting both traditional queries and vector-based semantic search.

use async_trait::async_trait;
#[cfg(test)]
use mockall::automock;

use domain::{Memory, MemoryId, MemoryQuery, MemoryType, UserId};

use crate::error::ApplicationError;

/// Result of a similarity search containing memory and its similarity score
#[derive(Debug, Clone)]
pub struct SimilarMemory {
    /// The memory entry
    pub memory: Memory,
    /// Cosine similarity score (0.0 - 1.0)
    pub similarity: f32,
}

impl SimilarMemory {
    /// Create a new similar memory result
    #[must_use]
    pub const fn new(memory: Memory, similarity: f32) -> Self {
        Self { memory, similarity }
    }

    /// Calculate the combined relevance score
    #[must_use]
    pub fn relevance_score(&self) -> f32 {
        self.memory.relevance_score(self.similarity)
    }
}

/// Statistics about the memory store
#[derive(Debug, Clone, Default)]
pub struct MemoryStats {
    /// Total number of memories
    pub total_count: usize,
    /// Count by memory type
    pub by_type: Vec<(MemoryType, usize)>,
    /// Count of memories with embeddings
    pub with_embeddings: usize,
    /// Average importance score
    pub avg_importance: f32,
}

/// Port for memory persistence and retrieval
#[cfg_attr(test, automock)]
#[async_trait]
pub trait MemoryStore: Send + Sync {
    /// Save a new memory entry
    async fn save(&self, memory: &Memory) -> Result<(), ApplicationError>;

    /// Get a memory by ID
    async fn get(&self, id: &MemoryId) -> Result<Option<Memory>, ApplicationError>;

    /// Update an existing memory
    async fn update(&self, memory: &Memory) -> Result<(), ApplicationError>;

    /// Delete a memory by ID
    async fn delete(&self, id: &MemoryId) -> Result<(), ApplicationError>;

    /// Find similar memories using vector search
    ///
    /// Returns memories sorted by similarity score (highest first).
    async fn search_similar(
        &self,
        user_id: &UserId,
        embedding: &[f32],
        limit: usize,
        min_similarity: f32,
    ) -> Result<Vec<SimilarMemory>, ApplicationError>;

    /// List memories matching the query criteria
    async fn list(&self, query: &MemoryQuery) -> Result<Vec<Memory>, ApplicationError>;

    /// List memories by type for a user
    async fn list_by_type(
        &self,
        user_id: &UserId,
        memory_type: MemoryType,
        limit: usize,
    ) -> Result<Vec<Memory>, ApplicationError>;

    /// Apply decay to all memories and return IDs of memories below threshold
    ///
    /// This should update importance scores based on time since last access.
    async fn apply_decay(&self, decay_rate: f32) -> Result<Vec<MemoryId>, ApplicationError>;

    /// Delete memories below the importance threshold
    async fn cleanup_below_threshold(&self, threshold: f32) -> Result<usize, ApplicationError>;

    /// Find memories similar to the given one for potential merging
    async fn find_merge_candidates(
        &self,
        memory: &Memory,
        similarity_threshold: f32,
    ) -> Result<Vec<SimilarMemory>, ApplicationError>;

    /// Get statistics about stored memories for a user
    async fn stats(&self, user_id: &UserId) -> Result<MemoryStats, ApplicationError>;

    /// Record that a memory was accessed (updates accessed_at and access_count)
    async fn record_access(&self, id: &MemoryId) -> Result<(), ApplicationError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn similar_memory_relevance_score() {
        let user_id = UserId::parse("550e8400-e29b-41d4-a716-446655440000").unwrap();
        let memory =
            Memory::new(user_id, "content", "summary", MemoryType::Fact).with_importance(0.8);

        let similar = SimilarMemory::new(memory, 0.9);

        // 0.9 * 0.7 + 0.8 * 0.3 = 0.63 + 0.24 = 0.87
        let score = similar.relevance_score();
        assert!((score - 0.87).abs() < 0.01);
    }

    #[test]
    fn memory_stats_default() {
        let stats = MemoryStats::default();
        assert_eq!(stats.total_count, 0);
        assert!(stats.by_type.is_empty());
        assert_eq!(stats.with_embeddings, 0);
        assert!(stats.avg_importance.abs() < f32::EPSILON);
    }
}
