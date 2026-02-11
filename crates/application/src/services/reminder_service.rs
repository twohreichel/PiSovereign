//! Reminder service
//!
//! Business logic for reminder management including creation from calendar
//! events, custom reminders, snooze/acknowledge lifecycle, and due reminder
//! polling.

use std::{fmt, sync::Arc};

use chrono::{DateTime, Duration, Utc};
use domain::entities::{Reminder, ReminderSource};
use domain::value_objects::{ReminderId, UserId};
use tracing::{debug, info, instrument, warn};

use crate::{
    error::ApplicationError,
    ports::{CalendarEvent, CalendarPort, ReminderPort, ReminderQuery},
};

/// Configuration for the reminder service
#[derive(Debug, Clone)]
pub struct ReminderServiceConfig {
    /// Minutes before event for the first reminder (default: 60)
    pub first_reminder_minutes: i64,
    /// Minutes before event for the second reminder (default: 15)
    pub second_reminder_minutes: i64,
    /// Maximum number of snoozes allowed (default: 3)
    pub max_snooze: u8,
    /// Default snooze duration in minutes (default: 15)
    pub default_snooze_minutes: i64,
    /// Age in days after which terminal reminders are cleaned up (default: 30)
    pub cleanup_days: i64,
}

impl Default for ReminderServiceConfig {
    fn default() -> Self {
        Self {
            first_reminder_minutes: 60,
            second_reminder_minutes: 15,
            max_snooze: 3,
            default_snooze_minutes: 15,
            cleanup_days: 30,
        }
    }
}

/// Service for managing reminders
pub struct ReminderService<R: ReminderPort> {
    reminder_store: Arc<R>,
    calendar_port: Option<Arc<dyn CalendarPort>>,
    config: ReminderServiceConfig,
}

impl<R: ReminderPort> fmt::Debug for ReminderService<R> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ReminderService")
            .field("has_calendar", &self.calendar_port.is_some())
            .finish_non_exhaustive()
    }
}

impl<R: ReminderPort> Clone for ReminderService<R> {
    fn clone(&self) -> Self {
        Self {
            reminder_store: Arc::clone(&self.reminder_store),
            calendar_port: self.calendar_port.as_ref().map(Arc::clone),
            config: self.config.clone(),
        }
    }
}

impl<R: ReminderPort> ReminderService<R> {
    /// Create a new reminder service
    #[must_use]
    pub fn new(reminder_store: Arc<R>, config: ReminderServiceConfig) -> Self {
        Self {
            reminder_store,
            calendar_port: None,
            config,
        }
    }

    /// Attach a calendar port for calendar-based reminders
    #[must_use]
    pub fn with_calendar(mut self, calendar_port: Arc<dyn CalendarPort>) -> Self {
        self.calendar_port = Some(calendar_port);
        self
    }

    /// Create a custom reminder
    #[instrument(skip(self))]
    pub async fn create_custom_reminder(
        &self,
        user_id: UserId,
        title: &str,
        remind_at: DateTime<Utc>,
        description: Option<&str>,
    ) -> Result<Reminder, ApplicationError> {
        info!(%title, %remind_at, "Creating custom reminder");

        if remind_at <= Utc::now() {
            return Err(ApplicationError::InvalidOperation(
                "Reminder time must be in the future".to_string(),
            ));
        }

        let mut reminder = Reminder::new(user_id, ReminderSource::Custom, title, remind_at)
            .with_max_snooze(self.config.max_snooze);

        if let Some(desc) = description {
            reminder = reminder.with_description(desc);
        }

        self.reminder_store.save(&reminder).await?;

        debug!(id = %reminder.id, "Custom reminder created");
        Ok(reminder)
    }

