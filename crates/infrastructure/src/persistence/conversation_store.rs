//! SQLite conversation store implementation
//!
//! Implements the ConversationStore port using SQLite.

use std::sync::Arc;

use application::{error::ApplicationError, ports::ConversationStore};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use domain::{
    entities::{ChatMessage, Conversation, MessageMetadata, MessageRole},
    value_objects::ConversationId,
};
use rusqlite::{Row, params};
use tokio::task;
use tracing::{debug, instrument};
use uuid::Uuid;

use super::connection::ConnectionPool;

/// SQLite-based conversation store
#[derive(Debug, Clone)]
pub struct SqliteConversationStore {
    pool: Arc<ConnectionPool>,
}

impl SqliteConversationStore {
    /// Create a new SQLite conversation store
    #[must_use]
    pub const fn new(pool: Arc<ConnectionPool>) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl ConversationStore for SqliteConversationStore {
    #[instrument(skip(self, conversation), fields(conversation_id = %conversation.id))]
    async fn save(&self, conversation: &Conversation) -> Result<(), ApplicationError> {
        let pool = Arc::clone(&self.pool);
        let conversation = conversation.clone();

        task::spawn_blocking(move || {
            let conn = pool
                .get()
                .map_err(|e| ApplicationError::Internal(e.to_string()))?;

            conn.execute(
                "INSERT INTO conversations (id, created_at, updated_at, title, system_prompt)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params![
                    conversation.id.to_string(),
                    conversation.created_at.to_rfc3339(),
                    conversation.updated_at.to_rfc3339(),
                    conversation.title,
                    conversation.system_prompt,
                ],
            )
            .map_err(|e| ApplicationError::Internal(e.to_string()))?;

            // Insert all messages
            for message in &conversation.messages {
                insert_message(&conn, &conversation.id, message)?;
            }

            debug!("Saved conversation");
            Ok(())
        })
        .await
        .map_err(|e| ApplicationError::Internal(e.to_string()))?
    }

    #[instrument(skip(self), fields(conversation_id = %id))]
    async fn get(&self, id: &ConversationId) -> Result<Option<Conversation>, ApplicationError> {
        let pool = Arc::clone(&self.pool);
        let id_str = id.to_string();

        task::spawn_blocking(move || {
            let conn = pool
                .get()
                .map_err(|e| ApplicationError::Internal(e.to_string()))?;

            let conversation = conn
                .query_row(
                    "SELECT id, created_at, updated_at, title, system_prompt
                     FROM conversations WHERE id = ?1",
                    [&id_str],
                    row_to_conversation,
                )
                .optional()
                .map_err(|e| ApplicationError::Internal(e.to_string()))?;

            if let Some(mut conv) = conversation {
                // Load messages
                let mut stmt = conn
                    .prepare(
                        "SELECT id, role, content, created_at, tokens, model
                         FROM messages WHERE conversation_id = ?1 ORDER BY created_at ASC",
                    )
                    .map_err(|e| ApplicationError::Internal(e.to_string()))?;

                let messages = stmt
                    .query_map([&id_str], row_to_message)
                    .map_err(|e| ApplicationError::Internal(e.to_string()))?
                    .filter_map(Result::ok)
                    .collect();

                conv.messages = messages;
                Ok(Some(conv))
            } else {
                Ok(None)
            }
        })
        .await
        .map_err(|e| ApplicationError::Internal(e.to_string()))?
    }

    #[instrument(skip(self, conversation), fields(conversation_id = %conversation.id))]
    async fn update(&self, conversation: &Conversation) -> Result<(), ApplicationError> {
        let pool = Arc::clone(&self.pool);
        let conversation = conversation.clone();

        task::spawn_blocking(move || {
            let conn = pool
                .get()
                .map_err(|e| ApplicationError::Internal(e.to_string()))?;

            conn.execute(
                "UPDATE conversations SET updated_at = ?1, title = ?2, system_prompt = ?3
                 WHERE id = ?4",
                params![
                    conversation.updated_at.to_rfc3339(),
                    conversation.title,
                    conversation.system_prompt,
                    conversation.id.to_string(),
                ],
            )
            .map_err(|e| ApplicationError::Internal(e.to_string()))?;

            debug!("Updated conversation");
            Ok(())
        })
        .await
        .map_err(|e| ApplicationError::Internal(e.to_string()))?
    }

    #[instrument(skip(self), fields(conversation_id = %id))]
    async fn delete(&self, id: &ConversationId) -> Result<(), ApplicationError> {
        let pool = Arc::clone(&self.pool);
        let id_str = id.to_string();

        task::spawn_blocking(move || {
            let conn = pool
                .get()
                .map_err(|e| ApplicationError::Internal(e.to_string()))?;

            conn.execute("DELETE FROM conversations WHERE id = ?1", [&id_str])
                .map_err(|e| ApplicationError::Internal(e.to_string()))?;

            debug!("Deleted conversation");
            Ok(())
        })
        .await
        .map_err(|e| ApplicationError::Internal(e.to_string()))?
    }

