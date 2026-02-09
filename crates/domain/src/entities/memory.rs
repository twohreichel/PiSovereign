//! Memory entity - Stores knowledge and learned information for AI context retrieval

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::value_objects::{ConversationId, MemoryId, UserId};

/// Type of memory content
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryType {
    /// Factual knowledge extracted from conversations
    Fact,
    /// User preferences learned over time
    Preference,
    /// Results from tool executions (weather, calendar, web search, etc.)
    ToolResult,
    /// Corrections made by the user to AI responses
    Correction,
    /// General contextual information
    Context,
}

impl MemoryType {
    /// Returns all memory types for iteration
    #[must_use]
    pub const fn all() -> &'static [Self] {
        &[
            Self::Fact,
            Self::Preference,
            Self::ToolResult,
            Self::Correction,
            Self::Context,
        ]
    }

    /// Get a human-readable label for the memory type
    #[must_use]
    pub const fn label(&self) -> &'static str {
        match self {
            Self::Fact => "Fact",
            Self::Preference => "Preference",
            Self::ToolResult => "Tool Result",
            Self::Correction => "Correction",
            Self::Context => "Context",
        }
    }
}

impl std::fmt::Display for MemoryType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.label())
    }
}

/// A memory entry storing knowledge for AI context retrieval
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Memory {
    /// Unique identifier for this memory
    pub id: MemoryId,
    /// User who owns this memory
    pub user_id: UserId,
    /// Optional conversation this memory originated from
    #[serde(skip_serializing_if = "Option::is_none")]
    pub conversation_id: Option<ConversationId>,
    /// The actual content (may be encrypted at rest)
    pub content: String,
    /// Summary or title of the memory for quick reference
    pub summary: String,
    /// Vector embedding for semantic search (384 dimensions for nomic-embed-text)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub embedding: Option<Vec<f32>>,
    /// Importance score (0.0 - 1.0), used for relevance ranking and decay
    pub importance: f32,
    /// Type of memory
    pub memory_type: MemoryType,
    /// Tags for categorical filtering
    #[serde(default)]
    pub tags: Vec<String>,
    /// When the memory was created
    pub created_at: DateTime<Utc>,
    /// When the memory was last accessed
    pub accessed_at: DateTime<Utc>,
    /// Number of times this memory has been accessed
    pub access_count: u32,
}

impl Memory {
    /// Default importance for new memories
    pub const DEFAULT_IMPORTANCE: f32 = 0.5;

    /// Minimum importance before memory is considered for cleanup
    pub const MIN_IMPORTANCE: f32 = 0.1;

    /// Maximum importance value
    pub const MAX_IMPORTANCE: f32 = 1.0;

    /// Create a new memory entry
    #[must_use]
    pub fn new(
        user_id: UserId,
        content: impl Into<String>,
        summary: impl Into<String>,
        memory_type: MemoryType,
    ) -> Self {
        let now = Utc::now();
        Self {
            id: MemoryId::new(),
            user_id,
            conversation_id: None,
            content: content.into(),
            summary: summary.into(),
            embedding: None,
            importance: Self::DEFAULT_IMPORTANCE,
            memory_type,
            tags: Vec::new(),
            created_at: now,
            accessed_at: now,
            access_count: 0,
        }
    }

    /// Set the conversation this memory originated from
    #[must_use]
    pub fn with_conversation(mut self, conversation_id: ConversationId) -> Self {
        self.conversation_id = Some(conversation_id);
        self
    }

    /// Set the embedding vector
    #[must_use]
    pub fn with_embedding(mut self, embedding: Vec<f32>) -> Self {
        self.embedding = Some(embedding);
        self
    }

    /// Set the importance score
    #[must_use]
    pub fn with_importance(mut self, importance: f32) -> Self {
        self.importance = importance.clamp(0.0, Self::MAX_IMPORTANCE);
        self
    }

    /// Add tags to the memory
    #[must_use]
    pub fn with_tags(mut self, tags: Vec<String>) -> Self {
        self.tags = tags;
        self
    }

    /// Set specific ID (useful for restoration from database)
    #[must_use]
    pub const fn with_id(mut self, id: MemoryId) -> Self {
        self.id = id;
        self
    }

    /// Set created_at timestamp (useful for restoration from database)
    #[must_use]
    pub const fn with_created_at(mut self, created_at: DateTime<Utc>) -> Self {
        self.created_at = created_at;
        self
    }

    /// Set accessed_at timestamp (useful for restoration from database)
    #[must_use]
    pub const fn with_accessed_at(mut self, accessed_at: DateTime<Utc>) -> Self {
        self.accessed_at = accessed_at;
        self
    }

    /// Set access count (useful for restoration from database)
    #[must_use]
    pub const fn with_access_count(mut self, access_count: u32) -> Self {
        self.access_count = access_count;
        self
    }

