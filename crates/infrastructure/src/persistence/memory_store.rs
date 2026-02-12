//! SQLite memory store implementation
//!
//! Implements the `MemoryStore` port using sqlx with vector similarity search
//! via cosine similarity calculations.

use application::{
    error::ApplicationError,
    ports::{MemoryStats, MemoryStore, SimilarMemory},
};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use domain::{Memory, MemoryId, MemoryQuery, MemoryType, UserId};
use sqlx::SqlitePool;
use tracing::{debug, instrument};

use super::error::map_sqlx_error;

/// SQLite-based memory store
#[derive(Debug, Clone)]
pub struct SqliteMemoryStore {
    pool: SqlitePool,
}

impl SqliteMemoryStore {
    /// Create a new SQLite memory store
    #[must_use]
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

/// Row type for memory queries (with optional embedding join)
#[derive(sqlx::FromRow)]
struct MemoryRow {
    id: String,
    user_id: String,
    conversation_id: Option<String>,
    content: String,
    summary: String,
    importance: f64,
    memory_type: String,
    tags: String,
    created_at: String,
    accessed_at: String,
    access_count: i64,
    embedding: Option<Vec<u8>>,
}

impl MemoryRow {
    fn to_memory(self) -> Memory {
        let id = MemoryId::parse(&self.id).unwrap_or_else(|_| MemoryId::new());
        let user_id = UserId::parse(&self.user_id).unwrap_or_else(|_| UserId::new());
        let conversation_id = self
            .conversation_id
            .and_then(|s| domain::value_objects::ConversationId::parse(&s).ok());

        let memory_type = str_to_memory_type(&self.memory_type);
        let tags: Vec<String> = serde_json::from_str(&self.tags).unwrap_or_default();

        let created_at = DateTime::parse_from_rfc3339(&self.created_at)
            .map_or_else(|_| Utc::now(), |dt| dt.with_timezone(&Utc));
        let accessed_at = DateTime::parse_from_rfc3339(&self.accessed_at)
            .map_or_else(|_| Utc::now(), |dt| dt.with_timezone(&Utc));

        let embedding = self.embedding.map(|bytes| bytes_to_embedding(&bytes));

        #[allow(clippy::cast_possible_truncation)]
        let importance = self.importance as f32;
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let access_count = self.access_count as u32;

        let mut memory = Memory::new(user_id, self.content, self.summary, memory_type)
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

        memory
    }
}

/// Row for decay calculations
#[derive(sqlx::FromRow)]
struct DecayRow {
    id: String,
    importance: f64,
    accessed_at: String,
    access_count: i64,
}

const SELECT_MEMORY: &str = "SELECT m.id, m.user_id, m.conversation_id, m.content, m.summary, \
                              m.importance, m.memory_type, m.tags, m.created_at, m.accessed_at, \
                              m.access_count, e.embedding
                              FROM memories m
                              LEFT JOIN memory_embeddings e ON m.id = e.memory_id";

const SELECT_MEMORY_INNER: &str =
    "SELECT m.id, m.user_id, m.conversation_id, m.content, m.summary, \
     m.importance, m.memory_type, m.tags, m.created_at, m.accessed_at, \
     m.access_count, e.embedding
     FROM memories m
     INNER JOIN memory_embeddings e ON m.id = e.memory_id";

#[async_trait]
impl MemoryStore for SqliteMemoryStore {
    #[instrument(skip(self, memory), fields(memory_id = %memory.id))]
    async fn save(&self, memory: &Memory) -> Result<(), ApplicationError> {
        let tags_json = serde_json::to_string(&memory.tags)
            .map_err(|e| ApplicationError::Internal(format!("Failed to serialize tags: {e}")))?;

        sqlx::query(
            "INSERT INTO memories (id, user_id, conversation_id, content, summary, importance, \
             memory_type, tags, created_at, accessed_at, access_count)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)",
        )
        .bind(memory.id.to_string())
        .bind(memory.user_id.to_string())
        .bind(memory.conversation_id.map(|c| c.to_string()))
        .bind(&memory.content)
        .bind(&memory.summary)
        .bind(f64::from(memory.importance))
        .bind(memory_type_to_str(memory.memory_type))
        .bind(&tags_json)
        .bind(memory.created_at.to_rfc3339())
        .bind(memory.accessed_at.to_rfc3339())
        .bind(memory.access_count)
        .execute(&self.pool)
        .await
        .map_err(map_sqlx_error)?;

        // If embedding exists, store separately
        if let Some(ref embedding) = memory.embedding {
            let embedding_bytes = embedding_to_bytes(embedding);
            sqlx::query(
                "INSERT INTO memory_embeddings (memory_id, embedding, dimensions, model, \
                 created_at)
                 VALUES ($1, $2, $3, $4, $5)",
            )
            .bind(memory.id.to_string())
            .bind(&embedding_bytes)
            .bind(embedding.len() as i64)
            .bind("nomic-embed-text")
            .bind(Utc::now().to_rfc3339())
            .execute(&self.pool)
            .await
            .map_err(map_sqlx_error)?;
        }

        debug!("Saved memory");
        Ok(())
    }

