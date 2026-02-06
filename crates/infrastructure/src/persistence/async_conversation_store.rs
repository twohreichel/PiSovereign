//! Async conversation store using sqlx
//!
//! Provides async conversation persistence using sqlx with SQLite.
//! Implements the same interface as the blocking version but with
//! true async operations.

use application::{error::ApplicationError, ports::ConversationStore};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use domain::{ChatMessage, Conversation, ConversationId, MessageMetadata, MessageRole};
use sqlx::SqlitePool;
use tracing::{debug, instrument};
use uuid::Uuid;

/// Async conversation store using sqlx
#[derive(Debug, Clone)]
pub struct AsyncConversationStore {
    pool: SqlitePool,
}

impl AsyncConversationStore {
    /// Create a new async conversation store
    #[must_use]
    pub const fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    /// Parse a role string into `MessageRole`
    fn parse_role(role: &str) -> Result<MessageRole, ApplicationError> {
        match role {
            "user" => Ok(MessageRole::User),
            "assistant" => Ok(MessageRole::Assistant),
            "system" => Ok(MessageRole::System),
            other => Err(ApplicationError::Internal(format!(
                "Invalid message role: {other}"
            ))),
        }
    }

    /// Convert `MessageRole` to string for storage
    const fn role_to_str(role: MessageRole) -> &'static str {
        match role {
            MessageRole::User => "user",
            MessageRole::Assistant => "assistant",
            MessageRole::System => "system",
        }
    }

    /// Parse a conversation ID from string
    fn parse_conversation_id(s: &str) -> Result<ConversationId, ApplicationError> {
        ConversationId::parse(s)
            .map_err(|e| ApplicationError::Internal(format!("Invalid conversation ID: {e}")))
    }

    /// Parse a UUID from string
    fn parse_uuid(s: &str) -> Result<Uuid, ApplicationError> {
        Uuid::parse_str(s).map_err(|e| ApplicationError::Internal(format!("Invalid UUID: {e}")))
    }
}

#[async_trait]
impl ConversationStore for AsyncConversationStore {
    #[instrument(skip(self, conversation), fields(conv_id = %conversation.id))]
    async fn save(&self, conversation: &Conversation) -> Result<(), ApplicationError> {
        let mut tx = self.pool.begin().await.map_err(map_sqlx_error)?;

        // Upsert conversation
        sqlx::query(
            r"
            INSERT INTO conversations (id, title, system_prompt, created_at, updated_at)
            VALUES ($1, $2, $3, $4, $5)
            ON CONFLICT(id) DO UPDATE SET
                title = excluded.title,
                system_prompt = excluded.system_prompt,
                updated_at = excluded.updated_at
            ",
        )
        .bind(conversation.id.to_string())
        .bind(&conversation.title)
        .bind(&conversation.system_prompt)
        .bind(conversation.created_at.to_rfc3339())
        .bind(conversation.updated_at.to_rfc3339())
        .execute(&mut *tx)
        .await
        .map_err(map_sqlx_error)?;

        // Delete existing messages (will be re-inserted)
        sqlx::query("DELETE FROM messages WHERE conversation_id = $1")
            .bind(conversation.id.to_string())
            .execute(&mut *tx)
            .await
            .map_err(map_sqlx_error)?;

        // Insert all messages
        for message in &conversation.messages {
            let metadata_json = message
                .metadata
                .as_ref()
                .and_then(|m| serde_json::to_string(m).ok());

            sqlx::query(
                r"
                INSERT INTO messages (id, conversation_id, role, content, created_at, metadata)
                VALUES ($1, $2, $3, $4, $5, $6)
                ",
            )
            .bind(message.id.to_string())
            .bind(conversation.id.to_string())
            .bind(Self::role_to_str(message.role))
            .bind(&message.content)
            .bind(message.created_at.to_rfc3339())
            .bind(metadata_json)
            .execute(&mut *tx)
            .await
            .map_err(map_sqlx_error)?;
        }

        tx.commit().await.map_err(map_sqlx_error)?;
        debug!("Conversation saved");
        Ok(())
    }