    /// Record that this memory was accessed
    pub fn record_access(&mut self) {
        self.accessed_at = Utc::now();
        self.access_count = self.access_count.saturating_add(1);
    }

    /// Apply time-based decay to importance
    ///
    /// Reduces importance based on time since last access and decay rate.
    /// Returns true if the memory is still above minimum importance threshold.
    pub fn apply_decay(&mut self, decay_rate: f32) -> bool {
        let days_since_access = Utc::now()
            .signed_duration_since(self.accessed_at)
            .num_days() as f32;

        // Exponential decay: importance * e^(-decay_rate * days)
        let decay_factor = (-decay_rate * days_since_access).exp();
        self.importance *= decay_factor;

        // Boost importance slightly based on access frequency
        let access_boost = (self.access_count as f32 * 0.01).min(0.1);
        self.importance = (self.importance + access_boost).min(Self::MAX_IMPORTANCE);

        self.importance >= Self::MIN_IMPORTANCE
    }

    /// Check if memory should be considered for cleanup
    #[must_use]
    pub fn below_importance_threshold(&self) -> bool {
        self.importance < Self::MIN_IMPORTANCE
    }

    /// Calculate relevance score combining similarity and importance
    #[must_use]
    pub fn relevance_score(&self, similarity: f32) -> f32 {
        // 70% similarity, 30% importance
        (similarity * 0.7) + (self.importance * 0.3)
    }

    /// Check if this memory has an embedding
    #[must_use]
    pub fn has_embedding(&self) -> bool {
        self.embedding.is_some()
    }

    /// Get the embedding dimension count
    #[must_use]
    pub fn embedding_dimensions(&self) -> Option<usize> {
        self.embedding.as_ref().map(Vec::len)
    }
}

/// Query parameters for searching memories
#[derive(Debug, Clone, Default)]
pub struct MemoryQuery {
    /// User to search memories for
    pub user_id: Option<UserId>,
    /// Optional conversation filter
    pub conversation_id: Option<ConversationId>,
    /// Filter by memory types
    pub memory_types: Option<Vec<MemoryType>>,
    /// Filter by tags (any match)
    pub tags: Option<Vec<String>>,
    /// Minimum importance threshold
    pub min_importance: Option<f32>,
    /// Maximum number of results
    pub limit: Option<usize>,
    /// Text to search for (semantic or keyword)
    pub query_text: Option<String>,
}

impl MemoryQuery {
    /// Create a new memory query
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Filter by user
    #[must_use]
    pub const fn for_user(mut self, user_id: UserId) -> Self {
        self.user_id = Some(user_id);
        self
    }

    /// Filter by conversation
    #[must_use]
    pub const fn in_conversation(mut self, conversation_id: ConversationId) -> Self {
        self.conversation_id = Some(conversation_id);
        self
    }

    /// Filter by memory types
    #[must_use]
    pub fn of_types(mut self, types: Vec<MemoryType>) -> Self {
        self.memory_types = Some(types);
        self
    }

    /// Filter by tags
    #[must_use]
    pub fn with_tags(mut self, tags: Vec<String>) -> Self {
        self.tags = Some(tags);
        self
    }

    /// Set minimum importance
    #[must_use]
    pub const fn min_importance(mut self, importance: f32) -> Self {
        self.min_importance = Some(importance);
        self
    }

    /// Limit results
    #[must_use]
    pub const fn limit(mut self, limit: usize) -> Self {
        self.limit = Some(limit);
        self
    }

