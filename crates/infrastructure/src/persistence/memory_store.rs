//! SQLite memory store implementation
//!
//! Implements the MemoryStore port using SQLite with FTS5 for full-text search
//! and vector similarity search via cosine similarity calculations.

use std::sync::Arc;

use application::{
    error::ApplicationError,
    ports::{MemoryStats, MemoryStore, SimilarMemory},
};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use domain::{Memory, MemoryId, MemoryQuery, MemoryType, UserId};
use rusqlite::{OptionalExtension, Row, params};
use tokio::task;
use tracing::{debug, instrument};

use super::connection::ConnectionPool;

/// SQLite-based memory store
#[derive(Debug, Clone)]
pub struct SqliteMemoryStore {
    pool: Arc<ConnectionPool>,
}

impl SqliteMemoryStore {
    /// Create a new SQLite memory store
    #[must_use]
    pub const fn new(pool: Arc<ConnectionPool>) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl MemoryStore for SqliteMemoryStore {
    #[instrument(skip(self, memory), fields(memory_id = %memory.id))]
    async fn save(&self, memory: &Memory) -> Result<(), ApplicationError> {
        let pool = Arc::clone(&self.pool);
        let memory = memory.clone();

        task::spawn_blocking(move || -> Result<(), ApplicationError> {
            let conn = pool
                .get()
                .map_err(|e| ApplicationError::Internal(e.to_string()))?;

            // Serialize tags to JSON
            let tags_json = serde_json::to_string(&memory.tags)
                .map_err(|e| ApplicationError::Internal(format!("Failed to serialize tags: {e}")))?;

            // Insert the memory
            conn.execute(
                "INSERT INTO memories (id, user_id, conversation_id, content, summary, importance, memory_type, tags, created_at, accessed_at, access_count)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
                params![
                    memory.id.to_string(),
                    memory.user_id.to_string(),
                    memory.conversation_id.map(|c| c.to_string()),
                    memory.content,
                    memory.summary,
                    memory.importance,
                    memory_type_to_str(memory.memory_type),
                    tags_json,
                    memory.created_at.to_rfc3339(),
                    memory.accessed_at.to_rfc3339(),
                    memory.access_count,
                ],
            )
            .map_err(|e| ApplicationError::Internal(e.to_string()))?;

            // If embedding exists, store it separately
            if let Some(ref embedding) = memory.embedding {
                let embedding_bytes = embedding_to_bytes(embedding);
                conn.execute(
                    "INSERT INTO memory_embeddings (memory_id, embedding, dimensions, model, created_at)
                     VALUES (?1, ?2, ?3, ?4, ?5)",
                    params![
                        memory.id.to_string(),
                        embedding_bytes,
                        embedding.len(),
                        "nomic-embed-text", // Default model
                        Utc::now().to_rfc3339(),
                    ],
                )
                .map_err(|e| ApplicationError::Internal(e.to_string()))?;
            }

            debug!("Saved memory");
            Ok(())
        })
        .await
        .map_err(|e: tokio::task::JoinError| ApplicationError::Internal(e.to_string()))?
    }

    #[instrument(skip(self), fields(memory_id = %id))]
    async fn get(&self, id: &MemoryId) -> Result<Option<Memory>, ApplicationError> {
        let pool = Arc::clone(&self.pool);
        let id_str = id.to_string();

        task::spawn_blocking(move || -> Result<Option<Memory>, ApplicationError> {
            let conn = pool
                .get()
                .map_err(|e| ApplicationError::Internal(e.to_string()))?;

            let memory = conn
                .query_row(
                    "SELECT m.id, m.user_id, m.conversation_id, m.content, m.summary, m.importance, 
                            m.memory_type, m.tags, m.created_at, m.accessed_at, m.access_count,
                            e.embedding
                     FROM memories m
                     LEFT JOIN memory_embeddings e ON m.id = e.memory_id
                     WHERE m.id = ?1",
                    [&id_str],
                    row_to_memory,
                )
                .optional()
                .map_err(|e: rusqlite::Error| ApplicationError::Internal(e.to_string()))?;

            Ok(memory)
        })
        .await
        .map_err(|e: tokio::task::JoinError| ApplicationError::Internal(e.to_string()))?
    }

    #[instrument(skip(self, memory), fields(memory_id = %memory.id))]
    async fn update(&self, memory: &Memory) -> Result<(), ApplicationError> {
        let pool = Arc::clone(&self.pool);
        let memory = memory.clone();

        task::spawn_blocking(move || -> Result<(), ApplicationError> {
            let conn = pool
                .get()
                .map_err(|e| ApplicationError::Internal(e.to_string()))?;

            let tags_json = serde_json::to_string(&memory.tags)
                .map_err(|e| ApplicationError::Internal(format!("Failed to serialize tags: {e}")))?;

            conn.execute(
                "UPDATE memories SET content = ?1, summary = ?2, importance = ?3, memory_type = ?4,
                        tags = ?5, accessed_at = ?6, access_count = ?7
                 WHERE id = ?8",
                params![
                    memory.content,
                    memory.summary,
                    memory.importance,
                    memory_type_to_str(memory.memory_type),
                    tags_json,
                    memory.accessed_at.to_rfc3339(),
                    memory.access_count,
                    memory.id.to_string(),
                ],
            )
            .map_err(|e| ApplicationError::Internal(e.to_string()))?;

            // Update or insert embedding if present
            if let Some(ref embedding) = memory.embedding {
                let embedding_bytes = embedding_to_bytes(embedding);
                conn.execute(
                    "INSERT OR REPLACE INTO memory_embeddings (memory_id, embedding, dimensions, model, created_at)
                     VALUES (?1, ?2, ?3, ?4, ?5)",
                    params![
                        memory.id.to_string(),
                        embedding_bytes,
                        embedding.len(),
                        "nomic-embed-text",
                        Utc::now().to_rfc3339(),
                    ],
                )
                .map_err(|e| ApplicationError::Internal(e.to_string()))?;
            }

            debug!("Updated memory");
            Ok(())
        })
        .await
        .map_err(|e: tokio::task::JoinError| ApplicationError::Internal(e.to_string()))?
    }

    #[instrument(skip(self), fields(memory_id = %id))]
    async fn delete(&self, id: &MemoryId) -> Result<(), ApplicationError> {
        let pool = Arc::clone(&self.pool);
        let id_str = id.to_string();

        task::spawn_blocking(move || -> Result<(), ApplicationError> {
            let conn = pool
                .get()
                .map_err(|e| ApplicationError::Internal(e.to_string()))?;

            // Delete embedding first (cascade should handle this, but being explicit)
            conn.execute(
                "DELETE FROM memory_embeddings WHERE memory_id = ?1",
                [&id_str],
            )
            .map_err(|e| ApplicationError::Internal(e.to_string()))?;

            // Delete memory
            conn.execute("DELETE FROM memories WHERE id = ?1", [&id_str])
                .map_err(|e| ApplicationError::Internal(e.to_string()))?;

            debug!("Deleted memory");
            Ok(())
        })
        .await
        .map_err(|e: tokio::task::JoinError| ApplicationError::Internal(e.to_string()))?
    }

    #[instrument(skip(self, embedding), fields(user_id = %user_id, limit = limit))]
    async fn search_similar(
        &self,
        user_id: &UserId,
        embedding: &[f32],
        limit: usize,
        min_similarity: f32,
    ) -> Result<Vec<SimilarMemory>, ApplicationError> {
        let pool = Arc::clone(&self.pool);
        let user_id_str = user_id.to_string();
        let query_embedding = embedding.to_vec();

        task::spawn_blocking(move || -> Result<Vec<SimilarMemory>, ApplicationError> {
            let conn = pool
                .get()
                .map_err(|e| ApplicationError::Internal(e.to_string()))?;

            // Get all memories with embeddings for this user
            let mut stmt = conn
                .prepare(
                    "SELECT m.id, m.user_id, m.conversation_id, m.content, m.summary, m.importance,
                            m.memory_type, m.tags, m.created_at, m.accessed_at, m.access_count,
                            e.embedding
                     FROM memories m
                     INNER JOIN memory_embeddings e ON m.id = e.memory_id
                     WHERE m.user_id = ?1",
                )
                .map_err(|e| ApplicationError::Internal(e.to_string()))?;

            let memories: Vec<Memory> = stmt
                .query_map([&user_id_str], row_to_memory)
                .map_err(|e| ApplicationError::Internal(e.to_string()))?
                .filter_map(Result::ok)
                .collect();

            // Calculate similarities and filter
            let mut similar: Vec<SimilarMemory> = memories
                .into_iter()
                .filter_map(|memory| {
                    let emb = memory.embedding.clone()?;
                    let similarity = cosine_similarity(&query_embedding, &emb);
                    Some((memory, similarity))
                })
                .filter(|(_, similarity)| *similarity >= min_similarity)
                .map(|(memory, similarity)| SimilarMemory::new(memory, similarity))
                .collect();

            // Sort by relevance score (similarity * 0.7 + importance * 0.3)
            similar.sort_by(|a, b| {
                b.relevance_score()
                    .partial_cmp(&a.relevance_score())
                    .unwrap_or(std::cmp::Ordering::Equal)
            });

            // Limit results
            similar.truncate(limit);

            debug!(found = similar.len(), "Found similar memories");
            Ok(similar)
        })
        .await
        .map_err(|e: tokio::task::JoinError| ApplicationError::Internal(e.to_string()))?
    }

    #[instrument(skip(self, query))]
    async fn list(&self, query: &MemoryQuery) -> Result<Vec<Memory>, ApplicationError> {
        let pool = Arc::clone(&self.pool);
        let query = query.clone();

        task::spawn_blocking(move || -> Result<Vec<Memory>, ApplicationError> {
            let conn = pool
                .get()
                .map_err(|e| ApplicationError::Internal(e.to_string()))?;

            let mut sql = String::from(
                "SELECT m.id, m.user_id, m.conversation_id, m.content, m.summary, m.importance,
                        m.memory_type, m.tags, m.created_at, m.accessed_at, m.access_count,
                        e.embedding
                 FROM memories m
                 LEFT JOIN memory_embeddings e ON m.id = e.memory_id
                 WHERE 1=1",
            );

            let mut params: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();

            if let Some(ref user_id) = query.user_id {
                sql.push_str(&format!(" AND m.user_id = ?{}", params.len() + 1));
                params.push(Box::new(user_id.to_string()));
            }

            if let Some(ref conversation_id) = query.conversation_id {
                sql.push_str(&format!(" AND m.conversation_id = ?{}", params.len() + 1));
                params.push(Box::new(conversation_id.to_string()));
            }

            if let Some(ref types) = query.memory_types {
                if !types.is_empty() {
                    let placeholders: Vec<String> = types
                        .iter()
                        .enumerate()
                        .map(|(i, _)| format!("?{}", params.len() + i + 1))
                        .collect();
                    sql.push_str(&format!(
                        " AND m.memory_type IN ({})",
                        placeholders.join(", ")
                    ));
                    for t in types {
                        params.push(Box::new(memory_type_to_str(*t).to_string()));
                    }
                }
            }

            if let Some(min_importance) = query.min_importance {
                sql.push_str(&format!(" AND m.importance >= ?{}", params.len() + 1));
                params.push(Box::new(min_importance));
            }

            sql.push_str(" ORDER BY m.importance DESC, m.accessed_at DESC");

            if let Some(limit) = query.limit {
                sql.push_str(&format!(" LIMIT {limit}"));
            }

            let mut stmt = conn
                .prepare(&sql)
                .map_err(|e| ApplicationError::Internal(e.to_string()))?;

            let param_refs: Vec<&dyn rusqlite::ToSql> =
                params.iter().map(std::convert::AsRef::as_ref).collect();

            let memories: Vec<Memory> = stmt
                .query_map(param_refs.as_slice(), row_to_memory)
                .map_err(|e| ApplicationError::Internal(e.to_string()))?
                .filter_map(Result::ok)
                .collect();

            debug!(count = memories.len(), "Listed memories");
            Ok(memories)
        })
        .await
        .map_err(|e: tokio::task::JoinError| ApplicationError::Internal(e.to_string()))?
    }

    #[instrument(skip(self), fields(user_id = %user_id, memory_type = ?memory_type, limit = limit))]
    async fn list_by_type(
        &self,
        user_id: &UserId,
        memory_type: MemoryType,
        limit: usize,
    ) -> Result<Vec<Memory>, ApplicationError> {
        let query = MemoryQuery::new()
            .for_user(*user_id)
            .of_types(vec![memory_type])
            .limit(limit);

        self.list(&query).await
    }

    #[instrument(skip(self), fields(decay_rate = decay_rate))]
    async fn apply_decay(&self, decay_rate: f32) -> Result<Vec<MemoryId>, ApplicationError> {
        let pool = Arc::clone(&self.pool);

        task::spawn_blocking(move || -> Result<Vec<MemoryId>, ApplicationError> {
            let conn = pool
                .get()
                .map_err(|e| ApplicationError::Internal(e.to_string()))?;

            // Get all memories
            let mut stmt = conn
                .prepare("SELECT id, importance, accessed_at, access_count FROM memories")
                .map_err(|e| ApplicationError::Internal(e.to_string()))?;

            let rows: Vec<(String, f32, String, u32)> = stmt
                .query_map([], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, f32>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, u32>(3)?,
                    ))
                })
                .map_err(|e| ApplicationError::Internal(e.to_string()))?
                .filter_map(Result::ok)
                .collect();

            let mut below_threshold = Vec::new();

            for (id, importance, accessed_at_str, access_count) in rows {
                let accessed_at = DateTime::parse_from_rfc3339(&accessed_at_str)
                    .map_or_else(|_| Utc::now(), |dt| dt.with_timezone(&Utc));

                #[allow(clippy::cast_precision_loss)] // Days count won't exceed f32 precision
                let days_since_access =
                    Utc::now().signed_duration_since(accessed_at).num_days() as f32;

                // Exponential decay with access boost
                let decay_factor = (-decay_rate * days_since_access).exp();
                let mut new_importance = importance * decay_factor;

                // Add access boost
                #[allow(clippy::cast_precision_loss)] // Access count won't exceed f32 precision
                let access_boost = (access_count as f32 * 0.01).min(0.1);
                new_importance = (new_importance + access_boost).min(1.0);

                // Update importance
                conn.execute(
                    "UPDATE memories SET importance = ?1 WHERE id = ?2",
                    params![new_importance, id],
                )
                .map_err(|e| ApplicationError::Internal(e.to_string()))?;

                // Check if below threshold
                if new_importance < Memory::MIN_IMPORTANCE {
                    if let Ok(memory_id) = MemoryId::parse(&id) {
                        below_threshold.push(memory_id);
                    }
                }
            }

            debug!(decayed = below_threshold.len(), "Applied decay to memories");
            Ok(below_threshold)
        })
        .await
        .map_err(|e: tokio::task::JoinError| ApplicationError::Internal(e.to_string()))?
    }

    #[instrument(skip(self), fields(threshold = threshold))]
    async fn cleanup_below_threshold(&self, threshold: f32) -> Result<usize, ApplicationError> {
        let pool = Arc::clone(&self.pool);

        task::spawn_blocking(move || -> Result<usize, ApplicationError> {
            let conn = pool
                .get()
                .map_err(|e| ApplicationError::Internal(e.to_string()))?;

            // Delete embeddings for memories below threshold first
            conn.execute(
                "DELETE FROM memory_embeddings WHERE memory_id IN (SELECT id FROM memories WHERE importance < ?1)",
                params![threshold],
            )
            .map_err(|e| ApplicationError::Internal(e.to_string()))?;

            // Delete memories below threshold
            let deleted = conn
                .execute(
                    "DELETE FROM memories WHERE importance < ?1",
                    params![threshold],
                )
                .map_err(|e| ApplicationError::Internal(e.to_string()))?;

            debug!(deleted = deleted, "Cleaned up low-importance memories");
            Ok(deleted)
        })
        .await
        .map_err(|e: tokio::task::JoinError| ApplicationError::Internal(e.to_string()))?
    }

    #[instrument(skip(self, memory), fields(memory_id = %memory.id))]
    async fn find_merge_candidates(
        &self,
        memory: &Memory,
        similarity_threshold: f32,
    ) -> Result<Vec<SimilarMemory>, ApplicationError> {
        if let Some(ref embedding) = memory.embedding {
            self.search_similar(&memory.user_id, embedding, 10, similarity_threshold)
                .await
        } else {
            Ok(Vec::new())
        }
    }

    #[instrument(skip(self), fields(user_id = %user_id))]
    async fn stats(&self, user_id: &UserId) -> Result<MemoryStats, ApplicationError> {
        let pool = Arc::clone(&self.pool);
        let user_id_str = user_id.to_string();

        task::spawn_blocking(move || -> Result<MemoryStats, ApplicationError> {
            let conn = pool
                .get()
                .map_err(|e| ApplicationError::Internal(e.to_string()))?;

            // Total count
            let total_count: usize = conn
                .query_row(
                    "SELECT COUNT(*) FROM memories WHERE user_id = ?1",
                    [&user_id_str],
                    |row| row.get(0),
                )
                .map_err(|e| ApplicationError::Internal(e.to_string()))?;

            // Count by type
            let mut by_type = Vec::new();
            for memory_type in MemoryType::all() {
                let count: usize = conn
                    .query_row(
                        "SELECT COUNT(*) FROM memories WHERE user_id = ?1 AND memory_type = ?2",
                        params![&user_id_str, memory_type_to_str(*memory_type)],
                        |row| row.get(0),
                    )
                    .map_err(|e| ApplicationError::Internal(e.to_string()))?;
                by_type.push((*memory_type, count));
            }

            // Count with embeddings
            let with_embeddings: usize = conn
                .query_row(
                    "SELECT COUNT(*) FROM memories m
                     INNER JOIN memory_embeddings e ON m.id = e.memory_id
                     WHERE m.user_id = ?1",
                    [&user_id_str],
                    |row| row.get(0),
                )
                .map_err(|e| ApplicationError::Internal(e.to_string()))?;

            // Average importance
            let avg_importance: f32 = conn
                .query_row(
                    "SELECT COALESCE(AVG(importance), 0.0) FROM memories WHERE user_id = ?1",
                    [&user_id_str],
                    |row| row.get(0),
                )
                .map_err(|e| ApplicationError::Internal(e.to_string()))?;

            Ok(MemoryStats {
                total_count,
                by_type,
                with_embeddings,
                avg_importance,
            })
        })
        .await
        .map_err(|e: tokio::task::JoinError| ApplicationError::Internal(e.to_string()))?
    }

    #[instrument(skip(self), fields(memory_id = %id))]
    async fn record_access(&self, id: &MemoryId) -> Result<(), ApplicationError> {
        let pool = Arc::clone(&self.pool);
        let id_str = id.to_string();

        task::spawn_blocking(move || -> Result<(), ApplicationError> {
            let conn = pool
                .get()
                .map_err(|e| ApplicationError::Internal(e.to_string()))?;

            conn.execute(
                "UPDATE memories SET accessed_at = ?1, access_count = access_count + 1 WHERE id = ?2",
                params![Utc::now().to_rfc3339(), id_str],
            )
            .map_err(|e| ApplicationError::Internal(e.to_string()))?;

            debug!("Recorded memory access");
            Ok(())
        })
        .await
        .map_err(|e| ApplicationError::Internal(e.to_string()))?
    }
}