    #[instrument(skip(self, message), fields(conversation_id = %conversation_id))]
    async fn add_message(
        &self,
        conversation_id: &ConversationId,
        message: &ChatMessage,
    ) -> Result<(), ApplicationError> {
        let pool = Arc::clone(&self.pool);
        let conv_id = *conversation_id;
        let message = message.clone();

        task::spawn_blocking(move || {
            let conn = pool
                .get()
                .map_err(|e| ApplicationError::Internal(e.to_string()))?;

            insert_message(&conn, &conv_id, &message)?;

            // Update conversation updated_at
            conn.execute(
                "UPDATE conversations SET updated_at = ?1 WHERE id = ?2",
                params![Utc::now().to_rfc3339(), conv_id.to_string()],
            )
            .map_err(|e| ApplicationError::Internal(e.to_string()))?;

            debug!("Added message to conversation");
            Ok(())
        })
        .await
        .map_err(|e| ApplicationError::Internal(e.to_string()))?
    }

    #[instrument(skip(self))]
    async fn list_recent(&self, limit: usize) -> Result<Vec<Conversation>, ApplicationError> {
        let pool = Arc::clone(&self.pool);

        task::spawn_blocking(move || {
            let conn = pool
                .get()
                .map_err(|e| ApplicationError::Internal(e.to_string()))?;

            let mut stmt = conn
                .prepare(
                    "SELECT id, created_at, updated_at, title, system_prompt
                     FROM conversations ORDER BY updated_at DESC LIMIT ?1",
                )
                .map_err(|e| ApplicationError::Internal(e.to_string()))?;

            let conversations: Vec<Conversation> = stmt
                .query_map([limit], row_to_conversation)
                .map_err(|e| ApplicationError::Internal(e.to_string()))?
                .filter_map(Result::ok)
                .collect();

            Ok(conversations)
        })
        .await
        .map_err(|e| ApplicationError::Internal(e.to_string()))?
    }

    #[instrument(skip(self))]
    async fn search(
        &self,
        query: &str,
        limit: usize,
    ) -> Result<Vec<Conversation>, ApplicationError> {
        let pool = Arc::clone(&self.pool);
        let query = format!("%{query}%");

        task::spawn_blocking(move || {
            let conn = pool
                .get()
                .map_err(|e| ApplicationError::Internal(e.to_string()))?;

            let mut stmt = conn
                .prepare(
                    "SELECT DISTINCT c.id, c.created_at, c.updated_at, c.title, c.system_prompt
                     FROM conversations c
                     LEFT JOIN messages m ON c.id = m.conversation_id
                     WHERE c.title LIKE ?1 OR m.content LIKE ?1
                     ORDER BY c.updated_at DESC LIMIT ?2",
                )
                .map_err(|e| ApplicationError::Internal(e.to_string()))?;

            let conversations: Vec<Conversation> = stmt
                .query_map(params![query, limit], row_to_conversation)
                .map_err(|e| ApplicationError::Internal(e.to_string()))?
                .filter_map(Result::ok)
                .collect();

            Ok(conversations)
        })
        .await
        .map_err(|e| ApplicationError::Internal(e.to_string()))?
    }

    #[instrument(skip(self))]
    async fn cleanup_older_than(
        &self,
        cutoff: chrono::DateTime<chrono::Utc>,
    ) -> Result<usize, ApplicationError> {
        let pool = Arc::clone(&self.pool);
        let cutoff_str = cutoff.to_rfc3339();

        task::spawn_blocking(move || {
            let conn = pool
                .get()
                .map_err(|e| ApplicationError::Internal(e.to_string()))?;

            // Messages are deleted via CASCADE constraint
            let deleted = conn
                .execute(
                    "DELETE FROM conversations WHERE updated_at < ?1",
                    [&cutoff_str],
                )
                .map_err(|e| ApplicationError::Internal(e.to_string()))?;

            debug!(deleted_count = deleted, "Cleaned up old conversations");
            Ok(deleted)
        })
        .await
        .map_err(|e| ApplicationError::Internal(e.to_string()))?
    }
}

fn insert_message(
    conn: &rusqlite::Connection,
    conversation_id: &ConversationId,
    message: &ChatMessage,
) -> Result<(), ApplicationError> {
    let role_str = match message.role {
        MessageRole::User => "user",
        MessageRole::Assistant => "assistant",
        MessageRole::System => "system",
    };

    let (tokens, model) = message
        .metadata
        .as_ref()
        .map_or((None, None), |m| (m.tokens, m.model.clone()));

    conn.execute(
        "INSERT INTO messages (id, conversation_id, role, content, created_at, tokens, model)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![
            message.id.to_string(),
            conversation_id.to_string(),
            role_str,
            message.content,
            message.created_at.to_rfc3339(),
            tokens,
            model,
        ],
    )
    .map_err(|e| ApplicationError::Internal(e.to_string()))?;

    Ok(())
}

