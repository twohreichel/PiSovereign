//! SQLite-based reminder persistence
//!
//! Implements the `ReminderPort` using sqlx for async reminder storage.

use application::{
    error::ApplicationError,
    ports::{ReminderPort, ReminderQuery},
};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use domain::entities::{Reminder, ReminderSource, ReminderStatus};
use domain::value_objects::{ReminderId, UserId};
use sqlx::SqlitePool;
use tracing::{debug, instrument};
use uuid::Uuid;

use super::error::map_sqlx_error;

/// SQLite-based reminder store
#[derive(Debug, Clone)]
pub struct SqliteReminderStore {
    pool: SqlitePool,
}

impl SqliteReminderStore {
    /// Create a new SQLite reminder store
    #[must_use]
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

/// Row type for reminder queries
#[derive(sqlx::FromRow)]
struct ReminderRow {
    id: String,
    user_id: String,
    source: String,
    source_id: Option<String>,
    title: String,
    description: Option<String>,
    event_time: Option<String>,
    remind_at: String,
    location: Option<String>,
    status: String,
    snooze_count: i32,
    max_snooze: i32,
    created_at: String,
    updated_at: String,
}

impl ReminderRow {
    #[allow(clippy::wrong_self_convention)]
    fn to_reminder(self) -> Reminder {
        let id = ReminderId::parse(&self.id).unwrap_or_else(|_| ReminderId::from(Uuid::new_v4()));
        let user_id = UserId::parse(&self.user_id).unwrap_or_else(|_| UserId::from(Uuid::new_v4()));
        let source = str_to_source(&self.source);
        let status = str_to_status(&self.status);

        let event_time = self.event_time.and_then(|s| {
            DateTime::parse_from_rfc3339(&s)
                .ok()
                .map(|dt| dt.with_timezone(&Utc))
        });
        let remind_at = DateTime::parse_from_rfc3339(&self.remind_at)
            .map_or_else(|_| Utc::now(), |dt| dt.with_timezone(&Utc));
        let created_at = DateTime::parse_from_rfc3339(&self.created_at)
            .map_or_else(|_| Utc::now(), |dt| dt.with_timezone(&Utc));
        let updated_at = DateTime::parse_from_rfc3339(&self.updated_at)
            .map_or_else(|_| Utc::now(), |dt| dt.with_timezone(&Utc));

        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        Reminder {
            id,
            user_id,
            source,
            source_id: self.source_id,
            title: self.title,
            description: self.description,
            event_time,
            remind_at,
            location: self.location,
            status,
            snooze_count: self.snooze_count as u8,
            max_snooze: self.max_snooze as u8,
            created_at,
            updated_at,
        }
    }
}

const SELECT_REMINDER: &str = "SELECT id, user_id, source, source_id, title, description, \
                                event_time, remind_at, location, status, \
                                snooze_count, max_snooze, created_at, updated_at
                                FROM reminders";

#[async_trait]
impl ReminderPort for SqliteReminderStore {
    #[instrument(skip(self, reminder), fields(reminder_id = %reminder.id))]
    async fn save(&self, reminder: &Reminder) -> Result<(), ApplicationError> {
        sqlx::query(
            "INSERT INTO reminders (
                id, user_id, source, source_id, title, description,
                event_time, remind_at, location, status,
                snooze_count, max_snooze, created_at, updated_at
            ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14)",
        )
        .bind(reminder.id.to_string())
        .bind(reminder.user_id.to_string())
        .bind(source_to_str(reminder.source))
        .bind(&reminder.source_id)
        .bind(&reminder.title)
        .bind(&reminder.description)
        .bind(reminder.event_time.map(|t| t.to_rfc3339()))
        .bind(reminder.remind_at.to_rfc3339())
        .bind(&reminder.location)
        .bind(status_to_str(reminder.status))
        .bind(i32::from(reminder.snooze_count))
        .bind(i32::from(reminder.max_snooze))
        .bind(reminder.created_at.to_rfc3339())
        .bind(reminder.updated_at.to_rfc3339())
        .execute(&self.pool)
        .await
        .map_err(map_sqlx_error)?;

        debug!("Saved reminder");
        Ok(())
    }

