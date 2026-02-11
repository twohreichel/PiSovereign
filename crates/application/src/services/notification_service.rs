//! Notification service for proactive reminder delivery
//!
//! Orchestrates the processing of due reminders: polls for due reminders,
//! formats them with optional transit connections, and prepares
//! notifications ready to send via any messenger.

use std::sync::Arc;

use domain::entities::{Reminder, ReminderSource};
use tracing::{debug, error, info, instrument, warn};

use crate::error::ApplicationError;
use crate::ports::{ReminderPort, TransitPort};
use crate::services::reminder_formatter;

/// A formatted notification ready to be sent
#[derive(Debug, Clone)]
pub struct ReminderNotification {
    /// The reminder that triggered this notification
    pub reminder: Reminder,
    /// The formatted message text
    pub message: String,
}

/// Configuration for the notification service
#[derive(Debug, Clone)]
pub struct NotificationConfig {
    /// Whether to include transit connections in event reminders
    pub include_transit: bool,
    /// Home location coordinates for transit routing (start point)
    pub home_latitude: Option<f64>,
    /// Home location longitude
    pub home_longitude: Option<f64>,
    /// Maximum number of transit options to show
    pub max_transit_options: u8,
}

impl Default for NotificationConfig {
    fn default() -> Self {
        Self {
            include_transit: true,
            home_latitude: None,
            home_longitude: None,
            max_transit_options: 3,
        }
    }
}

impl NotificationConfig {
    /// Check if home coordinates are configured
    #[must_use]
    pub const fn has_home_location(&self) -> bool {
        self.home_latitude.is_some() && self.home_longitude.is_some()
    }
}

/// Service that processes due reminders and prepares notifications
pub struct NotificationService<R: ReminderPort> {
    reminder_port: Arc<R>,
    transit_port: Option<Arc<dyn TransitPort>>,
    config: NotificationConfig,
}

impl<R: ReminderPort> std::fmt::Debug for NotificationService<R> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NotificationService")
            .field("config", &self.config)
            .field("has_transit", &self.transit_port.is_some())
            .finish_non_exhaustive()
    }
}
impl<R: ReminderPort> NotificationService<R> {
    /// Create a new notification service
    #[must_use]
    pub fn new(reminder_port: Arc<R>, config: NotificationConfig) -> Self {
        Self {
            reminder_port,
            transit_port: None,
            config,
        }
    }

    /// Attach a transit port for ÖPNV connections
    #[must_use]
    pub fn with_transit(mut self, transit_port: Arc<dyn TransitPort>) -> Self {
        self.transit_port = Some(transit_port);
        self
    }

    /// Process all due reminders and return formatted notifications
    ///
    /// This method:
    /// 1. Fetches all reminders that are due now
    /// 2. Formats each one with optional transit info
    /// 3. Marks each as sent
    /// 4. Returns the list of formatted notifications
    #[instrument(skip(self))]
    pub async fn process_due_reminders(
        &self,
    ) -> Result<Vec<ReminderNotification>, ApplicationError> {
        let due = self.reminder_port.get_due_reminders().await?;

        if due.is_empty() {
            debug!("No due reminders");
            return Ok(Vec::new());
        }

        info!(count = due.len(), "Processing due reminders");
        let mut notifications = Vec::with_capacity(due.len());

        for mut reminder in due {
            match self.format_notification(&reminder).await {
                Ok(message) => {
                    reminder.mark_sent();
                    if let Err(e) = self.reminder_port.update(&reminder).await {
                        error!(
                            reminder_id = %reminder.id,
                            error = %e,
                            "Failed to mark reminder as sent"
                        );
                        continue;
                    }

                    notifications.push(ReminderNotification {
                        reminder: reminder.clone(),
                        message,
                    });
                },
                Err(e) => {
                    warn!(
                        reminder_id = %reminder.id,
                        error = %e,
                        "Failed to format reminder notification"
                    );
                },
            }
        }

        info!(sent = notifications.len(), "Processed due reminders");
        Ok(notifications)
    }

    /// Format a single reminder into a notification message
    async fn format_notification(&self, reminder: &Reminder) -> Result<String, ApplicationError> {
        // Only fetch transit for calendar events with a location
        let transit_connections = if self.should_fetch_transit(reminder) {
            self.fetch_transit_connections(reminder).await
        } else {
            None
        };

        Ok(reminder_formatter::format_reminder(
            reminder,
            transit_connections.as_deref(),
        ))
    }

    /// Check whether we should fetch transit connections for this reminder
    fn should_fetch_transit(&self, reminder: &Reminder) -> bool {
        self.config.include_transit
            && self.transit_port.is_some()
            && self.config.has_home_location()
            && reminder.source == ReminderSource::CalendarEvent
            && reminder.location.is_some()
            && reminder.event_time.is_some()
    }

