//! SQLite-based reminder persistence

use std::sync::Arc;

use application::{
    error::ApplicationError,
    ports::{ReminderPort, ReminderQuery},
};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use domain::entities::{Reminder, ReminderSource, ReminderStatus};
use domain::value_objects::{ReminderId, UserId};
use rusqlite::{Row, params};
use tokio::task;
use tracing::{debug, instrument};
use uuid::Uuid;

use super::connection::ConnectionPool;

/// SQLite-based reminder store
#[derive(Debug, Clone)]
pub struct SqliteReminderStore {
    pool: Arc<ConnectionPool>,
}

impl SqliteReminderStore {
    /// Create a new SQLite reminder store
    #[must_use]
    pub const fn new(pool: Arc<ConnectionPool>) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl ReminderPort for SqliteReminderStore {
    #[instrument(skip(self, reminder), fields(reminder_id = %reminder.id))]
    async fn save(&self, reminder: &Reminder) -> Result<(), ApplicationError> {
        let pool = Arc::clone(&self.pool);
        let reminder = reminder.clone();

        task::spawn_blocking(move || {
            let conn = pool
                .get()
                .map_err(|e| ApplicationError::Internal(e.to_string()))?;

            conn.execute(
                "INSERT INTO reminders (
                    id, user_id, source, source_id, title, description,
                    event_time, remind_at, location, status,
                    snooze_count, max_snooze, created_at, updated_at
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
                params![
                    reminder.id.to_string(),
                    reminder.user_id.to_string(),
                    source_to_str(reminder.source),
                    reminder.source_id,
                    reminder.title,
                    reminder.description,
                    reminder.event_time.map(|t| t.to_rfc3339()),
                    reminder.remind_at.to_rfc3339(),
                    reminder.location,
                    status_to_str(reminder.status),
                    reminder.snooze_count,
                    reminder.max_snooze,
                    reminder.created_at.to_rfc3339(),
                    reminder.updated_at.to_rfc3339(),
                ],
            )
            .map_err(|e| ApplicationError::Internal(e.to_string()))?;

            debug!("Saved reminder");
            Ok(())
        })
        .await
        .map_err(|e| ApplicationError::Internal(e.to_string()))?
    }

    #[instrument(skip(self), fields(reminder_id = %id))]
    async fn get(&self, id: &ReminderId) -> Result<Option<Reminder>, ApplicationError> {
        let pool = Arc::clone(&self.pool);
        let id_str = id.to_string();

        task::spawn_blocking(move || {
            let conn = pool
                .get()
                .map_err(|e| ApplicationError::Internal(e.to_string()))?;

            let result = conn
                .query_row(
                    "SELECT id, user_id, source, source_id, title, description,
                        event_time, remind_at, location, status,
                        snooze_count, max_snooze, created_at, updated_at
                     FROM reminders WHERE id = ?1",
                    [&id_str],
                    row_to_reminder,
                )
                .optional()
                .map_err(|e| ApplicationError::Internal(e.to_string()))?;

            Ok(result)
        })
        .await
        .map_err(|e| ApplicationError::Internal(e.to_string()))?
    }

    #[instrument(skip(self))]
    async fn get_by_source_id(
        &self,
        source: ReminderSource,
        source_id: &str,
    ) -> Result<Option<Reminder>, ApplicationError> {
        let pool = Arc::clone(&self.pool);
        let source_str = source_to_str(source).to_string();
        let source_id = source_id.to_string();

        task::spawn_blocking(move || {
            let conn = pool
                .get()
                .map_err(|e| ApplicationError::Internal(e.to_string()))?;

            let result = conn
                .query_row(
                    "SELECT id, user_id, source, source_id, title, description,
                        event_time, remind_at, location, status,
                        snooze_count, max_snooze, created_at, updated_at
                     FROM reminders WHERE source = ?1 AND source_id = ?2",
                    params![source_str, source_id],
                    row_to_reminder,
                )
                .optional()
                .map_err(|e| ApplicationError::Internal(e.to_string()))?;

            Ok(result)
        })
        .await
        .map_err(|e| ApplicationError::Internal(e.to_string()))?
    }

    #[instrument(skip(self, reminder), fields(reminder_id = %reminder.id))]
    async fn update(&self, reminder: &Reminder) -> Result<(), ApplicationError> {
        let pool = Arc::clone(&self.pool);
        let reminder = reminder.clone();

        task::spawn_blocking(move || {
            let conn = pool
                .get()
                .map_err(|e| ApplicationError::Internal(e.to_string()))?;

            let affected = conn
                .execute(
                    "UPDATE reminders SET
                        title = ?1, description = ?2, event_time = ?3,
                        remind_at = ?4, location = ?5, status = ?6,
                        snooze_count = ?7, max_snooze = ?8, updated_at = ?9
                     WHERE id = ?10",
                    params![
                        reminder.title,
                        reminder.description,
                        reminder.event_time.map(|t| t.to_rfc3339()),
                        reminder.remind_at.to_rfc3339(),
                        reminder.location,
                        status_to_str(reminder.status),
                        reminder.snooze_count,
                        reminder.max_snooze,
                        reminder.updated_at.to_rfc3339(),
                        reminder.id.to_string(),
                    ],
                )
                .map_err(|e| ApplicationError::Internal(e.to_string()))?;

            if affected == 0 {
                return Err(ApplicationError::NotFound(format!(
                    "Reminder {} not found",
                    reminder.id
                )));
            }

            debug!("Updated reminder");
            Ok(())
        })
        .await
        .map_err(|e| ApplicationError::Internal(e.to_string()))?
    }

    #[instrument(skip(self), fields(reminder_id = %id))]
    async fn delete(&self, id: &ReminderId) -> Result<(), ApplicationError> {
        let pool = Arc::clone(&self.pool);
        let id_str = id.to_string();

        task::spawn_blocking(move || {
            let conn = pool
                .get()
                .map_err(|e| ApplicationError::Internal(e.to_string()))?;

            conn.execute("DELETE FROM reminders WHERE id = ?1", [&id_str])
                .map_err(|e| ApplicationError::Internal(e.to_string()))?;

            debug!("Deleted reminder");
            Ok(())
        })
        .await
        .map_err(|e| ApplicationError::Internal(e.to_string()))?
    }

    #[instrument(skip(self))]
    async fn query(&self, query: &ReminderQuery) -> Result<Vec<Reminder>, ApplicationError> {
        let pool = Arc::clone(&self.pool);
        let query = query.clone();

        task::spawn_blocking(move || {
            let conn = pool
                .get()
                .map_err(|e| ApplicationError::Internal(e.to_string()))?;

            let mut sql = String::from(
                "SELECT id, user_id, source, source_id, title, description,
                    event_time, remind_at, location, status,
                    snooze_count, max_snooze, created_at, updated_at
                 FROM reminders WHERE 1=1",
            );
            let mut param_values: Vec<String> = Vec::new();

            if let Some(ref user_id) = query.user_id {
                param_values.push(user_id.to_string());
                sql.push_str(&format!(" AND user_id = ?{}", param_values.len()));
            }

            if let Some(status) = query.status {
                param_values.push(status_to_str(status).to_string());
                sql.push_str(&format!(" AND status = ?{}", param_values.len()));
            }

            if let Some(source) = query.source {
                param_values.push(source_to_str(source).to_string());
                sql.push_str(&format!(" AND source = ?{}", param_values.len()));
            }

            if let Some(ref due_before) = query.due_before {
                param_values.push(due_before.to_rfc3339());
                sql.push_str(&format!(" AND remind_at <= ?{}", param_values.len()));
            }

            if !query.include_terminal {
                sql.push_str(" AND status IN ('pending', 'sent', 'snoozed')");
            }

            sql.push_str(" ORDER BY remind_at ASC");

            if let Some(limit) = query.limit {
                param_values.push(limit.to_string());
                sql.push_str(&format!(" LIMIT ?{}", param_values.len()));
            }

            let mut stmt = conn
                .prepare(&sql)
                .map_err(|e| ApplicationError::Internal(e.to_string()))?;

            let params_refs: Vec<&dyn rusqlite::types::ToSql> = param_values
                .iter()
                .map(|s| s as &dyn rusqlite::types::ToSql)
                .collect();

            let reminders: Vec<Reminder> = stmt
                .query_map(params_refs.as_slice(), row_to_reminder)
                .map_err(|e| ApplicationError::Internal(e.to_string()))?
                .filter_map(Result::ok)
                .collect();

            Ok(reminders)
        })
        .await
        .map_err(|e| ApplicationError::Internal(e.to_string()))?
    }

    #[instrument(skip(self))]
    async fn get_due_reminders(&self) -> Result<Vec<Reminder>, ApplicationError> {
        let pool = Arc::clone(&self.pool);
        let now = Utc::now().to_rfc3339();

        task::spawn_blocking(move || {
            let conn = pool
                .get()
                .map_err(|e| ApplicationError::Internal(e.to_string()))?;

            let mut stmt = conn
                .prepare(
                    "SELECT id, user_id, source, source_id, title, description,
                        event_time, remind_at, location, status,
                        snooze_count, max_snooze, created_at, updated_at
                     FROM reminders
                     WHERE status IN ('pending', 'snoozed')
                       AND remind_at <= ?1
                     ORDER BY remind_at ASC",
                )
                .map_err(|e| ApplicationError::Internal(e.to_string()))?;

            let reminders: Vec<Reminder> = stmt
                .query_map([&now], row_to_reminder)
                .map_err(|e| ApplicationError::Internal(e.to_string()))?
                .filter_map(Result::ok)
                .collect();

            debug!(count = reminders.len(), "Fetched due reminders");
            Ok(reminders)
        })
        .await
        .map_err(|e| ApplicationError::Internal(e.to_string()))?
    }

    #[instrument(skip(self))]
    async fn count_active(&self, user_id: &UserId) -> Result<u64, ApplicationError> {
        let pool = Arc::clone(&self.pool);
        let user_id_str = user_id.to_string();

        task::spawn_blocking(move || {
            let conn = pool
                .get()
                .map_err(|e| ApplicationError::Internal(e.to_string()))?;

            let count: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM reminders
                     WHERE user_id = ?1
                       AND status IN ('pending', 'sent', 'snoozed')",
                    [&user_id_str],
                    |row| row.get(0),
                )
                .map_err(|e| ApplicationError::Internal(e.to_string()))?;

            #[allow(clippy::cast_sign_loss)] // COUNT(*) is always non-negative
            Ok(count as u64)
        })
        .await
        .map_err(|e| ApplicationError::Internal(e.to_string()))?
    }