    #[instrument(skip(self), fields(memory_id = %id))]
    async fn get(&self, id: &MemoryId) -> Result<Option<Memory>, ApplicationError> {
        let sql = format!("{SELECT_MEMORY} WHERE m.id = $1");
        let row: Option<MemoryRow> = sqlx::query_as(&sql)
            .bind(id.to_string())
            .fetch_optional(&self.pool)
            .await
            .map_err(map_sqlx_error)?;

        Ok(row.map(MemoryRow::to_memory))
    }

    #[instrument(skip(self, memory), fields(memory_id = %memory.id))]
    async fn update(&self, memory: &Memory) -> Result<(), ApplicationError> {
        let tags_json = serde_json::to_string(&memory.tags)
            .map_err(|e| ApplicationError::Internal(format!("Failed to serialize tags: {e}")))?;

        sqlx::query(
            "UPDATE memories SET content = $1, summary = $2, importance = $3, memory_type = $4,
             tags = $5, accessed_at = $6, access_count = $7
             WHERE id = $8",
        )
        .bind(&memory.content)
        .bind(&memory.summary)
        .bind(f64::from(memory.importance))
        .bind(memory_type_to_str(memory.memory_type))
        .bind(&tags_json)
        .bind(memory.accessed_at.to_rfc3339())
        .bind(memory.access_count)
        .bind(memory.id.to_string())
        .execute(&self.pool)
        .await
        .map_err(map_sqlx_error)?;

        // Upsert embedding if present
        if let Some(ref embedding) = memory.embedding {
            let embedding_bytes = embedding_to_bytes(embedding);
            sqlx::query(
                "INSERT OR REPLACE INTO memory_embeddings \
                 (memory_id, embedding, dimensions, model, created_at)
                 VALUES ($1, $2, $3, $4, $5)",
            )
            .bind(memory.id.to_string())
            .bind(&embedding_bytes)
            .bind(embedding.len() as i64)
            .bind("nomic-embed-text")
            .bind(Utc::now().to_rfc3339())
            .execute(&self.pool)
            .await
            .map_err(map_sqlx_error)?;
        }

        debug!("Updated memory");
        Ok(())
    }

    #[instrument(skip(self), fields(memory_id = %id))]
    async fn delete(&self, id: &MemoryId) -> Result<(), ApplicationError> {
        let id_str = id.to_string();

        // Delete embedding first
        sqlx::query("DELETE FROM memory_embeddings WHERE memory_id = $1")
            .bind(&id_str)
            .execute(&self.pool)
            .await
            .map_err(map_sqlx_error)?;

        // Delete memory
        sqlx::query("DELETE FROM memories WHERE id = $1")
            .bind(&id_str)
            .execute(&self.pool)
            .await
            .map_err(map_sqlx_error)?;

        debug!("Deleted memory");
        Ok(())
    }