    /// Create a reminder from a calendar event
    #[instrument(skip(self, event))]
    pub async fn create_from_calendar_event(
        &self,
        user_id: UserId,
        event: &CalendarEvent,
    ) -> Result<Vec<Reminder>, ApplicationError> {
        info!(event_id = %event.id, title = %event.title, "Creating reminders from calendar event");

        let mut created = Vec::new();

        // Check for existing reminder with this source ID
        let existing = self
            .reminder_store
            .get_by_source_id(ReminderSource::CalendarEvent, &event.id)
            .await?;

        if existing.is_some() {
            debug!(event_id = %event.id, "Reminder already exists for event, skipping");
            return Ok(created);
        }

        // Parse event start time
        let event_time: DateTime<Utc> = event.start.parse().map_err(|e| {
            ApplicationError::Internal(format!(
                "Cannot parse event start time '{}': {e}",
                event.start
            ))
        })?;

        // Create first reminder (1 hour before)
        let first_remind_at = event_time - Duration::minutes(self.config.first_reminder_minutes);
        if first_remind_at > Utc::now() {
            let mut reminder = Reminder::new(
                user_id,
                ReminderSource::CalendarEvent,
                &event.title,
                first_remind_at,
            )
            .with_source_id(format!("{}-1h", event.id))
            .with_event_time(event_time)
            .with_max_snooze(self.config.max_snooze);

            if let Some(ref loc) = event.location {
                reminder = reminder.with_location(loc);
            }
            if let Some(ref desc) = event.description {
                reminder = reminder.with_description(desc);
            }

            self.reminder_store.save(&reminder).await?;
            created.push(reminder);
        }

        // Create second reminder (15 minutes before)
        let second_remind_at = event_time - Duration::minutes(self.config.second_reminder_minutes);
        if second_remind_at > Utc::now() {
            let mut reminder = Reminder::new(
                user_id,
                ReminderSource::CalendarEvent,
                &event.title,
                second_remind_at,
            )
            .with_source_id(format!("{}-15m", event.id))
            .with_event_time(event_time)
            .with_max_snooze(self.config.max_snooze);

            if let Some(ref loc) = event.location {
                reminder = reminder.with_location(loc);
            }

            self.reminder_store.save(&reminder).await?;
            created.push(reminder);
        }

        debug!(count = created.len(), event_id = %event.id, "Reminders created from event");
        Ok(created)
    }

    /// Get all active reminders for a user
    #[instrument(skip(self))]
    pub async fn list_active(&self, user_id: UserId) -> Result<Vec<Reminder>, ApplicationError> {
        let query = ReminderQuery::active_for_user(user_id);
        self.reminder_store.query(&query).await
    }

    /// Get all reminders for a user (including done/cancelled)
    #[instrument(skip(self))]
    pub async fn list_all(&self, user_id: UserId) -> Result<Vec<Reminder>, ApplicationError> {
        let query = ReminderQuery {
            user_id: Some(user_id),
            include_terminal: true,
            ..Default::default()
        };
        self.reminder_store.query(&query).await
    }

    /// Get all due reminders (ready to fire)
    #[instrument(skip(self))]
    pub async fn get_due_reminders(&self) -> Result<Vec<Reminder>, ApplicationError> {
        self.reminder_store.get_due_reminders().await
    }

    /// Snooze a reminder
    #[instrument(skip(self))]
    pub async fn snooze(
        &self,
        reminder_id: &ReminderId,
        duration_minutes: Option<i64>,
    ) -> Result<Reminder, ApplicationError> {
        let mut reminder = self.reminder_store.get(reminder_id).await?.ok_or_else(|| {
            ApplicationError::NotFound(format!("Reminder {reminder_id} not found"))
        })?;

        let snooze_minutes = duration_minutes.unwrap_or(self.config.default_snooze_minutes);
        let new_time = Utc::now() + Duration::minutes(snooze_minutes);

        if !reminder.snooze(new_time) {
            return Err(ApplicationError::InvalidOperation(format!(
                "Cannot snooze reminder (max {} snoozes reached)",
                self.config.max_snooze
            )));
        }

        self.reminder_store.update(&reminder).await?;
        info!(id = %reminder_id, snooze_until = %new_time, "Reminder snoozed");
        Ok(reminder)
    }

    /// Acknowledge a reminder (mark as done)
    #[instrument(skip(self))]
    pub async fn acknowledge(
        &self,
        reminder_id: &ReminderId,
    ) -> Result<Reminder, ApplicationError> {
        let mut reminder = self.reminder_store.get(reminder_id).await?.ok_or_else(|| {
            ApplicationError::NotFound(format!("Reminder {reminder_id} not found"))
        })?;

        reminder.acknowledge();
        self.reminder_store.update(&reminder).await?;
        info!(id = %reminder_id, "Reminder acknowledged");
        Ok(reminder)
    }