    #[instrument(skip(self), fields(conv_id = %id))]
    async fn get(&self, id: &ConversationId) -> Result<Option<Conversation>, ApplicationError> {
        // Fetch conversation
        let conv_row: Option<ConversationRow> = sqlx::query_as(
            r"
            SELECT id, title, system_prompt, created_at, updated_at
            FROM conversations WHERE id = $1
            ",
        )
        .bind(id.to_string())
        .fetch_optional(&self.pool)
        .await
        .map_err(map_sqlx_error)?;

        let Some(row) = conv_row else {
            debug!("Conversation not found");
            return Ok(None);
        };

        // Fetch messages
        let message_rows: Vec<MessageRow> = sqlx::query_as(
            r"
            SELECT id, role, content, created_at, metadata
            FROM messages WHERE conversation_id = $1
            ORDER BY created_at ASC
            ",
        )
        .bind(id.to_string())
        .fetch_all(&self.pool)
        .await
        .map_err(map_sqlx_error)?;

        // Convert to domain types
        let mut messages = Vec::with_capacity(message_rows.len());
        for msg_row in message_rows {
            let metadata: Option<MessageMetadata> = msg_row
                .metadata
                .as_deref()
                .and_then(|s| serde_json::from_str(s).ok());

            messages.push(ChatMessage {
                id: Self::parse_uuid(&msg_row.id)?,
                role: Self::parse_role(&msg_row.role)?,
                content: msg_row.content,
                created_at: parse_datetime(&msg_row.created_at)?,
                metadata,
            });
        }

        let message_count = messages.len();
        let conversation = Conversation {
            id: Self::parse_conversation_id(&row.id)?,
            title: row.title,
            system_prompt: row.system_prompt,
            messages,
            created_at: parse_datetime(&row.created_at)?,
            updated_at: parse_datetime(&row.updated_at)?,
            // All loaded messages are already persisted
            persisted_message_count: message_count,
        };

        debug!("Conversation loaded");
        Ok(Some(conversation))
    }

    #[instrument(skip(self, conversation), fields(conv_id = %conversation.id))]
    async fn update(&self, conversation: &Conversation) -> Result<(), ApplicationError> {
        // For SQLite, update is the same as save (upsert)
        self.save(conversation).await
    }

    #[instrument(skip(self), fields(conv_id = %id))]
    async fn delete(&self, id: &ConversationId) -> Result<(), ApplicationError> {
        sqlx::query("DELETE FROM conversations WHERE id = $1")
            .bind(id.to_string())
            .execute(&self.pool)
            .await
            .map_err(map_sqlx_error)?;

        debug!("Conversation deleted");
        Ok(())
    }

    #[instrument(skip(self, message), fields(conv_id = %conversation_id))]
    async fn add_message(
        &self,
        conversation_id: &ConversationId,
        message: &ChatMessage,
    ) -> Result<(), ApplicationError> {
        let metadata_json = message
            .metadata
            .as_ref()
            .and_then(|m| serde_json::to_string(m).ok());

        sqlx::query(
            r"
            INSERT INTO messages (id, conversation_id, role, content, created_at, metadata)
            VALUES ($1, $2, $3, $4, $5, $6)
            ",
        )
        .bind(message.id.to_string())
        .bind(conversation_id.to_string())
        .bind(Self::role_to_str(message.role))
        .bind(&message.content)
        .bind(message.created_at.to_rfc3339())
        .bind(metadata_json)
        .execute(&self.pool)
        .await
        .map_err(map_sqlx_error)?;

        // Update conversation's updated_at
        sqlx::query("UPDATE conversations SET updated_at = $1 WHERE id = $2")
            .bind(Utc::now().to_rfc3339())
            .bind(conversation_id.to_string())
            .execute(&self.pool)
            .await
            .map_err(map_sqlx_error)?;

        debug!("Message added");
        Ok(())
    }

    #[instrument(skip(self, messages), fields(conv_id = %conversation_id, count = messages.len()))]
    async fn add_messages(
        &self,
        conversation_id: &ConversationId,
        messages: &[ChatMessage],
    ) -> Result<usize, ApplicationError> {
        if messages.is_empty() {
            return Ok(0);
        }

        let mut tx = self.pool.begin().await.map_err(map_sqlx_error)?;

        for message in messages {
            let metadata_json = message
                .metadata
                .as_ref()
                .and_then(|m| serde_json::to_string(m).ok());

            sqlx::query(
                r"
                INSERT INTO messages (id, conversation_id, role, content, created_at, metadata)
                VALUES ($1, $2, $3, $4, $5, $6)
                ",
            )
            .bind(message.id.to_string())
            .bind(conversation_id.to_string())
            .bind(Self::role_to_str(message.role))
            .bind(&message.content)
            .bind(message.created_at.to_rfc3339())
            .bind(metadata_json)
            .execute(&mut *tx)
            .await
            .map_err(map_sqlx_error)?;
        }

        // Update conversation's updated_at once at the end
        sqlx::query("UPDATE conversations SET updated_at = $1 WHERE id = $2")
            .bind(Utc::now().to_rfc3339())
            .bind(conversation_id.to_string())
            .execute(&mut *tx)
            .await
            .map_err(map_sqlx_error)?;

        tx.commit().await.map_err(map_sqlx_error)?;

        let count = messages.len();
        debug!(count, "Messages added in batch");
        Ok(count)
    }