    #[instrument(skip(self, embedding), fields(user_id = %user_id, limit = limit))]
    async fn search_similar(
        &self,
        user_id: &UserId,
        embedding: &[f32],
        limit: usize,
        min_similarity: f32,
    ) -> Result<Vec<SimilarMemory>, ApplicationError> {
        let sql = format!("{SELECT_MEMORY_INNER} WHERE m.user_id = $1");
        let rows: Vec<MemoryRow> = sqlx::query_as(&sql)
            .bind(user_id.to_string())
            .fetch_all(&self.pool)
            .await
            .map_err(map_sqlx_error)?;

        let memories: Vec<Memory> = rows.into_iter().map(MemoryRow::to_memory).collect();
        let query_embedding = embedding;

        // Calculate similarities and filter
        let mut similar: Vec<SimilarMemory> = memories
            .into_iter()
            .filter_map(|memory| {
                let emb = memory.embedding.clone()?;
                let similarity = cosine_similarity(query_embedding, &emb);
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

        similar.truncate(limit);

        debug!(found = similar.len(), "Found similar memories");
        Ok(similar)
    }

    #[instrument(skip(self, query))]
    async fn list(&self, query: &MemoryQuery) -> Result<Vec<Memory>, ApplicationError> {
        let mut sql = format!("{SELECT_MEMORY} WHERE 1=1");
        let mut binds: Vec<String> = Vec::new();

        if let Some(ref user_id) = query.user_id {
            binds.push(user_id.to_string());
            sql.push_str(&format!(" AND m.user_id = ${}", binds.len()));
        }

        if let Some(ref conversation_id) = query.conversation_id {
            binds.push(conversation_id.to_string());
            sql.push_str(&format!(" AND m.conversation_id = ${}", binds.len()));
        }

        if let Some(ref types) = query.memory_types {
            if !types.is_empty() {
                let placeholders: Vec<String> = types
                    .iter()
                    .enumerate()
                    .map(|_| {
                        binds.push(String::new()); // placeholder, filled below
                        format!("${}", binds.len())
                    })
                    .collect();
                // Fill in the actual values
                let start = binds.len() - types.len();
                for (i, t) in types.iter().enumerate() {
                    binds[start + i] = memory_type_to_str(*t).to_string();
                }
                sql.push_str(&format!(
                    " AND m.memory_type IN ({})",
                    placeholders.join(", ")
                ));
            }
        }

        if let Some(min_importance) = query.min_importance {
            binds.push(min_importance.to_string());
            sql.push_str(&format!(" AND m.importance >= ${}", binds.len()));
        }

        sql.push_str(" ORDER BY m.importance DESC, m.accessed_at DESC");

        if let Some(limit) = query.limit {
            binds.push(limit.to_string());
            sql.push_str(&format!(" LIMIT ${}", binds.len()));
        }

        let mut q = sqlx::query_as::<_, MemoryRow>(&sql);
        for b in &binds {
            q = q.bind(b);
        }

        let rows: Vec<MemoryRow> = q.fetch_all(&self.pool).await.map_err(map_sqlx_error)?;
        let memories: Vec<Memory> = rows.into_iter().map(MemoryRow::to_memory).collect();

        debug!(count = memories.len(), "Listed memories");
        Ok(memories)
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
        let rows: Vec<DecayRow> =
            sqlx::query_as("SELECT id, importance, accessed_at, access_count FROM memories")
                .fetch_all(&self.pool)
                .await
                .map_err(map_sqlx_error)?;

        let mut below_threshold = Vec::new();

        for row in rows {
            #[allow(clippy::cast_possible_truncation)]
            let importance = row.importance as f32;

            let accessed_at = DateTime::parse_from_rfc3339(&row.accessed_at)
                .map_or_else(|_| Utc::now(), |dt| dt.with_timezone(&Utc));

            #[allow(clippy::cast_precision_loss)]
            let days_since_access = Utc::now().signed_duration_since(accessed_at).num_days() as f32;

            // Exponential decay with access boost
            let decay_factor = (-decay_rate * days_since_access).exp();
            let mut new_importance = importance * decay_factor;

            // Add access boost
            #[allow(clippy::cast_precision_loss)]
            let access_boost = (row.access_count as f32 * 0.01).min(0.1);
            new_importance = (new_importance + access_boost).min(1.0);

            sqlx::query("UPDATE memories SET importance = $1 WHERE id = $2")
                .bind(f64::from(new_importance))
                .bind(&row.id)
                .execute(&self.pool)
                .await
                .map_err(map_sqlx_error)?;

            if new_importance < Memory::MIN_IMPORTANCE {
                if let Ok(memory_id) = MemoryId::parse(&row.id) {
                    below_threshold.push(memory_id);
                }
            }
        }

        debug!(decayed = below_threshold.len(), "Applied decay to memories");
        Ok(below_threshold)
    }

    #[instrument(skip(self), fields(threshold = threshold))]
    async fn cleanup_below_threshold(&self, threshold: f32) -> Result<usize, ApplicationError> {
        // Delete embeddings for memories below threshold first
        sqlx::query(
            "DELETE FROM memory_embeddings WHERE memory_id IN \
             (SELECT id FROM memories WHERE importance < $1)",
        )
        .bind(f64::from(threshold))
        .execute(&self.pool)
        .await
        .map_err(map_sqlx_error)?;

        // Delete memories below threshold
        let result = sqlx::query("DELETE FROM memories WHERE importance < $1")
            .bind(f64::from(threshold))
            .execute(&self.pool)
            .await
            .map_err(map_sqlx_error)?;

        let deleted = result.rows_affected() as usize;
        debug!(deleted = deleted, "Cleaned up low-importance memories");
        Ok(deleted)
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
        let user_id_str = user_id.to_string();

        let total_count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM memories WHERE user_id = $1")
                .bind(&user_id_str)
                .fetch_one(&self.pool)
                .await
                .map_err(map_sqlx_error)?;

        let mut by_type = Vec::new();
        for memory_type in MemoryType::all() {
            let count: i64 = sqlx::query_scalar(
                "SELECT COUNT(*) FROM memories WHERE user_id = $1 AND memory_type = $2",
            )
            .bind(&user_id_str)
            .bind(memory_type_to_str(*memory_type))
            .fetch_one(&self.pool)
            .await
            .map_err(map_sqlx_error)?;

            #[allow(clippy::cast_sign_loss)]
            by_type.push((*memory_type, count as usize));
        }

        let with_embeddings: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM memories m
             INNER JOIN memory_embeddings e ON m.id = e.memory_id
             WHERE m.user_id = $1",
        )
        .bind(&user_id_str)
        .fetch_one(&self.pool)
        .await
        .map_err(map_sqlx_error)?;

        let avg_importance: f64 = sqlx::query_scalar(
            "SELECT COALESCE(AVG(importance), 0.0) FROM memories WHERE user_id = $1",
        )
        .bind(&user_id_str)
        .fetch_one(&self.pool)
        .await
        .map_err(map_sqlx_error)?;

        #[allow(clippy::cast_sign_loss)]
        Ok(MemoryStats {
            total_count: total_count as usize,
            by_type,
            with_embeddings: with_embeddings as usize,
            avg_importance: avg_importance as f32,
        })
    }

    #[instrument(skip(self), fields(memory_id = %id))]
    async fn record_access(&self, id: &MemoryId) -> Result<(), ApplicationError> {
        sqlx::query(
            "UPDATE memories SET accessed_at = $1, access_count = access_count + 1 WHERE id = $2",
        )
        .bind(Utc::now().to_rfc3339())
        .bind(id.to_string())
        .execute(&self.pool)
        .await
        .map_err(map_sqlx_error)?;

        debug!("Recorded memory access");
        Ok(())
    }
}

/// Convert `MemoryType` to string for storage
const fn memory_type_to_str(memory_type: MemoryType) -> &'static str {
    match memory_type {
        MemoryType::Fact => "fact",
        MemoryType::Preference => "preference",
        MemoryType::ToolResult => "tool_result",
        MemoryType::Correction => "correction",
        MemoryType::Context => "context",
    }
}

/// Convert string to `MemoryType`
fn str_to_memory_type(s: &str) -> MemoryType {
    match s {
        "fact" => MemoryType::Fact,
        "preference" => MemoryType::Preference,
        "tool_result" => MemoryType::ToolResult,
        "correction" => MemoryType::Correction,
        _ => MemoryType::Context,
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
    use crate::persistence::async_connection::AsyncDatabase;

    async fn setup() -> (AsyncDatabase, SqliteMemoryStore) {
        let db = AsyncDatabase::in_memory().await.unwrap();
        db.migrate().await.unwrap();
        let store = SqliteMemoryStore::new(db.pool().clone());
        (db, store)
    }

    /// Insert a user profile row so foreign key constraints are satisfied.
    async fn ensure_user(db: &AsyncDatabase, user_id: &UserId) {
        let now = chrono::Utc::now().to_rfc3339();
        sqlx::query(
            "INSERT OR IGNORE INTO user_profiles (user_id, timezone, created_at, updated_at) \
             VALUES ($1, 'UTC', $2, $3)",
        )
        .bind(user_id.to_string())
        .bind(&now)
        .bind(&now)
        .execute(db.pool())
        .await
        .unwrap();
    }

    fn make_memory(user_id: UserId) -> Memory {
        Memory::new(
            user_id,
            "Test content".to_string(),
            "Test summary".to_string(),
            MemoryType::Fact,
        )
        .with_importance(0.8)
        .with_tags(vec!["test".to_string()])
    }

    #[tokio::test]
    async fn save_and_get() {
        let (db, store) = setup().await;
        let user_id = UserId::new();
        ensure_user(&db, &user_id).await;
        let memory = make_memory(user_id);
        let id = memory.id;

        store.save(&memory).await.unwrap();
        let found = store.get(&id).await.unwrap();
        assert!(found.is_some());
        let found = found.unwrap();
        assert_eq!(found.content, "Test content");
        assert_eq!(found.summary, "Test summary");
    }

    #[tokio::test]
    async fn save_with_embedding() {
        let (db, store) = setup().await;
        let user_id = UserId::new();
        ensure_user(&db, &user_id).await;
        let memory = make_memory(user_id).with_embedding(vec![0.1, 0.2, 0.3]);
        let id = memory.id;

        store.save(&memory).await.unwrap();
        let found = store.get(&id).await.unwrap().unwrap();
        assert!(found.embedding.is_some());
        let emb = found.embedding.unwrap();
        assert_eq!(emb.len(), 3);
        assert!((emb[0] - 0.1).abs() < 0.001);
    }

    #[tokio::test]
    async fn update_memory() {
        let (db, store) = setup().await;
        let user_id = UserId::new();
        ensure_user(&db, &user_id).await;
        let mut memory = make_memory(user_id);
        store.save(&memory).await.unwrap();

        memory = memory.with_importance(0.9);
        store.update(&memory).await.unwrap();

        let found = store.get(&memory.id).await.unwrap().unwrap();
        assert!((found.importance - 0.9).abs() < 0.01);
    }

    #[tokio::test]
    async fn delete_memory() {
        let (db, store) = setup().await;
        let user_id = UserId::new();
        ensure_user(&db, &user_id).await;
        let memory = make_memory(user_id);
        let id = memory.id;

        store.save(&memory).await.unwrap();
        store.delete(&id).await.unwrap();
        assert!(store.get(&id).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn list_memories() {
        let (db, store) = setup().await;
        let user_id = UserId::new();
        ensure_user(&db, &user_id).await;

        for _ in 0..3 {
            store.save(&make_memory(user_id)).await.unwrap();
        }

        let query = MemoryQuery::new().for_user(user_id);
        let results = store.list(&query).await.unwrap();
        assert_eq!(results.len(), 3);
    }

    #[tokio::test]
    async fn list_by_type() {
        let (db, store) = setup().await;
        let user_id = UserId::new();
        ensure_user(&db, &user_id).await;

        store.save(&make_memory(user_id)).await.unwrap();
        let pref = Memory::new(
            user_id,
            "Preference".to_string(),
            "Pref summary".to_string(),
            MemoryType::Preference,
        );
        store.save(&pref).await.unwrap();

        let facts = store
            .list_by_type(&user_id, MemoryType::Fact, 10)
            .await
            .unwrap();
        assert_eq!(facts.len(), 1);
    }

    #[tokio::test]
    async fn search_similar_memories() {
        let (db, store) = setup().await;
        let user_id = UserId::new();
        ensure_user(&db, &user_id).await;

        let m1 = make_memory(user_id).with_embedding(vec![1.0, 0.0, 0.0]);
        let m2 = make_memory(user_id).with_embedding(vec![0.9, 0.1, 0.0]);
        let m3 = make_memory(user_id).with_embedding(vec![0.0, 1.0, 0.0]);
        store.save(&m1).await.unwrap();
        store.save(&m2).await.unwrap();
        store.save(&m3).await.unwrap();

        let query_emb = vec![1.0, 0.0, 0.0];
        let results = store
            .search_similar(&user_id, &query_emb, 10, 0.5)
            .await
            .unwrap();

        // m1 (identical) and m2 (very similar) should match; m3 (orthogonal) should not
        assert!(results.len() >= 2);
    }

    #[tokio::test]
    async fn stats() {
        let (db, store) = setup().await;
        let user_id = UserId::new();
        ensure_user(&db, &user_id).await;

        store.save(&make_memory(user_id)).await.unwrap();
        store.save(&make_memory(user_id)).await.unwrap();

        let stats = store.stats(&user_id).await.unwrap();
        assert_eq!(stats.total_count, 2);
    }

    #[tokio::test]
    async fn record_access() {
        let (db, store) = setup().await;
        let user_id = UserId::new();
        ensure_user(&db, &user_id).await;
        let memory = make_memory(user_id);
        let id = memory.id;

        store.save(&memory).await.unwrap();
        store.record_access(&id).await.unwrap();

        let found = store.get(&id).await.unwrap().unwrap();
        assert_eq!(found.access_count, 1);
    }

    #[tokio::test]
    async fn cleanup_below_threshold() {
        let (db, store) = setup().await;
        let user_id = UserId::new();
        ensure_user(&db, &user_id).await;

        let low = make_memory(user_id).with_importance(0.01);
        let high = make_memory(user_id).with_importance(0.9);
        store.save(&low).await.unwrap();
        store.save(&high).await.unwrap();

        let deleted = store.cleanup_below_threshold(0.1).await.unwrap();
        assert_eq!(deleted, 1);
    }

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

    #[test]
    fn sqlite_memory_store_is_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<SqliteMemoryStore>();
    }
}