    #[instrument(skip(self))]
    async fn cleanup_old(&self, older_than: DateTime<Utc>) -> Result<u64, ApplicationError> {
        let pool = Arc::clone(&self.pool);
        let threshold = older_than.to_rfc3339();

        task::spawn_blocking(move || {
            let conn = pool
                .get()
                .map_err(|e| ApplicationError::Internal(e.to_string()))?;

            let deleted = conn
                .execute(
                    "DELETE FROM reminders
                     WHERE status IN ('acknowledged', 'cancelled', 'expired')
                       AND updated_at < ?1",
                    [&threshold],
                )
                .map_err(|e| ApplicationError::Internal(e.to_string()))?;

            debug!(deleted, "Cleaned up old reminders");
            #[allow(clippy::cast_sign_loss)] // DELETE count is always non-negative
            Ok(deleted as u64)
        })
        .await
        .map_err(|e| ApplicationError::Internal(e.to_string()))?
    }
}

/// Convert a database row to a Reminder domain entity
fn row_to_reminder(row: &Row<'_>) -> rusqlite::Result<Reminder> {
    let id_str: String = row.get(0)?;
    let user_id_str: String = row.get(1)?;
    let source_str: String = row.get(2)?;
    let source_id: Option<String> = row.get(3)?;
    let title: String = row.get(4)?;
    let description: Option<String> = row.get(5)?;
    let event_time_str: Option<String> = row.get(6)?;
    let remind_at_str: String = row.get(7)?;
    let location: Option<String> = row.get(8)?;
    let status_str: String = row.get(9)?;
    let snooze_count: i32 = row.get(10)?;
    let max_snooze: i32 = row.get(11)?;
    let created_at_str: String = row.get(12)?;
    let updated_at_str: String = row.get(13)?;

    let id = ReminderId::parse(&id_str).unwrap_or_else(|_| ReminderId::from(Uuid::new_v4()));
    let user_id = UserId::parse(&user_id_str).unwrap_or_else(|_| UserId::from(Uuid::new_v4()));

    let source = str_to_source(&source_str);
    let status = str_to_status(&status_str);

    let event_time = event_time_str.and_then(|s| {
        DateTime::parse_from_rfc3339(&s)
            .ok()
            .map(|dt| dt.with_timezone(&Utc))
    });
    let remind_at = DateTime::parse_from_rfc3339(&remind_at_str)
        .map_or_else(|_| Utc::now(), |dt| dt.with_timezone(&Utc));
    let created_at = DateTime::parse_from_rfc3339(&created_at_str)
        .map_or_else(|_| Utc::now(), |dt| dt.with_timezone(&Utc));
    let updated_at = DateTime::parse_from_rfc3339(&updated_at_str)
        .map_or_else(|_| Utc::now(), |dt| dt.with_timezone(&Utc));

    Ok(Reminder {
        id,
        user_id,
        source,
        source_id,
        title,
        description,
        event_time,
        remind_at,
        location,
        status,
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // DB values are validated
        snooze_count: snooze_count as u8,
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // DB values are validated
        max_snooze: max_snooze as u8,
        created_at,
        updated_at,
    })
}