fn row_to_conversation(row: &Row<'_>) -> rusqlite::Result<Conversation> {
    let id_str: String = row.get(0)?;
    let created_at_str: String = row.get(1)?;
    let updated_at_str: String = row.get(2)?;
    let title: Option<String> = row.get(3)?;
    let system_prompt: Option<String> = row.get(4)?;

    let id = ConversationId::from(Uuid::parse_str(&id_str).unwrap_or_else(|_| Uuid::new_v4()));
    let created_at = DateTime::parse_from_rfc3339(&created_at_str)
        .map_or_else(|_| Utc::now(), |dt| dt.with_timezone(&Utc));
    let updated_at = DateTime::parse_from_rfc3339(&updated_at_str)
        .map_or_else(|_| Utc::now(), |dt| dt.with_timezone(&Utc));

    Ok(Conversation {
        id,
        messages: Vec::new(),
        created_at,
        updated_at,
        title,
        system_prompt,
    })
}

fn row_to_message(row: &Row<'_>) -> rusqlite::Result<ChatMessage> {
    let id_str: String = row.get(0)?;
    let role_str: String = row.get(1)?;
    let content: String = row.get(2)?;
    let created_at_str: String = row.get(3)?;
    let tokens: Option<u32> = row.get(4)?;
    let model: Option<String> = row.get(5)?;

    let id = Uuid::parse_str(&id_str).unwrap_or_else(|_| Uuid::new_v4());
    let role = match role_str.as_str() {
        "user" => MessageRole::User,
        "assistant" => MessageRole::Assistant,
        _ => MessageRole::System,
    };
    let created_at = DateTime::parse_from_rfc3339(&created_at_str)
        .map_or_else(|_| Utc::now(), |dt| dt.with_timezone(&Utc));

    let metadata = if tokens.is_some() || model.is_some() {
        Some(MessageMetadata {
            model,
            tokens,
            latency_ms: None,
        })
    } else {
        None
    };

    Ok(ChatMessage {
        id,
        role,
        content,
        created_at,
        metadata,
    })
}

// Extension trait for optional query results
trait OptionalExt<T> {
    fn optional(self) -> Result<Option<T>, rusqlite::Error>;
}

impl<T> OptionalExt<T> for Result<T, rusqlite::Error> {
    fn optional(self) -> Result<Option<T>, rusqlite::Error> {
        match self {
            Ok(val) => Ok(Some(val)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{config::DatabaseConfig, persistence::connection::create_pool};

    fn create_test_store() -> SqliteConversationStore {
        let config = DatabaseConfig {
            path: ":memory:".to_string(),
            max_connections: 1,
            run_migrations: true,
        };
        let pool = create_pool(&config).unwrap();
        SqliteConversationStore::new(Arc::new(pool))
    }

    #[tokio::test]
    async fn save_and_get_conversation() {
        let store = create_test_store();
        let conversation = Conversation::with_system_prompt("Test prompt");
        let id = conversation.id;

        store.save(&conversation).await.unwrap();

        let retrieved = store.get(&id).await.unwrap();
        assert!(retrieved.is_some());
        let retrieved = retrieved.unwrap();
        assert_eq!(retrieved.id, id);
        assert_eq!(retrieved.system_prompt, Some("Test prompt".to_string()));
    }

    #[tokio::test]
    async fn save_conversation_with_messages() {
        let store = create_test_store();
        let mut conversation = Conversation::new();
        conversation.add_user_message("Hello");
        conversation.add_assistant_message("Hi there!");
        let id = conversation.id;

        store.save(&conversation).await.unwrap();

        let retrieved = store.get(&id).await.unwrap().unwrap();
        assert_eq!(retrieved.messages.len(), 2);
        assert_eq!(retrieved.messages[0].role, MessageRole::User);
        assert_eq!(retrieved.messages[1].role, MessageRole::Assistant);
    }

    #[tokio::test]
    async fn get_nonexistent_conversation() {
        let store = create_test_store();
        let id = ConversationId::new();

        let result = store.get(&id).await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn delete_conversation() {
        let store = create_test_store();
        let conversation = Conversation::new();
        let id = conversation.id;

        store.save(&conversation).await.unwrap();
        store.delete(&id).await.unwrap();

        let result = store.get(&id).await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn add_message_to_conversation() {
        let store = create_test_store();
        let conversation = Conversation::new();
        let id = conversation.id;

        store.save(&conversation).await.unwrap();

        let message = ChatMessage::user("New message");
        store.add_message(&id, &message).await.unwrap();

        let retrieved = store.get(&id).await.unwrap().unwrap();
        assert_eq!(retrieved.messages.len(), 1);
        assert_eq!(retrieved.messages[0].content, "New message");
    }

    #[tokio::test]
    async fn list_recent_conversations() {
        let store = create_test_store();

        for i in 0..5 {
            let mut conv = Conversation::new();
            conv.title = Some(format!("Conversation {i}"));
            store.save(&conv).await.unwrap();
        }

        let recent = store.list_recent(3).await.unwrap();
        assert_eq!(recent.len(), 3);
    }

    #[tokio::test]
    async fn search_conversations() {
        let store = create_test_store();

        let mut conv = Conversation::new();
        conv.title = Some("Important meeting".to_string());
        store.save(&conv).await.unwrap();

        let results = store.search("meeting", 10).await.unwrap();
        assert!(!results.is_empty());
    }
}
