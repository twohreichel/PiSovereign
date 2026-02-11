//! Reminder storage port
//!
//! Defines the interface for persisting and querying reminders.
//! Adapters in the infrastructure layer implement this port using SQLite.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use domain::entities::{Reminder, ReminderSource, ReminderStatus};
use domain::value_objects::{ReminderId, UserId};
#[cfg(test)]
use mockall::automock;

use crate::error::ApplicationError;

/// Query options for listing reminders
#[derive(Debug, Clone, Default)]
pub struct ReminderQuery {
    /// Filter by user
    pub user_id: Option<UserId>,
    /// Filter by status
    pub status: Option<ReminderStatus>,
    /// Filter by source type
    pub source: Option<ReminderSource>,
    /// Only reminders due before this time
    pub due_before: Option<DateTime<Utc>>,
    /// Include terminal (done/cancelled/expired) reminders
    pub include_terminal: bool,
    /// Maximum number of results
    pub limit: Option<u32>,
}

impl ReminderQuery {
    /// Create a query for all active reminders of a user
    #[must_use]
    pub fn active_for_user(user_id: UserId) -> Self {
        Self {
            user_id: Some(user_id),
            include_terminal: false,
            ..Default::default()
        }
    }

    /// Create a query for due reminders (ready to fire)
    #[must_use]
    pub fn due_now() -> Self {
        Self {
            due_before: Some(Utc::now()),
            include_terminal: false,
            ..Default::default()
        }
    }

    /// Set status filter
    #[must_use]
    pub const fn with_status(mut self, status: ReminderStatus) -> Self {
        self.status = Some(status);
        self
    }

    /// Set source filter
    #[must_use]
    pub const fn with_source(mut self, source: ReminderSource) -> Self {
        self.source = Some(source);
        self
    }

    /// Set limit
    #[must_use]
    pub const fn with_limit(mut self, limit: u32) -> Self {
        self.limit = Some(limit);
        self
    }
}

/// Port for reminder persistence operations
#[cfg_attr(test, automock)]
#[async_trait]
pub trait ReminderPort: Send + Sync {
    /// Save a new reminder
    async fn save(&self, reminder: &Reminder) -> Result<(), ApplicationError>;

    /// Get a reminder by ID
    async fn get(&self, id: &ReminderId) -> Result<Option<Reminder>, ApplicationError>;

    /// Get a reminder by its source ID (for deduplication)
    async fn get_by_source_id(
        &self,
        source: ReminderSource,
        source_id: &str,
    ) -> Result<Option<Reminder>, ApplicationError>;

    /// Update an existing reminder
    async fn update(&self, reminder: &Reminder) -> Result<(), ApplicationError>;

    /// Delete a reminder
    async fn delete(&self, id: &ReminderId) -> Result<(), ApplicationError>;

    /// Query reminders with filters
    async fn query(&self, query: &ReminderQuery) -> Result<Vec<Reminder>, ApplicationError>;

    /// Get all reminders that are due (ready to fire)
    async fn get_due_reminders(&self) -> Result<Vec<Reminder>, ApplicationError>;

    /// Count active reminders for a user
    async fn count_active(&self, user_id: &UserId) -> Result<u64, ApplicationError>;

    /// Delete all expired/completed reminders older than a given threshold
    async fn cleanup_old(&self, older_than: DateTime<Utc>) -> Result<u64, ApplicationError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn _assert_object_safe(_: &dyn ReminderPort) {}

    #[test]
    fn trait_is_send_sync() {
        fn assert_send_sync<T: Send + Sync + ?Sized>() {}
        assert_send_sync::<dyn ReminderPort>();
    }

    #[test]
    fn query_active_for_user() {
        let user = UserId::new();
        let query = ReminderQuery::active_for_user(user);
        assert_eq!(query.user_id, Some(user));
        assert!(!query.include_terminal);
    }

    #[test]
    fn query_due_now() {
        let query = ReminderQuery::due_now();
        assert!(query.due_before.is_some());
        assert!(!query.include_terminal);
    }

    #[test]
    fn query_builder() {
        let query = ReminderQuery::default()
            .with_status(ReminderStatus::Pending)
            .with_source(ReminderSource::CalendarEvent)
            .with_limit(10);
        assert_eq!(query.status, Some(ReminderStatus::Pending));
        assert_eq!(query.source, Some(ReminderSource::CalendarEvent));
        assert_eq!(query.limit, Some(10));
    }
}