/// Convert a `ReminderSource` to its database string representation
const fn source_to_str(source: ReminderSource) -> &'static str {
    match source {
        ReminderSource::CalendarEvent => "calendar_event",
        ReminderSource::CalendarTask => "calendar_task",
        ReminderSource::Custom => "custom",
    }
}

/// Convert a database string to a `ReminderSource`
fn str_to_source(s: &str) -> ReminderSource {
    match s {
        "calendar_event" => ReminderSource::CalendarEvent,
        "calendar_task" => ReminderSource::CalendarTask,
        _ => ReminderSource::Custom,
    }
}

/// Convert a `ReminderStatus` to its database string representation
const fn status_to_str(status: ReminderStatus) -> &'static str {
    match status {
        ReminderStatus::Pending => "pending",
        ReminderStatus::Sent => "sent",
        ReminderStatus::Acknowledged => "acknowledged",
        ReminderStatus::Snoozed => "snoozed",
        ReminderStatus::Cancelled => "cancelled",
        ReminderStatus::Expired => "expired",
    }
}

/// Convert a database string to a `ReminderStatus`
#[allow(clippy::match_same_arms)] // Fallback to Pending is intentional
fn str_to_status(s: &str) -> ReminderStatus {
    match s {
        "pending" => ReminderStatus::Pending,
        "sent" => ReminderStatus::Sent,
        "acknowledged" => ReminderStatus::Acknowledged,
        "snoozed" => ReminderStatus::Snoozed,
        "cancelled" => ReminderStatus::Cancelled,
        "expired" => ReminderStatus::Expired,
        _ => ReminderStatus::Pending,
    }
}

