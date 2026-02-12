//! SQLite draft store implementation
//!
//! Implements the `DraftStorePort` for persisting email drafts using sqlx.

use application::{error::ApplicationError, ports::DraftStorePort};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use domain::{DraftId, EmailAddress, PersistedEmailDraft, UserId};
use sqlx::SqlitePool;
use tracing::{debug, instrument};
use uuid::Uuid;

use super::error::map_sqlx_error;

/// SQLite-based email draft store
#[derive(Debug, Clone)]
pub struct SqliteDraftStore {
    pool: SqlitePool,
}

impl SqliteDraftStore {
    /// Create a new SQLite draft store
    #[must_use]
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

/// Row type for draft queries
#[derive(sqlx::FromRow)]
struct DraftRow {
    id: String,
    user_id: String,
    to_address: String,
    cc: Option<String>,
    subject: String,
    body: String,
    created_at: String,
    expires_at: String,
}

impl DraftRow {
    fn to_draft(self) -> PersistedEmailDraft {
        let id = DraftId::parse(&self.id).unwrap_or_else(|_| DraftId::from(Uuid::new_v4()));
        let user_id = UserId::parse(&self.user_id).unwrap_or_else(|_| UserId::from(Uuid::new_v4()));

        let to = EmailAddress::new(&self.to_address).unwrap_or_else(|_| {
            EmailAddress::new("unknown@invalid.local").expect("fallback email")
        });

        let cc = self
            .cc
            .map(|s| {
                s.split(',')
                    .filter_map(|addr| EmailAddress::new(addr.trim()).ok())
                    .collect()
            })
            .unwrap_or_default();

        let created_at = DateTime::parse_from_rfc3339(&self.created_at)
            .map_or_else(|_| Utc::now(), |dt| dt.with_timezone(&Utc));
        let expires_at = DateTime::parse_from_rfc3339(&self.expires_at)
            .map_or_else(|_| Utc::now(), |dt| dt.with_timezone(&Utc));

        PersistedEmailDraft {
            id,
            user_id,
            to,
            cc,
            subject: self.subject,
            body: self.body,
            created_at,
            expires_at,
        }
    }
}

#[async_trait]
impl DraftStorePort for SqliteDraftStore {
    #[instrument(skip(self, draft), fields(draft_id = %draft.id, user_id = %draft.user_id))]
    async fn save(&self, draft: &PersistedEmailDraft) -> Result<DraftId, ApplicationError> {
        let cc_str = if draft.cc.is_empty() {
            None
        } else {
            Some(
                draft
                    .cc
                    .iter()
                    .map(EmailAddress::as_str)
                    .collect::<Vec<_>>()
                    .join(","),
            )
        };

        sqlx::query(
            "INSERT INTO email_drafts (id, user_id, to_address, cc, subject, body, created_at, expires_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
        )
        .bind(draft.id.to_string())
        .bind(draft.user_id.to_string())
        .bind(draft.to.as_str())
        .bind(&cc_str)
        .bind(&draft.subject)
        .bind(&draft.body)
        .bind(draft.created_at.to_rfc3339())
        .bind(draft.expires_at.to_rfc3339())
        .execute(&self.pool)
        .await
        .map_err(map_sqlx_error)?;

        debug!("Saved email draft");
        Ok(draft.id)
    }

    #[instrument(skip(self), fields(draft_id = %id))]
    async fn get(&self, id: &DraftId) -> Result<Option<PersistedEmailDraft>, ApplicationError> {
        let row: Option<DraftRow> = sqlx::query_as(
            "SELECT id, user_id, to_address, cc, subject, body, created_at, expires_at
             FROM email_drafts WHERE id = $1",
        )
        .bind(id.to_string())
        .fetch_optional(&self.pool)
        .await
        .map_err(map_sqlx_error)?;

        // Filter out expired drafts
        Ok(row.map(DraftRow::to_draft).filter(|d| !d.is_expired()))
    }

    #[instrument(skip(self), fields(draft_id = %id, user_id = %user_id))]
    async fn get_for_user(
        &self,
        id: &DraftId,
        user_id: &UserId,
    ) -> Result<Option<PersistedEmailDraft>, ApplicationError> {
        let row: Option<DraftRow> = sqlx::query_as(
            "SELECT id, user_id, to_address, cc, subject, body, created_at, expires_at
             FROM email_drafts WHERE id = $1 AND user_id = $2",
        )
        .bind(id.to_string())
        .bind(user_id.to_string())
        .fetch_optional(&self.pool)
        .await
        .map_err(map_sqlx_error)?;

        Ok(row.map(DraftRow::to_draft).filter(|d| !d.is_expired()))
    }