    /// Mark a sent reminder as sent
    #[instrument(skip(self))]
    pub async fn mark_sent(&self, reminder_id: &ReminderId) -> Result<(), ApplicationError> {
        let mut reminder = self.reminder_store.get(reminder_id).await?.ok_or_else(|| {
            ApplicationError::NotFound(format!("Reminder {reminder_id} not found"))
        })?;

        reminder.mark_sent();
        self.reminder_store.update(&reminder).await?;
        debug!(id = %reminder_id, "Reminder marked as sent");
        Ok(())
    }

    /// Delete/cancel a reminder
    #[instrument(skip(self))]
    pub async fn delete(&self, reminder_id: &ReminderId) -> Result<(), ApplicationError> {
        let mut reminder = self.reminder_store.get(reminder_id).await?.ok_or_else(|| {
            ApplicationError::NotFound(format!("Reminder {reminder_id} not found"))
        })?;

        reminder.cancel();
        self.reminder_store.update(&reminder).await?;
        info!(id = %reminder_id, "Reminder cancelled");
        Ok(())
    }

    /// Sync reminders from calendar events
    ///
    /// Fetches upcoming events and creates reminders for those without one.
    #[instrument(skip(self))]
    pub async fn sync_calendar_reminders(&self, user_id: UserId) -> Result<u32, ApplicationError> {
        let calendar = self.calendar_port.as_ref().ok_or_else(|| {
            ApplicationError::InvalidOperation("Calendar port not configured".to_string())
        })?;

        let events = calendar
            .get_today_events()
            .await
            .map_err(|e| ApplicationError::ExternalService(e.to_string()))?;

        let mut created_count = 0u32;
        for event in &events {
            match self.create_from_calendar_event(user_id, event).await {
                Ok(reminders) => {
                    created_count += u32::try_from(reminders.len()).unwrap_or(0);
                },
                Err(e) => {
                    warn!(event_id = %event.id, error = %e, "Failed to create reminder for event");
                },
            }
        }

        info!(created = created_count, "Calendar reminder sync complete");
        Ok(created_count)
    }

    /// Clean up old terminal reminders
    #[instrument(skip(self))]
    pub async fn cleanup_old_reminders(&self) -> Result<u64, ApplicationError> {
        let threshold = Utc::now() - Duration::days(self.config.cleanup_days);
        let deleted = self.reminder_store.cleanup_old(threshold).await?;
        info!(deleted, "Cleaned up old reminders");
        Ok(deleted)
    }

    /// Count active reminders for a user
    pub async fn count_active(&self, user_id: &UserId) -> Result<u64, ApplicationError> {
        self.reminder_store.count_active(user_id).await
    }
}

#[cfg(test)]
mod tests {
    use domain::entities::ReminderStatus;

    use super::*;
    use crate::ports::MockReminderPort;

    fn default_config() -> ReminderServiceConfig {
        ReminderServiceConfig::default()
    }

    fn user_id() -> UserId {
        UserId::new()
    }

    #[tokio::test]
    async fn create_custom_reminder_success() {
        let mut mock = MockReminderPort::new();
        mock.expect_save().times(1).returning(|_| Ok(()));

        let service = ReminderService::new(Arc::new(mock), default_config());
        let remind_at = Utc::now() + Duration::hours(1);

        let result = service
            .create_custom_reminder(user_id(), "Buy groceries", remind_at, None)
            .await;

        assert!(result.is_ok());
        let reminder = result.unwrap();
        assert_eq!(reminder.title, "Buy groceries");
        assert_eq!(reminder.source, ReminderSource::Custom);
        assert_eq!(reminder.status, ReminderStatus::Pending);
    }