/// Extension trait for optional query results
trait OptionalExt<T> {
    fn optional(self) -> rusqlite::Result<Option<T>>;
}

impl<T> OptionalExt<T> for rusqlite::Result<T> {
    fn optional(self) -> rusqlite::Result<Option<T>> {
        match self {
            Ok(val) => Ok(Some(val)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e),
        }
    }
}

#[cfg(test)]
mod tests {
    use chrono::Duration;

    use super::*;
    use crate::{config::DatabaseConfig, persistence::connection::create_pool};

    fn create_test_store() -> SqliteReminderStore {
        let config = DatabaseConfig {
            path: ":memory:".to_string(),
            max_connections: 1,
            run_migrations: true,
        };
        let pool = create_pool(&config).unwrap();
        SqliteReminderStore::new(Arc::new(pool))
    }

    fn test_user_id() -> UserId {
        UserId::new()
    }

    #[tokio::test]
    async fn save_and_get_reminder() {
        let store = create_test_store();
        let user_id = test_user_id();
        let remind_at = Utc::now() + Duration::hours(1);
        let reminder = Reminder::new(user_id, ReminderSource::Custom, "Buy milk", remind_at)
            .with_description("From the store on the corner");

        store.save(&reminder).await.unwrap();

        let retrieved = store.get(&reminder.id).await.unwrap();
        assert!(retrieved.is_some());
        let r = retrieved.unwrap();
        assert_eq!(r.id, reminder.id);
        assert_eq!(r.title, "Buy milk");
        assert_eq!(
            r.description.as_deref(),
            Some("From the store on the corner")
        );
        assert_eq!(r.source, ReminderSource::Custom);
        assert_eq!(r.status, ReminderStatus::Pending);
    }