    /// Fetch transit connections from home to the reminder location
    async fn fetch_transit_connections(
        &self,
        reminder: &Reminder,
    ) -> Option<Vec<crate::ports::TransitConnection>> {
        let transit = self.transit_port.as_ref()?;
        let lat = self.config.home_latitude?;
        let lon = self.config.home_longitude?;
        let location = reminder.location.as_ref()?;
        let event_time = reminder.event_time?;

        let home = domain::value_objects::GeoLocation::new(lat, lon).ok()?;

        // Aim to arrive ~30 min before the event
        let query_departure = event_time - chrono::Duration::minutes(30);

        match transit
            .find_connections_to_address(
                &home,
                location,
                Some(query_departure),
                self.config.max_transit_options,
            )
            .await
        {
            Ok(connections) => {
                if connections.is_empty() {
                    None
                } else {
                    Some(connections)
                }
            },
            Err(e) => {
                warn!(
                    reminder_id = %reminder.id,
                    error = %e,
                    "Failed to fetch transit connections"
                );
                None
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use chrono::Utc;
    use domain::value_objects::UserId;

    use super::*;
    use crate::ports::{MockReminderPort, MockTransitPort};

    fn make_due_reminder(title: &str) -> Reminder {
        Reminder::new(
            UserId::new(),
            ReminderSource::Custom,
            title,
            Utc::now() - chrono::Duration::minutes(1),
        )
    }

    fn make_event_reminder(title: &str, location: &str) -> Reminder {
        let event_time = Utc::now() + chrono::Duration::hours(1);
        Reminder::new(
            UserId::new(),
            ReminderSource::CalendarEvent,
            title,
            Utc::now() - chrono::Duration::minutes(5),
        )
        .with_event_time(event_time)
        .with_location(location)
    }

    #[tokio::test]
    async fn process_no_due_reminders() {
        let mut mock_port = MockReminderPort::new();
        mock_port
            .expect_get_due_reminders()
            .returning(|| Ok(vec![]));

        let service = NotificationService::new(Arc::new(mock_port), NotificationConfig::default());

        let result = service.process_due_reminders().await.unwrap();
        assert!(result.is_empty());
    }

    #[tokio::test]
    async fn process_custom_reminder() {
        let reminder = make_due_reminder("Buy groceries");
        let reminder_clone = reminder.clone();

        let mut mock_port = MockReminderPort::new();
        mock_port
            .expect_get_due_reminders()
            .returning(move || Ok(vec![reminder_clone.clone()]));
        mock_port.expect_update().returning(|_| Ok(()));

        let service = NotificationService::new(Arc::new(mock_port), NotificationConfig::default());

        let result = service.process_due_reminders().await.unwrap();
        assert_eq!(result.len(), 1);
        assert!(result[0].message.contains("Buy groceries"));
        assert!(result[0].message.contains("⏰"));
    }

    #[tokio::test]
    async fn process_event_reminder_without_transit() {
        let reminder = make_event_reminder("Meeting", "TU Berlin");
        let reminder_clone = reminder.clone();

        let mut mock_port = MockReminderPort::new();
        mock_port
            .expect_get_due_reminders()
            .returning(move || Ok(vec![reminder_clone.clone()]));
        mock_port.expect_update().returning(|_| Ok(()));

        // No transit port configured
        let config = NotificationConfig {
            include_transit: false,
            ..Default::default()
        };
        let service = NotificationService::new(Arc::new(mock_port), config);

        let result = service.process_due_reminders().await.unwrap();
        assert_eq!(result.len(), 1);
        assert!(result[0].message.contains("Meeting"));
        assert!(result[0].message.contains("📍 TU Berlin"));
        assert!(!result[0].message.contains("ÖPNV"));
    }

    #[tokio::test]
    async fn process_multiple_reminders() {
        let r1 = make_due_reminder("Task 1");
        let r2 = make_due_reminder("Task 2");
        let r1_clone = r1.clone();
        let r2_clone = r2.clone();

        let mut mock_port = MockReminderPort::new();
        mock_port
            .expect_get_due_reminders()
            .returning(move || Ok(vec![r1_clone.clone(), r2_clone.clone()]));
        mock_port.expect_update().times(2).returning(|_| Ok(()));

        let service = NotificationService::new(Arc::new(mock_port), NotificationConfig::default());

        let result = service.process_due_reminders().await.unwrap();
        assert_eq!(result.len(), 2);
    }

    #[tokio::test]
    async fn should_not_fetch_transit_for_custom() {
        let reminder = make_due_reminder("Custom task");
        let config = NotificationConfig {
            include_transit: true,
            home_latitude: Some(52.52),
            home_longitude: Some(13.41),
            ..Default::default()
        };
        let service = NotificationService::new(Arc::new(MockReminderPort::new()), config);

        assert!(!service.should_fetch_transit(&reminder));
    }

    #[tokio::test]
    async fn should_fetch_transit_for_event_with_location() {
        let reminder = make_event_reminder("Meeting", "TU Berlin");
        let config = NotificationConfig {
            include_transit: true,
            home_latitude: Some(52.52),
            home_longitude: Some(13.41),
            ..Default::default()
        };
        let mock_transit = MockTransitPort::new();
        let service = NotificationService::new(Arc::new(MockReminderPort::new()), config)
            .with_transit(Arc::new(mock_transit));

        assert!(service.should_fetch_transit(&reminder));
    }

    #[tokio::test]
    async fn should_not_fetch_transit_when_disabled() {
        let reminder = make_event_reminder("Meeting", "TU Berlin");
        let config = NotificationConfig {
            include_transit: false,
            home_latitude: Some(52.52),
            home_longitude: Some(13.41),
            ..Default::default()
        };
        let service = NotificationService::new(Arc::new(MockReminderPort::new()), config);

        assert!(!service.should_fetch_transit(&reminder));
    }

    #[tokio::test]
    async fn should_not_fetch_transit_without_home() {
        let reminder = make_event_reminder("Meeting", "TU Berlin");
        let config = NotificationConfig {
            include_transit: true,
            home_latitude: None,
            home_longitude: None,
            ..Default::default()
        };
        let mock_transit = MockTransitPort::new();
        let service = NotificationService::new(Arc::new(MockReminderPort::new()), config)
            .with_transit(Arc::new(mock_transit));

        assert!(!service.should_fetch_transit(&reminder));
    }
}