    #[instrument(skip(self), fields(reminder_id = %id))]
    async fn get(&self, id: &ReminderId) -> Result<Option<Reminder>, ApplicationError> {
        let sql = format!("{SELECT_REMINDER} WHERE id = $1");
        let row: Option<ReminderRow> = sqlx::query_as(&sql)
            .bind(id.to_string())
            .fetch_optional(&self.pool)
            .await
            .map_err(map_sqlx_error)?;

        Ok(row.map(ReminderRow::to_reminder))
    }

    #[instrument(skip(self))]
    async fn get_by_source_id(
        &self,
        source: ReminderSource,
        source_id: &str,
    ) -> Result<Option<Reminder>, ApplicationError> {
        let sql = format!("{SELECT_REMINDER} WHERE source = $1 AND source_id = $2");
        let row: Option<ReminderRow> = sqlx::query_as(&sql)
            .bind(source_to_str(source))
            .bind(source_id)
            .fetch_optional(&self.pool)
            .await
            .map_err(map_sqlx_error)?;

        Ok(row.map(ReminderRow::to_reminder))
    }

    #[instrument(skip(self, reminder), fields(reminder_id = %reminder.id))]
    async fn update(&self, reminder: &Reminder) -> Result<(), ApplicationError> {
        let result = sqlx::query(
            "UPDATE reminders SET
                title = $1, description = $2, event_time = $3,
                remind_at = $4, location = $5, status = $6,
                snooze_count = $7, max_snooze = $8, updated_at = $9
             WHERE id = $10",
        )
        .bind(&reminder.title)
        .bind(&reminder.description)
        .bind(reminder.event_time.map(|t| t.to_rfc3339()))
        .bind(reminder.remind_at.to_rfc3339())
        .bind(&reminder.location)
        .bind(status_to_str(reminder.status))
        .bind(i32::from(reminder.snooze_count))
        .bind(i32::from(reminder.max_snooze))
        .bind(reminder.updated_at.to_rfc3339())
        .bind(reminder.id.to_string())
        .execute(&self.pool)
        .await
        .map_err(map_sqlx_error)?;

        if result.rows_affected() == 0 {
            return Err(ApplicationError::NotFound(format!(
                "Reminder {} not found",
                reminder.id
            )));
        }

        debug!("Updated reminder");
        Ok(())
    }

    #[instrument(skip(self), fields(reminder_id = %id))]
    async fn delete(&self, id: &ReminderId) -> Result<(), ApplicationError> {
        sqlx::query("DELETE FROM reminders WHERE id = $1")
            .bind(id.to_string())
            .execute(&self.pool)
            .await
            .map_err(map_sqlx_error)?;

        debug!("Deleted reminder");
        Ok(())
    }

    #[instrument(skip(self))]
    async fn query(&self, query: &ReminderQuery) -> Result<Vec<Reminder>, ApplicationError> {
        let mut sql = format!("{SELECT_REMINDER} WHERE 1=1");
        let mut binds: Vec<String> = Vec::new();

        if let Some(ref user_id) = query.user_id {
            binds.push(user_id.to_string());
            sql.push_str(&format!(" AND user_id = ${}", binds.len()));
        }

        if let Some(status) = query.status {
            binds.push(status_to_str(status).to_string());
            sql.push_str(&format!(" AND status = ${}", binds.len()));
        }

        if let Some(source) = query.source {
            binds.push(source_to_str(source).to_string());
            sql.push_str(&format!(" AND source = ${}", binds.len()));
        }

        if let Some(ref due_before) = query.due_before {
            binds.push(due_before.to_rfc3339());
            sql.push_str(&format!(" AND remind_at <= ${}", binds.len()));
        }

        if !query.include_terminal {
            sql.push_str(" AND status IN ('pending', 'sent', 'snoozed')");
        }

        sql.push_str(" ORDER BY remind_at ASC");

        if let Some(limit) = query.limit {
            binds.push(limit.to_string());
            sql.push_str(&format!(" LIMIT ${}", binds.len()));
        }

        let mut q = sqlx::query_as::<_, ReminderRow>(&sql);
        for b in &binds {
            q = q.bind(b);
        }

        let rows: Vec<ReminderRow> = q.fetch_all(&self.pool).await.map_err(map_sqlx_error)?;
        Ok(rows.into_iter().map(ReminderRow::to_reminder).collect())
    }

