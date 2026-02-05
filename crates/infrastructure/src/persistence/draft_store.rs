//! SQLite draft store implementation
//!
//! Implements the `DraftStorePort` for persisting email drafts.

use std::sync::Arc;

use application::{error::ApplicationError, ports::DraftStorePort};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use domain::{DraftId, PersistedEmailDraft, UserId};
use rusqlite::{Row, params};
use tokio::task;
use tracing::{debug, instrument};
use uuid::Uuid;

use super::connection::ConnectionPool;

/// SQLite-based email draft store
#[derive(Debug, Clone)]
pub struct SqliteDraftStore {
    pool: Arc<ConnectionPool>,
}

impl SqliteDraftStore {
    /// Create a new SQLite draft store
    #[must_use]
    pub const fn new(pool: Arc<ConnectionPool>) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl DraftStorePort for SqliteDraftStore {
    #[instrument(skip(self, draft), fields(draft_id = %draft.id, user_id = %draft.user_id))]
    async fn save(&self, draft: &PersistedEmailDraft) -> Result<DraftId, ApplicationError> {
        let pool = Arc::clone(&self.pool);
        let draft = draft.clone();
        let draft_id = draft.id;

        task::spawn_blocking(move || {
            let conn = pool
                .get()
                .map_err(|e| ApplicationError::Internal(e.to_string()))?;

            // Serialize CC list as comma-separated string (or NULL if empty)
            let cc_str = if draft.cc.is_empty() {
                None
            } else {
                Some(draft.cc.join(","))
            };

            conn.execute(
                "INSERT INTO email_drafts (id, user_id, to_address, cc, subject, body, created_at, expires_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                params![
                    draft.id.to_string(),
                    draft.user_id.to_string(),
                    draft.to,
                    cc_str,
                    draft.subject,
                    draft.body,
                    draft.created_at.to_rfc3339(),
                    draft.expires_at.to_rfc3339(),
                ],
            )
            .map_err(|e| ApplicationError::Internal(e.to_string()))?;

            debug!("Saved email draft");
            Ok(draft_id)
        })
        .await
        .map_err(|e| ApplicationError::Internal(e.to_string()))?
    }

    #[instrument(skip(self), fields(draft_id = %id))]
    async fn get(&self, id: &DraftId) -> Result<Option<PersistedEmailDraft>, ApplicationError> {
        let pool = Arc::clone(&self.pool);
        let id_str = id.to_string();

        task::spawn_blocking(move || {
            let conn = pool
                .get()
                .map_err(|e| ApplicationError::Internal(e.to_string()))?;

            let draft = conn
                .query_row(
                    "SELECT id, user_id, to_address, cc, subject, body, created_at, expires_at
                     FROM email_drafts WHERE id = ?1",
                    [&id_str],
                    row_to_draft,
                )
                .optional()
                .map_err(|e| ApplicationError::Internal(e.to_string()))?;

            // Filter out expired drafts
            Ok(draft.filter(|d| !d.is_expired()))
        })
        .await
        .map_err(|e| ApplicationError::Internal(e.to_string()))?
    }

    #[instrument(skip(self), fields(draft_id = %id, user_id = %user_id))]
    async fn get_for_user(
        &self,
        id: &DraftId,
        user_id: &UserId,
    ) -> Result<Option<PersistedEmailDraft>, ApplicationError> {
        let pool = Arc::clone(&self.pool);
        let id_str = id.to_string();
        let user_id_str = user_id.to_string();

        task::spawn_blocking(move || {
            let conn = pool
                .get()
                .map_err(|e| ApplicationError::Internal(e.to_string()))?;

            let draft = conn
                .query_row(
                    "SELECT id, user_id, to_address, cc, subject, body, created_at, expires_at
                     FROM email_drafts WHERE id = ?1 AND user_id = ?2",
                    [&id_str, &user_id_str],
                    row_to_draft,
                )
                .optional()
                .map_err(|e| ApplicationError::Internal(e.to_string()))?;

            // Filter out expired drafts
            Ok(draft.filter(|d| !d.is_expired()))
        })
        .await
        .map_err(|e| ApplicationError::Internal(e.to_string()))?
    }