    #[instrument(skip(self), fields(draft_id = %id))]
    async fn delete(&self, id: &DraftId) -> Result<bool, ApplicationError> {
        let result = sqlx::query("DELETE FROM email_drafts WHERE id = $1")
            .bind(id.to_string())
            .execute(&self.pool)
            .await
            .map_err(map_sqlx_error)?;

        let deleted = result.rows_affected() > 0;
        debug!(deleted, "Deleted email draft");
        Ok(deleted)
    }

    #[instrument(skip(self), fields(user_id = %user_id))]
    async fn list_for_user(
        &self,
        user_id: &UserId,
        limit: usize,
    ) -> Result<Vec<PersistedEmailDraft>, ApplicationError> {
        self.list_for_user_inner(user_id, limit).await
    }

    #[instrument(skip(self))]
    async fn cleanup_expired(&self) -> Result<usize, ApplicationError> {
        let now = Utc::now().to_rfc3339();

        let result = sqlx::query("DELETE FROM email_drafts WHERE expires_at <= $1")
            .bind(&now)
            .execute(&self.pool)
            .await
            .map_err(map_sqlx_error)?;

        let deleted = result.rows_affected() as usize;
        debug!(deleted_count = deleted, "Cleaned up expired drafts");
        Ok(deleted)
    }
}

/// Helper trait extension — not needed, using fetch_all directly
impl SqliteDraftStore {
    async fn list_for_user_inner(
        &self,
        user_id: &UserId,
        limit: usize,
    ) -> Result<Vec<PersistedEmailDraft>, ApplicationError> {
        let now = Utc::now().to_rfc3339();

        let rows: Vec<DraftRow> = sqlx::query_as(
            "SELECT id, user_id, to_address, cc, subject, body, created_at, expires_at
             FROM email_drafts
             WHERE user_id = $1 AND expires_at > $2
             ORDER BY created_at DESC
             LIMIT $3",
        )
        .bind(user_id.to_string())
        .bind(&now)
        .bind(limit as i64)
        .fetch_all(&self.pool)
        .await
        .map_err(map_sqlx_error)?;

        Ok(rows.into_iter().map(DraftRow::to_draft).collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::persistence::async_connection::AsyncDatabase;
    use chrono::Duration;

    async fn setup() -> (AsyncDatabase, SqliteDraftStore) {
        let db = AsyncDatabase::in_memory().await.unwrap();
        db.migrate().await.unwrap();
        let store = SqliteDraftStore::new(db.pool().clone());
        (db, store)
    }

    fn test_user_id() -> UserId {
        UserId::new()
    }

    fn email(addr: &str) -> EmailAddress {
        EmailAddress::new(addr).unwrap()
    }

    #[tokio::test]
    async fn save_and_get_draft() {
        let (_db, store) = setup().await;
        let user_id = test_user_id();

        let draft = PersistedEmailDraft::new(
            user_id,
            email("recipient@example.com"),
            "Test Subject",
            "Test body",
        );
        let draft_id = draft.id;

        let saved_id = store.save(&draft).await.unwrap();
        assert_eq!(saved_id, draft_id);

        let retrieved = store.get(&draft_id).await.unwrap();
        assert!(retrieved.is_some());

        let retrieved = retrieved.unwrap();
        assert_eq!(retrieved.id, draft_id);
        assert_eq!(retrieved.user_id, user_id);
        assert_eq!(retrieved.to, email("recipient@example.com"));
        assert_eq!(retrieved.subject, "Test Subject");
        assert_eq!(retrieved.body, "Test body");
    }

    #[tokio::test]
    async fn get_nonexistent_draft_returns_none() {
        let (_db, store) = setup().await;
        let result = store.get(&DraftId::new()).await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn get_expired_draft_returns_none() {
        let (_db, store) = setup().await;
        let user_id = test_user_id();

        let mut draft = PersistedEmailDraft::new(user_id, email("r@example.com"), "Test", "Body");
        draft.expires_at = Utc::now() - Duration::hours(1);
        let draft_id = draft.id;

        store.save(&draft).await.unwrap();

        let result = store.get(&draft_id).await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn get_for_user_checks_ownership() {
        let (_db, store) = setup().await;
        let user1 = test_user_id();
        let user2 = test_user_id();

        let draft = PersistedEmailDraft::new(user1, email("r@example.com"), "Test", "Body");
        let draft_id = draft.id;
        store.save(&draft).await.unwrap();

        assert!(
            store
                .get_for_user(&draft_id, &user1)
                .await
                .unwrap()
                .is_some()
        );
        assert!(
            store
                .get_for_user(&draft_id, &user2)
                .await
                .unwrap()
                .is_none()
        );
    }

    #[tokio::test]
    async fn delete_draft() {
        let (_db, store) = setup().await;

        let draft =
            PersistedEmailDraft::new(test_user_id(), email("r@example.com"), "Test", "Body");
        let draft_id = draft.id;
        store.save(&draft).await.unwrap();

        assert!(store.delete(&draft_id).await.unwrap());
        assert!(store.get(&draft_id).await.unwrap().is_none());
        assert!(!store.delete(&draft_id).await.unwrap());
    }

    #[tokio::test]
    async fn list_for_user() {
        let (_db, store) = setup().await;
        let user1 = test_user_id();
        let user2 = test_user_id();

        for i in 0..5 {
            let draft = PersistedEmailDraft::new(
                user1,
                email("r@example.com"),
                format!("Subject {i}"),
                "Body",
            );
            store.save(&draft).await.unwrap();
        }

        let draft =
            PersistedEmailDraft::new(user2, email("o@example.com"), "User2 Subject", "Body");
        store.save(&draft).await.unwrap();

        let drafts = store.list_for_user_inner(&user1, 3).await.unwrap();
        assert_eq!(drafts.len(), 3);
        assert!(drafts.iter().all(|d| d.user_id == user1));

        let drafts = store.list_for_user_inner(&user1, 10).await.unwrap();
        assert_eq!(drafts.len(), 5);

        let drafts = store.list_for_user_inner(&user2, 10).await.unwrap();
        assert_eq!(drafts.len(), 1);
        assert_eq!(drafts[0].subject, "User2 Subject");
    }

    #[tokio::test]
    async fn list_for_user_excludes_expired() {
        let (_db, store) = setup().await;
        let user_id = test_user_id();

        let valid = PersistedEmailDraft::new(user_id, email("r@example.com"), "Valid", "Body");
        store.save(&valid).await.unwrap();

        let mut expired =
            PersistedEmailDraft::new(user_id, email("r@example.com"), "Expired", "Body");
        expired.expires_at = Utc::now() - Duration::hours(1);
        store.save(&expired).await.unwrap();

        let drafts = store.list_for_user_inner(&user_id, 10).await.unwrap();
        assert_eq!(drafts.len(), 1);
        assert_eq!(drafts[0].subject, "Valid");
    }

    #[tokio::test]
    async fn cleanup_expired_drafts() {
        let (_db, store) = setup().await;
        let user_id = test_user_id();

        let valid = PersistedEmailDraft::new(user_id, email("r@example.com"), "Valid", "Body");
        store.save(&valid).await.unwrap();

        for i in 0..3 {
            let mut expired = PersistedEmailDraft::new(
                user_id,
                email("r@example.com"),
                format!("Expired {i}"),
                "Body",
            );
            expired.expires_at = Utc::now() - Duration::hours(1);
            store.save(&expired).await.unwrap();
        }

        let deleted = store.cleanup_expired().await.unwrap();
        assert_eq!(deleted, 3);
    }

    #[tokio::test]
    async fn draft_with_cc_recipients() {
        let (_db, store) = setup().await;

        let draft =
            PersistedEmailDraft::new(test_user_id(), email("r@example.com"), "Test", "Body")
                .with_ccs([email("cc1@example.com"), email("cc2@example.com")]);

        let draft_id = draft.id;
        store.save(&draft).await.unwrap();

        let retrieved = store.get(&draft_id).await.unwrap().unwrap();
        assert_eq!(retrieved.cc.len(), 2);
        assert!(retrieved.cc.contains(&email("cc1@example.com")));
        assert!(retrieved.cc.contains(&email("cc2@example.com")));
    }

    #[tokio::test]
    async fn draft_timestamps_preserved() {
        let (_db, store) = setup().await;

        let draft =
            PersistedEmailDraft::new(test_user_id(), email("r@example.com"), "Test", "Body");
        let draft_id = draft.id;
        let original_created = draft.created_at;
        let original_expires = draft.expires_at;

        store.save(&draft).await.unwrap();

        let retrieved = store.get(&draft_id).await.unwrap().unwrap();
        assert!(
            (retrieved.created_at - original_created)
                .num_seconds()
                .abs()
                <= 1
        );
        assert!(
            (retrieved.expires_at - original_expires)
                .num_seconds()
                .abs()
                <= 1
        );
    }
}