    #[instrument(skip(self))]
    async fn get_due_reminders(&self) -> Result<Vec<Reminder>, ApplicationError> {
        let sql = format!(
            "{SELECT_REMINDER} WHERE status IN ('pending', 'snoozed') \
             AND remind_at <= $1 ORDER BY remind_at ASC"
        );

        let rows: Vec<ReminderRow> = sqlx::query_as(&sql)
            .bind(Utc::now().to_rfc3339())
            .fetch_all(&self.pool)
            .await
            .map_err(map_sqlx_error)?;

        debug!(count = rows.len(), "Fetched due reminders");
        Ok(rows.into_iter().map(ReminderRow::to_reminder).collect())
    }

    #[instrument(skip(self))]
    async fn count_active(&self, user_id: &UserId) -> Result<u64, ApplicationError> {
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM reminders
             WHERE user_id = $1
               AND status IN ('pending', 'sent', 'snoozed')",
        )
        .bind(user_id.to_string())
        .fetch_one(&self.pool)
        .await
        .map_err(map_sqlx_error)?;

        #[allow(clippy::cast_sign_loss)]
        Ok(count as u64)
    }

    #[instrument(skip(self))]
    async fn cleanup_old(&self, older_than: DateTime<Utc>) -> Result<u64, ApplicationError> {
        let result = sqlx::query(
            "DELETE FROM reminders
             WHERE status IN ('acknowledged', 'cancelled', 'expired')
               AND updated_at < $1",
        )
        .bind(older_than.to_rfc3339())
        .execute(&self.pool)
        .await
        .map_err(map_sqlx_error)?;

        let deleted = result.rows_affected();
        debug!(deleted, "Cleaned up old reminders");
        Ok(deleted)
    }
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
#[allow(clippy::match_same_arms)]
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

#[cfg(test)]
mod tests {
    use chrono::Duration;

    use super::*;
    use crate::persistence::async_connection::AsyncDatabase;

    async fn setup() -> (AsyncDatabase, SqliteReminderStore) {
        let db = AsyncDatabase::in_memory().await.unwrap();
        db.migrate().await.unwrap();
        let store = SqliteReminderStore::new(db.pool().clone());
        (db, store)
    }

    fn test_user_id() -> UserId {
        UserId::new()
    }

    #[tokio::test]
    async fn save_and_get_reminder() {
        let (_db, store) = setup().await;
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
        let (_db, store) = setup().await;
        let result = store.get(&ReminderId::new()).await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn save_with_location_and_event_time() {
        let (_db, store) = setup().await;
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
        let (_db, store) = setup().await;
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

        let not_found = store
            .get_by_source_id(ReminderSource::Custom, "uid-abc-1h")
            .await
            .unwrap();
        assert!(not_found.is_none());
    }

    #[tokio::test]
    async fn update_reminder() {
        let (_db, store) = setup().await;
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
        let (_db, store) = setup().await;
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
        let (_db, store) = setup().await;
        let user = test_user_id();
        let other_user = test_user_id();

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
        let (_db, store) = setup().await;
        let user = test_user_id();

        let r1 = Reminder::new(
            user,
            ReminderSource::Custom,
            "Due now",
            Utc::now() - Duration::minutes(5),
        );
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
        let (_db, store) = setup().await;
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
        let (_db, store) = setup().await;
        let user = test_user_id();

        let mut r1 = Reminder::new(
            user,
            ReminderSource::Custom,
            "Old done",
            Utc::now() - Duration::days(35),
        );
        r1.acknowledge();
        r1.updated_at = Utc::now() - Duration::days(35);
        store.save(&r1).await.unwrap();
        store.update(&r1).await.unwrap();

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

        let active = store.get(&r2.id).await.unwrap();
        assert!(active.is_some());
    }

    #[tokio::test]
    async fn snooze_lifecycle() {
        let (_db, store) = setup().await;
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

    #[test]
    fn sqlite_reminder_store_is_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<SqliteReminderStore>();
    }
}