    #[instrument(skip(self), fields(draft_id = %id))]
    async fn delete(&self, id: &DraftId) -> Result<bool, ApplicationError> {
        let pool = Arc::clone(&self.pool);
        let id_str = id.to_string();

        task::spawn_blocking(move || {
            let conn = pool
                .get()
                .map_err(|e| ApplicationError::Internal(e.to_string()))?;

            let deleted = conn
                .execute("DELETE FROM email_drafts WHERE id = ?1", [&id_str])
                .map_err(|e| ApplicationError::Internal(e.to_string()))?;

            debug!(deleted = deleted > 0, "Deleted email draft");
            Ok(deleted > 0)
        })
        .await
        .map_err(|e| ApplicationError::Internal(e.to_string()))?
    }

    #[instrument(skip(self), fields(user_id = %user_id))]
    async fn list_for_user(
        &self,
        user_id: &UserId,
        limit: usize,
    ) -> Result<Vec<PersistedEmailDraft>, ApplicationError> {
        let pool = Arc::clone(&self.pool);
        let user_id_str = user_id.to_string();
        let now = Utc::now().to_rfc3339();

        task::spawn_blocking(move || {
            let conn = pool
                .get()
                .map_err(|e| ApplicationError::Internal(e.to_string()))?;

            let mut stmt = conn
                .prepare(
                    "SELECT id, user_id, to_address, cc, subject, body, created_at, expires_at
                     FROM email_drafts
                     WHERE user_id = ?1 AND expires_at > ?2
                     ORDER BY created_at DESC
                     LIMIT ?3",
                )
                .map_err(|e| ApplicationError::Internal(e.to_string()))?;

            let drafts: Vec<PersistedEmailDraft> = stmt
                .query_map(params![user_id_str, now, limit], row_to_draft)
                .map_err(|e| ApplicationError::Internal(e.to_string()))?
                .filter_map(Result::ok)
                .collect();

            debug!(count = drafts.len(), "Listed drafts for user");
            Ok(drafts)
        })
        .await
        .map_err(|e| ApplicationError::Internal(e.to_string()))?
    }

    #[instrument(skip(self))]
    async fn cleanup_expired(&self) -> Result<usize, ApplicationError> {
        let pool = Arc::clone(&self.pool);
        let now = Utc::now().to_rfc3339();

        task::spawn_blocking(move || {
            let conn = pool
                .get()
                .map_err(|e| ApplicationError::Internal(e.to_string()))?;

            let deleted = conn
                .execute("DELETE FROM email_drafts WHERE expires_at <= ?1", [&now])
                .map_err(|e| ApplicationError::Internal(e.to_string()))?;

            debug!(deleted_count = deleted, "Cleaned up expired drafts");
            Ok(deleted)
        })
        .await
        .map_err(|e| ApplicationError::Internal(e.to_string()))?
    }
}

/// Convert a database row to a `PersistedEmailDraft`
fn row_to_draft(row: &Row<'_>) -> rusqlite::Result<PersistedEmailDraft> {
    let id_str: String = row.get(0)?;
    let user_id_str: String = row.get(1)?;
    let to: String = row.get(2)?;
    let cc_str: Option<String> = row.get(3)?;
    let subject: String = row.get(4)?;
    let body: String = row.get(5)?;
    let created_at_str: String = row.get(6)?;
    let expires_at_str: String = row.get(7)?;

    let id = DraftId::parse(&id_str)
        .unwrap_or_else(|_| DraftId::from(Uuid::new_v4()));
    let user_id = UserId::parse(&user_id_str)
        .unwrap_or_else(|_| UserId::from(Uuid::new_v4()));

    // Parse CC from comma-separated string
    let cc = cc_str
        .map(|s| s.split(',').map(String::from).collect())
        .unwrap_or_default();

    let created_at = DateTime::parse_from_rfc3339(&created_at_str)
        .map_or_else(|_| Utc::now(), |dt| dt.with_timezone(&Utc));
    let expires_at = DateTime::parse_from_rfc3339(&expires_at_str)
        .map_or_else(|_| Utc::now(), |dt| dt.with_timezone(&Utc));

    Ok(PersistedEmailDraft {
        id,
        user_id,
        to,
        cc,
        subject,
        body,
        created_at,
        expires_at,
    })
}