    #[tokio::test]
    async fn get_nonexistent_returns_none() {
        let store = create_test_store();
        let result = store.get(&ReminderId::new()).await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn save_with_location_and_event_time() {
        let store = create_test_store();
        let event_time = Utc::now() + Duration::hours(2);
        let remind_at = Utc::now() + Duration::hours(1);
        let reminder = Reminder::new(
            test_user_id(),
            ReminderSource::CalendarEvent,
            "Meeting",
            remind_at,
        )
        .with_event_time(event_time)
        .with_location("Room 42, Berlin Hbf")
        .with_source_id("caldav-uid-123");

        store.save(&reminder).await.unwrap();

        let r = store.get(&reminder.id).await.unwrap().unwrap();
        assert_eq!(r.location.as_deref(), Some("Room 42, Berlin Hbf"));
        assert!(r.event_time.is_some());
        assert_eq!(r.source_id.as_deref(), Some("caldav-uid-123"));
    }

    #[tokio::test]
    async fn get_by_source_id() {
        let store = create_test_store();
        let reminder = Reminder::new(
            test_user_id(),
            ReminderSource::CalendarEvent,
            "Event",
            Utc::now() + Duration::hours(1),
        )
        .with_source_id("uid-abc-1h");

        store.save(&reminder).await.unwrap();

        let found = store
            .get_by_source_id(ReminderSource::CalendarEvent, "uid-abc-1h")
            .await
            .unwrap();
        assert!(found.is_some());
        assert_eq!(found.unwrap().id, reminder.id);

        // Not found with wrong source
        let not_found = store
            .get_by_source_id(ReminderSource::Custom, "uid-abc-1h")
            .await
            .unwrap();
        assert!(not_found.is_none());
    }

    #[tokio::test]
    async fn update_reminder() {
        let store = create_test_store();
        let mut reminder = Reminder::new(
            test_user_id(),
            ReminderSource::Custom,
            "Original",
            Utc::now() + Duration::hours(1),
        );
        store.save(&reminder).await.unwrap();

        reminder.mark_sent();
        store.update(&reminder).await.unwrap();

        let updated = store.get(&reminder.id).await.unwrap().unwrap();
        assert_eq!(updated.status, ReminderStatus::Sent);
    }

    #[tokio::test]
    async fn delete_reminder() {
        let store = create_test_store();
        let reminder = Reminder::new(
            test_user_id(),
            ReminderSource::Custom,
            "Delete me",
            Utc::now() + Duration::hours(1),
        );
        store.save(&reminder).await.unwrap();

        store.delete(&reminder.id).await.unwrap();

        let result = store.get(&reminder.id).await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn query_active_for_user() {
        let store = create_test_store();
        let user = test_user_id();
        let other_user = test_user_id();

        // Create reminders for our user
        let r1 = Reminder::new(
            user,
            ReminderSource::Custom,
            "Active 1",
            Utc::now() + Duration::hours(1),
        );
        let r2 = Reminder::new(
            user,
            ReminderSource::Custom,
            "Active 2",
            Utc::now() + Duration::hours(2),
        );
        // Create for other user
        let r3 = Reminder::new(
            other_user,
            ReminderSource::Custom,
            "Other user",
            Utc::now() + Duration::hours(1),
        );

        store.save(&r1).await.unwrap();
        store.save(&r2).await.unwrap();
        store.save(&r3).await.unwrap();

        let query = ReminderQuery::active_for_user(user);
        let results = store.query(&query).await.unwrap();
        assert_eq!(results.len(), 2);
    }

    #[tokio::test]
    async fn get_due_reminders() {
        let store = create_test_store();
        let user = test_user_id();

        // Due reminder (past time)
        let r1 = Reminder::new(
            user,
            ReminderSource::Custom,
            "Due now",
            Utc::now() - Duration::minutes(5),
        );
        // Future reminder (not due)
        let r2 = Reminder::new(
            user,
            ReminderSource::Custom,
            "Future",
            Utc::now() + Duration::hours(1),
        );

        store.save(&r1).await.unwrap();
        store.save(&r2).await.unwrap();

        let due = store.get_due_reminders().await.unwrap();
        assert_eq!(due.len(), 1);
        assert_eq!(due[0].title, "Due now");
    }

    #[tokio::test]
    async fn count_active_reminders() {
        let store = create_test_store();
        let user = test_user_id();

        let r1 = Reminder::new(
            user,
            ReminderSource::Custom,
            "R1",
            Utc::now() + Duration::hours(1),
        );
        let r2 = Reminder::new(
            user,
            ReminderSource::Custom,
            "R2",
            Utc::now() + Duration::hours(2),
        );

        store.save(&r1).await.unwrap();
        store.save(&r2).await.unwrap();

        let count = store.count_active(&user).await.unwrap();
        assert_eq!(count, 2);
    }

    #[tokio::test]
    async fn cleanup_old_reminders() {
        let store = create_test_store();
        let user = test_user_id();

        // Create and acknowledge a reminder
        let mut r1 = Reminder::new(
            user,
            ReminderSource::Custom,
            "Old done",
            Utc::now() - Duration::days(35),
        );
        r1.acknowledge();
        // Tan the updated_at to be old
        r1.updated_at = Utc::now() - Duration::days(35);
        store.save(&r1).await.unwrap();
        // Update to set the old updated_at
        store.update(&r1).await.unwrap();

        // Create a recent pending reminder
        let r2 = Reminder::new(
            user,
            ReminderSource::Custom,
            "Active",
            Utc::now() + Duration::hours(1),
        );
        store.save(&r2).await.unwrap();

        let threshold = Utc::now() - Duration::days(30);
        let deleted = store.cleanup_old(threshold).await.unwrap();
        assert_eq!(deleted, 1);

        // Active reminder should still exist
        let active = store.get(&r2.id).await.unwrap();
        assert!(active.is_some());
    }

    #[tokio::test]
    async fn snooze_lifecycle() {
        let store = create_test_store();
        let mut reminder = Reminder::new(
            test_user_id(),
            ReminderSource::Custom,
            "Snooze test",
            Utc::now() - Duration::minutes(5),
        );
        store.save(&reminder).await.unwrap();

        let new_time = Utc::now() + Duration::minutes(15);
        reminder.snooze(new_time);
        store.update(&reminder).await.unwrap();

        let updated = store.get(&reminder.id).await.unwrap().unwrap();
        assert_eq!(updated.status, ReminderStatus::Snoozed);
        assert_eq!(updated.snooze_count, 1);
    }

    #[test]
    fn source_enum_roundtrip() {
        for source in [
            ReminderSource::CalendarEvent,
            ReminderSource::CalendarTask,
            ReminderSource::Custom,
        ] {
            let s = source_to_str(source);
            assert_eq!(str_to_source(s), source);
        }
    }

    #[test]
    fn status_enum_roundtrip() {
        for status in [
            ReminderStatus::Pending,
            ReminderStatus::Sent,
            ReminderStatus::Acknowledged,
            ReminderStatus::Snoozed,
            ReminderStatus::Cancelled,
            ReminderStatus::Expired,
        ] {
            let s = status_to_str(status);
            assert_eq!(str_to_status(s), status);
        }
    }
}