/// Convert a database row to a Memory entity
fn row_to_memory(row: &Row<'_>) -> Result<Memory, rusqlite::Error> {
    let id_str: String = row.get(0)?;
    let user_id_str: String = row.get(1)?;
    let conversation_id_str: Option<String> = row.get(2)?;
    let content: String = row.get(3)?;
    let summary: String = row.get(4)?;
    let importance: f32 = row.get(5)?;
    let memory_type_str: String = row.get(6)?;
    let tags_json: String = row.get(7)?;
    let created_at_str: String = row.get(8)?;
    let accessed_at_str: String = row.get(9)?;
    let access_count: u32 = row.get(10)?;
    let embedding_bytes: Option<Vec<u8>> = row.get(11)?;

    let id = MemoryId::parse(&id_str).unwrap_or_else(|_| MemoryId::new());
    let user_id = UserId::parse(&user_id_str).unwrap_or_else(|_| UserId::new());
    let conversation_id =
        conversation_id_str.and_then(|s| domain::value_objects::ConversationId::parse(&s).ok());

    let memory_type = str_to_memory_type(&memory_type_str);

    let tags: Vec<String> = serde_json::from_str(&tags_json).unwrap_or_default();

    let created_at = DateTime::parse_from_rfc3339(&created_at_str)
        .map_or_else(|_| Utc::now(), |dt| dt.with_timezone(&Utc));

    let accessed_at = DateTime::parse_from_rfc3339(&accessed_at_str)
        .map_or_else(|_| Utc::now(), |dt| dt.with_timezone(&Utc));

    let embedding = embedding_bytes.map(|bytes| bytes_to_embedding(&bytes));

    let mut memory = Memory::new(user_id, content, summary, memory_type)
        .with_id(id)
        .with_importance(importance)
        .with_tags(tags)
        .with_created_at(created_at)
        .with_accessed_at(accessed_at)
        .with_access_count(access_count);

    if let Some(conv_id) = conversation_id {
        memory = memory.with_conversation(conv_id);
    }

    if let Some(emb) = embedding {
        memory = memory.with_embedding(emb);
    }

    Ok(memory)
}