    /// Set search text
    #[must_use]
    pub fn search(mut self, query: impl Into<String>) -> Self {
        self.query_text = Some(query.into());
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_user_id() -> UserId {
        UserId::parse("550e8400-e29b-41d4-a716-446655440000").unwrap()
    }

    #[test]
    fn memory_type_all_returns_all_variants() {
        let types = MemoryType::all();
        assert_eq!(types.len(), 5);
        assert!(types.contains(&MemoryType::Fact));
        assert!(types.contains(&MemoryType::Preference));
        assert!(types.contains(&MemoryType::ToolResult));
        assert!(types.contains(&MemoryType::Correction));
        assert!(types.contains(&MemoryType::Context));
    }

    #[test]
    fn memory_type_display() {
        assert_eq!(MemoryType::Fact.to_string(), "Fact");
        assert_eq!(MemoryType::ToolResult.to_string(), "Tool Result");
    }

    #[test]
    fn new_memory_has_correct_defaults() {
        let memory = Memory::new(
            test_user_id(),
            "Test content",
            "Test summary",
            MemoryType::Fact,
        );

        assert_eq!(memory.content, "Test content");
        assert_eq!(memory.summary, "Test summary");
        assert_eq!(memory.memory_type, MemoryType::Fact);
        assert!((memory.importance - Memory::DEFAULT_IMPORTANCE).abs() < f32::EPSILON);
        assert!(memory.tags.is_empty());
        assert_eq!(memory.access_count, 0);
        assert!(memory.embedding.is_none());
        assert!(memory.conversation_id.is_none());
    }

    #[test]
    fn builder_methods_work() {
        let conversation_id = ConversationId::new();
        let embedding = vec![0.1, 0.2, 0.3];

        let memory = Memory::new(
            test_user_id(),
            "Content",
            "Summary",
            MemoryType::Preference,
        )
        .with_conversation(conversation_id)
        .with_embedding(embedding.clone())
        .with_importance(0.9)
        .with_tags(vec!["tag1".to_string(), "tag2".to_string()]);

        assert_eq!(memory.conversation_id, Some(conversation_id));
        assert_eq!(memory.embedding, Some(embedding));
        assert!((memory.importance - 0.9).abs() < f32::EPSILON);
        assert_eq!(memory.tags.len(), 2);
    }

    #[test]
    fn importance_is_clamped() {
        let memory = Memory::new(test_user_id(), "c", "s", MemoryType::Fact).with_importance(1.5);
        assert!((memory.importance - 1.0).abs() < f32::EPSILON);

        let memory = Memory::new(test_user_id(), "c", "s", MemoryType::Fact).with_importance(-0.5);
        assert!(memory.importance.abs() < f32::EPSILON);
    }

    #[test]
    fn record_access_updates_stats() {
        let mut memory = Memory::new(test_user_id(), "c", "s", MemoryType::Fact);
        let original_accessed = memory.accessed_at;

        std::thread::sleep(std::time::Duration::from_millis(10));
        memory.record_access();

        assert!(memory.accessed_at > original_accessed);
        assert_eq!(memory.access_count, 1);

        memory.record_access();
        assert_eq!(memory.access_count, 2);
    }

    #[test]
    fn relevance_score_calculation() {
        let memory = Memory::new(test_user_id(), "c", "s", MemoryType::Fact).with_importance(0.8);

        let score = memory.relevance_score(0.9);
        // 0.9 * 0.7 + 0.8 * 0.3 = 0.63 + 0.24 = 0.87
        assert!((score - 0.87).abs() < 0.01);
    }

    #[test]
    fn below_importance_threshold() {
        let low_importance = Memory::new(test_user_id(), "c", "s", MemoryType::Fact)
            .with_importance(Memory::MIN_IMPORTANCE - 0.01);
        assert!(low_importance.below_importance_threshold());

        let high_importance =
            Memory::new(test_user_id(), "c", "s", MemoryType::Fact).with_importance(0.5);
        assert!(!high_importance.below_importance_threshold());
    }

    #[test]
    fn embedding_helpers() {
        let memory = Memory::new(test_user_id(), "c", "s", MemoryType::Fact);
        assert!(!memory.has_embedding());
        assert!(memory.embedding_dimensions().is_none());

        let with_embedding = memory.with_embedding(vec![0.1; 384]);
        assert!(with_embedding.has_embedding());
        assert_eq!(with_embedding.embedding_dimensions(), Some(384));
    }

    #[test]
    fn memory_query_builder() {
        let user_id = test_user_id();
        let conversation_id = ConversationId::new();

        let query = MemoryQuery::new()
            .for_user(user_id)
            .in_conversation(conversation_id)
            .of_types(vec![MemoryType::Fact, MemoryType::Preference])
            .with_tags(vec!["important".to_string()])
            .min_importance(0.5)
            .limit(10)
            .search("test query");

        assert_eq!(query.user_id, Some(user_id));
        assert_eq!(query.conversation_id, Some(conversation_id));
        assert_eq!(query.memory_types.as_ref().unwrap().len(), 2);
        assert_eq!(query.tags.as_ref().unwrap().len(), 1);
        assert!((query.min_importance.unwrap() - 0.5).abs() < f32::EPSILON);
        assert_eq!(query.limit, Some(10));
        assert_eq!(query.query_text, Some("test query".to_string()));
    }

    #[test]
    fn serialize_deserialize_memory() {
        let memory = Memory::new(
            test_user_id(),
            "Test content",
            "Test summary",
            MemoryType::Fact,
        )
        .with_importance(0.7)
        .with_tags(vec!["test".to_string()]);

        let serialized = serde_json::to_string(&memory).unwrap();
        let deserialized: Memory = serde_json::from_str(&serialized).unwrap();

        assert_eq!(memory.id, deserialized.id);
        assert_eq!(memory.content, deserialized.content);
        assert_eq!(memory.memory_type, deserialized.memory_type);
    }

    #[test]
    fn serialize_memory_type() {
        let fact = MemoryType::Fact;
        let serialized = serde_json::to_string(&fact).unwrap();
        assert_eq!(serialized, "\"fact\"");

        let tool_result = MemoryType::ToolResult;
        let serialized = serde_json::to_string(&tool_result).unwrap();
        assert_eq!(serialized, "\"tool_result\"");
    }
}