    #[instrument(skip(self))]
    async fn list_recent(&self, limit: usize) -> Result<Vec<Conversation>, ApplicationError> {
        let conv_rows: Vec<ConversationRow> = sqlx::query_as(
            r"
            SELECT id, title, system_prompt, created_at, updated_at
            FROM conversations
            ORDER BY updated_at DESC
            LIMIT $1
            ",
        )
        .bind(i64::try_from(limit).unwrap_or(i64::MAX))
        .fetch_all(&self.pool)
        .await
        .map_err(map_sqlx_error)?;

        let mut conversations = Vec::with_capacity(conv_rows.len());
        for row in conv_rows {
            let conv_id = Self::parse_conversation_id(&row.id)?;

            // Fetch messages for each conversation
            let message_rows: Vec<MessageRow> = sqlx::query_as(
                r"
                SELECT id, role, content, created_at, metadata
                FROM messages WHERE conversation_id = $1
                ORDER BY created_at ASC
                ",
            )
            .bind(conv_id.to_string())
            .fetch_all(&self.pool)
            .await
            .map_err(map_sqlx_error)?;

            let mut messages = Vec::with_capacity(message_rows.len());
            for msg_row in message_rows {
                let metadata: Option<MessageMetadata> = msg_row
                    .metadata
                    .as_deref()
                    .and_then(|s| serde_json::from_str(s).ok());

                messages.push(ChatMessage {
                    id: Self::parse_uuid(&msg_row.id)?,
                    role: Self::parse_role(&msg_row.role)?,
                    content: msg_row.content,
                    created_at: parse_datetime(&msg_row.created_at)?,
                    metadata,
                });
            }

            let message_count = messages.len();
            conversations.push(Conversation {
                id: conv_id,
                title: row.title,
                system_prompt: row.system_prompt,
                messages,
                created_at: parse_datetime(&row.created_at)?,
                updated_at: parse_datetime(&row.updated_at)?,
                persisted_message_count: message_count,
            });
        }

        debug!(count = conversations.len(), "Listed recent conversations");
        Ok(conversations)
    }

    #[instrument(skip(self))]
    async fn search(
        &self,
        query: &str,
        limit: usize,
    ) -> Result<Vec<Conversation>, ApplicationError> {
        let search_pattern = format!("%{query}%");

        let conv_rows: Vec<ConversationRow> = sqlx::query_as(
            r"
            SELECT DISTINCT c.id, c.title, c.system_prompt, c.created_at, c.updated_at
            FROM conversations c
            LEFT JOIN messages m ON c.id = m.conversation_id
            WHERE c.title LIKE $1 OR m.content LIKE $1
            ORDER BY c.updated_at DESC
            LIMIT $2
            ",
        )
        .bind(&search_pattern)
        .bind(i64::try_from(limit).unwrap_or(i64::MAX))
        .fetch_all(&self.pool)
        .await
        .map_err(map_sqlx_error)?;

        let mut conversations = Vec::with_capacity(conv_rows.len());
        for row in conv_rows {
            let conv_id = Self::parse_conversation_id(&row.id)?;

            // Fetch messages
            let message_rows: Vec<MessageRow> = sqlx::query_as(
                r"
                SELECT id, role, content, created_at, metadata
                FROM messages WHERE conversation_id = $1
                ORDER BY created_at ASC
                ",
            )
            .bind(conv_id.to_string())
            .fetch_all(&self.pool)
            .await
            .map_err(map_sqlx_error)?;

            let mut messages = Vec::with_capacity(message_rows.len());
            for msg_row in message_rows {
                let metadata: Option<MessageMetadata> = msg_row
                    .metadata
                    .as_deref()
                    .and_then(|s| serde_json::from_str(s).ok());

                messages.push(ChatMessage {
                    id: Self::parse_uuid(&msg_row.id)?,
                    role: Self::parse_role(&msg_row.role)?,
                    content: msg_row.content,
                    created_at: parse_datetime(&msg_row.created_at)?,
                    metadata,
                });
            }

            let message_count = messages.len();
            conversations.push(Conversation {
                id: conv_id,
                title: row.title,
                system_prompt: row.system_prompt,
                messages,
                created_at: parse_datetime(&row.created_at)?,
                updated_at: parse_datetime(&row.updated_at)?,
                persisted_message_count: message_count,
            });
        }

        debug!(
            query = %query,
            count = conversations.len(),
            "Search completed"
        );
        Ok(conversations)
    }