    #[tokio::test]
    async fn create_custom_reminder_past_time_fails() {
        let mock = MockReminderPort::new();
        let service = ReminderService::new(Arc::new(mock), default_config());
        let remind_at = Utc::now() - Duration::hours(1);

        let result = service
            .create_custom_reminder(user_id(), "Too late", remind_at, None)
            .await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn snooze_reminder_success() {
        let mut mock = MockReminderPort::new();
        let reminder = Reminder::new(
            user_id(),
            ReminderSource::Custom,
            "Test",
            Utc::now() - Duration::minutes(5),
        );
        let rid = reminder.id;
        let reminder_clone = reminder.clone();

        mock.expect_get()
            .times(1)
            .returning(move |_| Ok(Some(reminder_clone.clone())));
        mock.expect_update().times(1).returning(|_| Ok(()));

        let service = ReminderService::new(Arc::new(mock), default_config());
        let result = service.snooze(&rid, Some(15)).await;

        assert!(result.is_ok());
        let snoozed = result.unwrap();
        assert_eq!(snoozed.status, ReminderStatus::Snoozed);
        assert_eq!(snoozed.snooze_count, 1);
    }

    #[tokio::test]
    async fn snooze_not_found() {
        let mut mock = MockReminderPort::new();
        mock.expect_get().times(1).returning(|_| Ok(None));

        let service = ReminderService::new(Arc::new(mock), default_config());
        let result = service.snooze(&ReminderId::new(), Some(15)).await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn acknowledge_reminder() {
        let mut mock = MockReminderPort::new();
        let reminder = Reminder::new(user_id(), ReminderSource::Custom, "Test", Utc::now());
        let rid = reminder.id;
        let reminder_clone = reminder.clone();

        mock.expect_get()
            .times(1)
            .returning(move |_| Ok(Some(reminder_clone.clone())));
        mock.expect_update().times(1).returning(|_| Ok(()));

        let service = ReminderService::new(Arc::new(mock), default_config());
        let result = service.acknowledge(&rid).await;

        assert!(result.is_ok());
        assert_eq!(result.unwrap().status, ReminderStatus::Acknowledged);
    }

    #[tokio::test]
    async fn delete_cancels_reminder() {
        let mut mock = MockReminderPort::new();
        let reminder = Reminder::new(user_id(), ReminderSource::Custom, "Test", Utc::now());
        let rid = reminder.id;
        let reminder_clone = reminder.clone();

        mock.expect_get()
            .times(1)
            .returning(move |_| Ok(Some(reminder_clone.clone())));
        mock.expect_update().times(1).returning(|_| Ok(()));

        let service = ReminderService::new(Arc::new(mock), default_config());
        let result = service.delete(&rid).await;

        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn list_active_delegates_to_store() {
        let mut mock = MockReminderPort::new();
        mock.expect_query().times(1).returning(|_| Ok(vec![]));

        let service = ReminderService::new(Arc::new(mock), default_config());
        let result = service.list_active(user_id()).await;

        assert!(result.is_ok());
        assert!(result.unwrap().is_empty());
    }

    #[tokio::test]
    async fn get_due_reminders_delegates() {
        let mut mock = MockReminderPort::new();
        mock.expect_get_due_reminders()
            .times(1)
            .returning(|| Ok(vec![]));

        let service = ReminderService::new(Arc::new(mock), default_config());
        let result = service.get_due_reminders().await;

        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn cleanup_old_reminders() {
        let mut mock = MockReminderPort::new();
        mock.expect_cleanup_old().times(1).returning(|_| Ok(5));

        let service = ReminderService::new(Arc::new(mock), default_config());
        let result = service.cleanup_old_reminders().await;

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 5);
    }

    #[test]
    fn config_defaults() {
        let config = ReminderServiceConfig::default();
        assert_eq!(config.first_reminder_minutes, 60);
        assert_eq!(config.second_reminder_minutes, 15);
        assert_eq!(config.max_snooze, 3);
        assert_eq!(config.default_snooze_minutes, 15);
        assert_eq!(config.cleanup_days, 30);
    }

    #[test]
    fn service_debug() {
        let mock = MockReminderPort::new();
        let service = ReminderService::new(Arc::new(mock), default_config());
        let debug = format!("{service:?}");
        assert!(debug.contains("ReminderService"));
    }
}
