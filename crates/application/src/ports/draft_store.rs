//! Draft storage port
//!
//! Defines the interface for persisting and retrieving email drafts.
//! Drafts are temporary documents that require approval before sending.

use async_trait::async_trait;
use domain::{DraftId, PersistedEmailDraft, UserId};

use crate::error::ApplicationError;

/// Port for email draft persistence
///
/// This port handles storage and retrieval of email drafts, which are
/// stored temporarily before they are approved and sent. Drafts have
/// a TTL (time-to-live) and are automatically cleaned up when expired.
#[async_trait]
pub trait DraftStorePort: Send + Sync {
    /// Save a new draft
    ///
    /// # Arguments
    /// * `draft` - The draft to save
    ///
    /// # Returns
    /// The ID of the saved draft
    async fn save(&self, draft: &PersistedEmailDraft) -> Result<DraftId, ApplicationError>;

    /// Get a draft by ID
    ///
    /// # Arguments
    /// * `id` - The draft ID to retrieve
    ///
    /// # Returns
    /// The draft if found and not expired, None otherwise
    async fn get(&self, id: &DraftId) -> Result<Option<PersistedEmailDraft>, ApplicationError>;

    /// Get a draft by ID for a specific user (ownership check)
    ///
    /// # Arguments
    /// * `id` - The draft ID to retrieve
    /// * `user_id` - The user who must own the draft
    ///
    /// # Returns
    /// The draft if found, not expired, and owned by the user
    async fn get_for_user(
        &self,
        id: &DraftId,
        user_id: &UserId,
    ) -> Result<Option<PersistedEmailDraft>, ApplicationError>;

    /// Delete a draft
    ///
    /// # Arguments
    /// * `id` - The draft ID to delete
    ///
    /// # Returns
    /// true if the draft was deleted, false if it didn't exist
    async fn delete(&self, id: &DraftId) -> Result<bool, ApplicationError>;

    /// List all drafts for a user
    ///
    /// # Arguments
    /// * `user_id` - The user whose drafts to list
    /// * `limit` - Maximum number of drafts to return
    ///
    /// # Returns
    /// List of drafts, newest first
    async fn list_for_user(
        &self,
        user_id: &UserId,
        limit: usize,
    ) -> Result<Vec<PersistedEmailDraft>, ApplicationError>;

    /// Cleanup expired drafts
    ///
    /// Removes all drafts that have passed their expiration time.
    ///
    /// # Returns
    /// The number of drafts deleted
    async fn cleanup_expired(&self) -> Result<usize, ApplicationError>;
}