    #[instrument(skip(self))]
    async fn cleanup_older_than(&self, cutoff: DateTime<Utc>) -> Result<usize, ApplicationError> {
        // Messages are deleted via CASCADE when conversation is deleted
        let result = sqlx::query("DELETE FROM conversations WHERE updated_at < $1")
            .bind(cutoff.to_rfc3339())
            .execute(&self.pool)
            .await
            .map_err(map_sqlx_error)?;

        let deleted = usize::try_from(result.rows_affected()).unwrap_or(usize::MAX);
        if deleted > 0 {
            debug!(deleted, cutoff = %cutoff, "Cleaned up old conversations");
        }
        Ok(deleted)
    }
}

/// Row type for conversation queries
#[derive(sqlx::FromRow)]
struct ConversationRow {
    id: String,
    title: Option<String>,
    system_prompt: Option<String>,
    created_at: String,
    updated_at: String,
}

/// Row type for message queries
#[derive(sqlx::FromRow)]
struct MessageRow {
    id: String,
    role: String,
    content: String,
    created_at: String,
    metadata: Option<String>,
}

/// Parse an RFC3339 datetime string
fn parse_datetime(s: &str) -> Result<DateTime<Utc>, ApplicationError> {
    DateTime::parse_from_rfc3339(s)
        .map(|dt| dt.with_timezone(&Utc))
        .map_err(|e| ApplicationError::Internal(format!("Invalid datetime: {e}")))
}

/// Map sqlx error to application error
fn map_sqlx_error(e: sqlx::Error) -> ApplicationError {
    match e {
        sqlx::Error::RowNotFound => {
            ApplicationError::NotFound("Database record not found".to_string())
        },
        sqlx::Error::Database(db_err) => {
            ApplicationError::Internal(format!("Database error: {db_err}"))
        },
        other => ApplicationError::Internal(format!("Database error: {other}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::persistence::async_connection::{AsyncDatabase, AsyncDatabaseConfig};

    async fn setup_test_db() -> (AsyncDatabase, AsyncConversationStore) {
        let db = AsyncDatabase::new(&AsyncDatabaseConfig::in_memory())
            .await
            .unwrap();
        db.migrate().await.unwrap();
        let store = AsyncConversationStore::new(db.pool().clone());
        (db, store)
    }

    #[tokio::test]
    async fn save_and_get_conversation() {
        let (_db, store) = setup_test_db().await;

        let conv = Conversation::new();
        store.save(&conv).await.unwrap();

        let loaded = store.get(&conv.id).await.unwrap();
        assert!(loaded.is_some());
        assert_eq!(loaded.unwrap().id, conv.id);
    }

    #[tokio::test]
    async fn get_nonexistent_returns_none() {
        let (_db, store) = setup_test_db().await;

        let result = store.get(&ConversationId::new()).await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn save_conversation_with_messages() {
        let (_db, store) = setup_test_db().await;

        let mut conv = Conversation::new();
        conv.add_user_message("Hello");
        conv.add_assistant_message("Hi there!");

        store.save(&conv).await.unwrap();

        let loaded = store.get(&conv.id).await.unwrap().unwrap();
        assert_eq!(loaded.messages.len(), 2);
        assert_eq!(loaded.messages[0].content, "Hello");
        assert_eq!(loaded.messages[1].content, "Hi there!");
    }

    #[tokio::test]
    async fn delete_conversation() {
        let (_db, store) = setup_test_db().await;

        let conv = Conversation::new();
        store.save(&conv).await.unwrap();
        store.delete(&conv.id).await.unwrap();

        let result = store.get(&conv.id).await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn list_recent_conversations() {
        let (_db, store) = setup_test_db().await;

        let conv1 = Conversation::new();
        let conv2 = Conversation::new();
        store.save(&conv1).await.unwrap();
        store.save(&conv2).await.unwrap();

        let recent = store.list_recent(10).await.unwrap();
        assert_eq!(recent.len(), 2);
    }

    #[tokio::test]
    async fn search_conversations() {
        let (_db, store) = setup_test_db().await;

        let mut conv = Conversation::new();
        conv.title = Some("Test Conversation".to_string());
        conv.add_user_message("Hello world");
        store.save(&conv).await.unwrap();

        let results = store.search("world", 50).await.unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, conv.id);
    }

    #[tokio::test]
    async fn add_message_to_conversation() {
        let (_db, store) = setup_test_db().await;

        let conv = Conversation::new();
        store.save(&conv).await.unwrap();

        let message = ChatMessage::user("Hello");
        store.add_message(&conv.id, &message).await.unwrap();

        let loaded = store.get(&conv.id).await.unwrap().unwrap();
        assert_eq!(loaded.messages.len(), 1);
        assert_eq!(loaded.messages[0].content, "Hello");
    }
}