/// Extension trait for optional query results
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
    use chrono::Duration;

    fn create_test_store() -> SqliteDraftStore {
        let config = DatabaseConfig {
            path: ":memory:".to_string(),
            max_connections: 1,
            run_migrations: true,
        };
        let pool = create_pool(&config).unwrap();
        SqliteDraftStore::new(Arc::new(pool))
    }

    fn test_user_id() -> UserId {
        UserId::new()
    }

    #[tokio::test]
    async fn save_and_get_draft() {
        let store = create_test_store();
        let user_id = test_user_id();

        let draft = PersistedEmailDraft::new(
            user_id,
            "recipient@example.com",
            "Test Subject",
            "Test body",
        );
        let draft_id = draft.id;

        // Save draft
        let saved_id = store.save(&draft).await.unwrap();
        assert_eq!(saved_id, draft_id);

        // Retrieve draft
        let retrieved = store.get(&draft_id).await.unwrap();
        assert!(retrieved.is_some());

        let retrieved = retrieved.unwrap();
        assert_eq!(retrieved.id, draft_id);
        assert_eq!(retrieved.user_id, user_id);
        assert_eq!(retrieved.to, "recipient@example.com");
        assert_eq!(retrieved.subject, "Test Subject");
        assert_eq!(retrieved.body, "Test body");
    }

    #[tokio::test]
    async fn get_nonexistent_draft_returns_none() {
        let store = create_test_store();
        let id = DraftId::new();

        let result = store.get(&id).await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn get_expired_draft_returns_none() {
        let store = create_test_store();
        let user_id = test_user_id();

        // Create a draft that's already expired
        let mut draft = PersistedEmailDraft::new(
            user_id,
            "recipient@example.com",
            "Test",
            "Body",
        );
        draft.expires_at = Utc::now() - Duration::hours(1);
        let draft_id = draft.id;

        store.save(&draft).await.unwrap();

        // Should not return expired draft
        let result = store.get(&draft_id).await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn get_for_user_checks_ownership() {
        let store = create_test_store();
        let user1 = test_user_id();
        let user2 = test_user_id();

        let draft = PersistedEmailDraft::new(
            user1,
            "recipient@example.com",
            "Test",
            "Body",
        );
        let draft_id = draft.id;
        store.save(&draft).await.unwrap();

        // User1 can access
        let result = store.get_for_user(&draft_id, &user1).await.unwrap();
        assert!(result.is_some());

        // User2 cannot access
        let result = store.get_for_user(&draft_id, &user2).await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn delete_draft() {
        let store = create_test_store();
        let user_id = test_user_id();

        let draft = PersistedEmailDraft::new(
            user_id,
            "recipient@example.com",
            "Test",
            "Body",
        );
        let draft_id = draft.id;
        store.save(&draft).await.unwrap();

        // Delete returns true
        let deleted = store.delete(&draft_id).await.unwrap();
        assert!(deleted);

        // Draft no longer exists
        let result = store.get(&draft_id).await.unwrap();
        assert!(result.is_none());

        // Deleting again returns false
        let deleted = store.delete(&draft_id).await.unwrap();
        assert!(!deleted);
    }

    #[tokio::test]
    async fn list_for_user() {
        let store = create_test_store();
        let user1 = test_user_id();
        let user2 = test_user_id();

        // Create drafts for user1
        for i in 0..5 {
            let draft = PersistedEmailDraft::new(
                user1,
                "recipient@example.com",
                format!("Subject {i}"),
                "Body",
            );
            store.save(&draft).await.unwrap();
        }

        // Create draft for user2
        let draft = PersistedEmailDraft::new(
            user2,
            "other@example.com",
            "User2 Subject",
            "Body",
        );
        store.save(&draft).await.unwrap();

        // List for user1 (limit 3)
        let drafts = store.list_for_user(&user1, 3).await.unwrap();
        assert_eq!(drafts.len(), 3);
        assert!(drafts.iter().all(|d| d.user_id == user1));

        // List for user1 (all)
        let drafts = store.list_for_user(&user1, 10).await.unwrap();
        assert_eq!(drafts.len(), 5);

        // List for user2
        let drafts = store.list_for_user(&user2, 10).await.unwrap();
        assert_eq!(drafts.len(), 1);
        assert_eq!(drafts[0].subject, "User2 Subject");
    }

    #[tokio::test]
    async fn list_for_user_excludes_expired() {
        let store = create_test_store();
        let user_id = test_user_id();

        // Create valid draft
        let valid_draft = PersistedEmailDraft::new(
            user_id,
            "recipient@example.com",
            "Valid",
            "Body",
        );
        store.save(&valid_draft).await.unwrap();

        // Create expired draft
        let mut expired_draft = PersistedEmailDraft::new(
            user_id,
            "recipient@example.com",
            "Expired",
            "Body",
        );
        expired_draft.expires_at = Utc::now() - Duration::hours(1);
        store.save(&expired_draft).await.unwrap();

        // Only valid draft should be listed
        let drafts = store.list_for_user(&user_id, 10).await.unwrap();
        assert_eq!(drafts.len(), 1);
        assert_eq!(drafts[0].subject, "Valid");
    }

    #[tokio::test]
    async fn cleanup_expired_drafts() {
        let store = create_test_store();
        let user_id = test_user_id();

        // Create valid draft
        let valid_draft = PersistedEmailDraft::new(
            user_id,
            "recipient@example.com",
            "Valid",
            "Body",
        );
        let valid_id = valid_draft.id;
        store.save(&valid_draft).await.unwrap();

        // Create expired drafts
        for i in 0..3 {
            let mut expired = PersistedEmailDraft::new(
                user_id,
                "recipient@example.com",
                format!("Expired {i}"),
                "Body",
            );
            expired.expires_at = Utc::now() - Duration::hours(1);
            store.save(&expired).await.unwrap();
        }

        // Cleanup
        let deleted = store.cleanup_expired().await.unwrap();
        assert_eq!(deleted, 3);

        // Valid draft still exists (via raw get bypassing expired filter)
        let pool = store.pool.clone();
        let id_str = valid_id.to_string();
        let exists = task::spawn_blocking(move || {
            let conn = pool.get().unwrap();
            conn.query_row(
                "SELECT COUNT(*) FROM email_drafts WHERE id = ?1",
                [&id_str],
                |row| row.get::<_, i32>(0),
            )
            .unwrap()
        })
        .await
        .unwrap();
        assert_eq!(exists, 1);
    }

    #[tokio::test]
    async fn draft_with_cc_recipients() {
        let store = create_test_store();
        let user_id = test_user_id();

        let draft = PersistedEmailDraft::new(
            user_id,
            "recipient@example.com",
            "Test",
            "Body",
        )
        .with_ccs(["cc1@example.com", "cc2@example.com"]);

        let draft_id = draft.id;
        store.save(&draft).await.unwrap();

        let retrieved = store.get(&draft_id).await.unwrap().unwrap();
        assert_eq!(retrieved.cc.len(), 2);
        assert!(retrieved.cc.contains(&"cc1@example.com".to_string()));
        assert!(retrieved.cc.contains(&"cc2@example.com".to_string()));
    }

    #[tokio::test]
    async fn draft_timestamps_preserved() {
        let store = create_test_store();
        let user_id = test_user_id();

        let draft = PersistedEmailDraft::new(
            user_id,
            "recipient@example.com",
            "Test",
            "Body",
        );
        let draft_id = draft.id;
        let original_created = draft.created_at;
        let original_expires = draft.expires_at;

        store.save(&draft).await.unwrap();

        let retrieved = store.get(&draft_id).await.unwrap().unwrap();
        // Allow 1 second tolerance for serialization/deserialization
        assert!((retrieved.created_at - original_created).num_seconds().abs() <= 1);
        assert!((retrieved.expires_at - original_expires).num_seconds().abs() <= 1);
    }
}