/// Convert MemoryType to string for storage
const fn memory_type_to_str(memory_type: MemoryType) -> &'static str {
    match memory_type {
        MemoryType::Fact => "fact",
        MemoryType::Preference => "preference",
        MemoryType::ToolResult => "tool_result",
        MemoryType::Correction => "correction",
        MemoryType::Context => "context",
    }
}

/// Convert string to MemoryType
fn str_to_memory_type(s: &str) -> MemoryType {
    match s {
        "fact" => MemoryType::Fact,
        "preference" => MemoryType::Preference,
        "tool_result" => MemoryType::ToolResult,
        "correction" => MemoryType::Correction,
        _ => MemoryType::Context, // Default fallback including "context"
    }
}

/// Convert embedding vector to bytes for storage
fn embedding_to_bytes(embedding: &[f32]) -> Vec<u8> {
    embedding.iter().flat_map(|f| f.to_le_bytes()).collect()
}

/// Convert bytes back to embedding vector
fn bytes_to_embedding(bytes: &[u8]) -> Vec<f32> {
    bytes
        .chunks_exact(4)
        .map(|chunk| {
            let arr: [u8; 4] = chunk.try_into().unwrap_or([0; 4]);
            f32::from_le_bytes(arr)
        })
        .collect()
}

/// Calculate cosine similarity between two vectors
fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }

    let dot_product: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();

    if norm_a == 0.0 || norm_b == 0.0 {
        return 0.0;
    }

    dot_product / (norm_a * norm_b)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn memory_type_roundtrip() {
        for memory_type in MemoryType::all() {
            let str_repr = memory_type_to_str(*memory_type);
            let parsed = str_to_memory_type(str_repr);
            assert_eq!(*memory_type, parsed);
        }
    }

    #[test]
    fn embedding_serialization_roundtrip() {
        let embedding = vec![0.1, 0.2, -0.3, 0.4, 0.5];
        let bytes = embedding_to_bytes(&embedding);
        let restored = bytes_to_embedding(&bytes);

        assert_eq!(embedding.len(), restored.len());
        for (a, b) in embedding.iter().zip(restored.iter()) {
            assert!((a - b).abs() < 0.0001);
        }
    }

    #[test]
    fn cosine_similarity_identical() {
        let vec = vec![1.0, 0.0, 0.0];
        let similarity = cosine_similarity(&vec, &vec);
        assert!((similarity - 1.0).abs() < 0.001);
    }

    #[test]
    fn cosine_similarity_orthogonal() {
        let a = vec![1.0, 0.0, 0.0];
        let b = vec![0.0, 1.0, 0.0];
        let similarity = cosine_similarity(&a, &b);
        assert!(similarity.abs() < 0.001);
    }

    #[test]
    fn cosine_similarity_opposite() {
        let a = vec![1.0, 0.0, 0.0];
        let b = vec![-1.0, 0.0, 0.0];
        let similarity = cosine_similarity(&a, &b);
        assert!((similarity + 1.0).abs() < 0.001);
    }

    #[test]
    fn cosine_similarity_empty() {
        let a: Vec<f32> = vec![];
        let b: Vec<f32> = vec![];
        assert!(cosine_similarity(&a, &b).abs() < f32::EPSILON);
    }

    #[test]
    fn cosine_similarity_different_lengths() {
        let a = vec![1.0, 0.0];
        let b = vec![1.0, 0.0, 0.0];
        assert!(cosine_similarity(&a, &b).abs() < f32::EPSILON);
    }
}
