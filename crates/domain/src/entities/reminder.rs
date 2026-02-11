//! Reminder entity - Proactive notifications for calendar events, tasks, and custom alerts

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::value_objects::{ReminderId, UserId};

/// Source of the reminder (what triggered its creation)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReminderSource {
    /// Synced from a CalDAV calendar event
    CalendarEvent,
    /// Synced from a CalDAV task/todo
    CalendarTask,
    /// Manually created by user via command
    Custom,
}

impl ReminderSource {
    /// Get a human-readable label
    #[must_use]
    pub const fn label(&self) -> &'static str {
        match self {
            Self::CalendarEvent => "Calendar Event",
            Self::CalendarTask => "Task",
            Self::Custom => "Custom",
        }
    }
}

impl std::fmt::Display for ReminderSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.label())
    }
}

/// Status of a reminder
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReminderStatus {
    /// Pending - not yet fired
    Pending,
    /// Sent - notification was delivered
    Sent,
    /// Acknowledged - user marked as done
    Acknowledged,
    /// Snoozed - rescheduled for later
    Snoozed,
    /// Cancelled - user cancelled the reminder
    Cancelled,
    /// Expired - reminder time passed without delivery
    Expired,
}

impl ReminderStatus {
    /// Check if this status is terminal (no further transitions)
    #[must_use]
    pub const fn is_terminal(&self) -> bool {
        matches!(self, Self::Acknowledged | Self::Cancelled | Self::Expired)
    }

    /// Check if this reminder is still actionable
    #[must_use]
    pub const fn is_active(&self) -> bool {
        matches!(self, Self::Pending | Self::Snoozed)
    }

    /// Get a human-readable label
    #[must_use]
    pub const fn label(&self) -> &'static str {
        match self {
            Self::Pending => "Pending",
            Self::Sent => "Sent",
            Self::Acknowledged => "Done",
            Self::Snoozed => "Snoozed",
            Self::Cancelled => "Cancelled",
            Self::Expired => "Expired",
        }
    }
}

impl std::fmt::Display for ReminderStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.label())
    }
}

/// A proactive reminder notification
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Reminder {
    /// Unique identifier
    pub id: ReminderId,
    /// User who owns this reminder
    pub user_id: UserId,
    /// What triggered this reminder
    pub source: ReminderSource,
    /// External source ID (CalDAV UID, etc.) for deduplication
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_id: Option<String>,
    /// Short title/summary
    pub title: String,
    /// Optional detailed description
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// When the actual event/task occurs
    #[serde(skip_serializing_if = "Option::is_none")]
    pub event_time: Option<DateTime<Utc>>,
    /// When the reminder notification should fire
    pub remind_at: DateTime<Utc>,
    /// Location of the event (free-form address)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub location: Option<String>,
    /// Current status
    pub status: ReminderStatus,
    /// Number of times this reminder has been snoozed
    pub snooze_count: u8,
    /// Maximum allowed snooze count
    pub max_snooze: u8,
    /// When this reminder was created
    pub created_at: DateTime<Utc>,
    /// When this reminder was last updated
    pub updated_at: DateTime<Utc>,
}

impl Reminder {
    /// Create a new pending reminder
    #[must_use]
    pub fn new(
        user_id: UserId,
        source: ReminderSource,
        title: impl Into<String>,
        remind_at: DateTime<Utc>,
    ) -> Self {
        let now = Utc::now();
        Self {
            id: ReminderId::new(),
            user_id,
            source,
            source_id: None,
            title: title.into(),
            description: None,
            event_time: None,
            remind_at,
            location: None,
            status: ReminderStatus::Pending,
            snooze_count: 0,
            max_snooze: 3,
            created_at: now,
            updated_at: now,
        }
    }

    /// Set the external source ID for deduplication
    #[must_use]
    pub fn with_source_id(mut self, source_id: impl Into<String>) -> Self {
        self.source_id = Some(source_id.into());
        self
    }

    /// Set the event time (when the actual event/task occurs)
    #[must_use]
    pub const fn with_event_time(mut self, event_time: DateTime<Utc>) -> Self {
        self.event_time = Some(event_time);
        self
    }

    /// Set a description
    #[must_use]
    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    /// Set a location
    #[must_use]
    pub fn with_location(mut self, location: impl Into<String>) -> Self {
        self.location = Some(location.into());
        self
    }

    /// Set max snooze count
    #[must_use]
    pub const fn with_max_snooze(mut self, max: u8) -> Self {
        self.max_snooze = max;
        self
    }

    /// Check if this reminder is due (ready to fire)
    #[must_use]
    pub fn is_due(&self) -> bool {
        self.status.is_active() && Utc::now() >= self.remind_at
    }

    /// Check if this reminder can be snoozed
    #[must_use]
    pub const fn can_snooze(&self) -> bool {
        self.snooze_count < self.max_snooze && !self.status.is_terminal()
    }

    /// Mark this reminder as sent
    pub fn mark_sent(&mut self) {
        self.status = ReminderStatus::Sent;
        self.updated_at = Utc::now();
    }

    /// Acknowledge this reminder (user marked as done)
    pub fn acknowledge(&mut self) {
        self.status = ReminderStatus::Acknowledged;
        self.updated_at = Utc::now();
    }

    /// Cancel this reminder
    pub fn cancel(&mut self) {
        self.status = ReminderStatus::Cancelled;
        self.updated_at = Utc::now();
    }

    /// Mark as expired
    pub fn expire(&mut self) {
        self.status = ReminderStatus::Expired;
        self.updated_at = Utc::now();
    }

    /// Snooze the reminder by a given duration
    ///
    /// Returns `true` if the snooze was successful, `false` if max snooze reached.
    pub fn snooze(&mut self, new_remind_at: DateTime<Utc>) -> bool {
        if !self.can_snooze() {
            return false;
        }
        self.snooze_count += 1;
        self.remind_at = new_remind_at;
        self.status = ReminderStatus::Snoozed;
        self.updated_at = Utc::now();
        true
    }

    /// Minutes until the event (if event_time is set)
    #[must_use]
    pub fn minutes_until_event(&self) -> Option<i64> {
        self.event_time.map(|et| (et - Utc::now()).num_minutes())
    }

    /// Check if the event has a location
    #[must_use]
    pub fn has_location(&self) -> bool {
        self.location.as_ref().is_some_and(|l| !l.trim().is_empty())
    }
}

impl std::fmt::Display for Reminder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[{}] {} ({})", self.source, self.title, self.status)
    }
}

#[cfg(test)]
mod tests {
    use chrono::Duration;

    use super::*;

    fn sample_user_id() -> UserId {
        UserId::new()
    }

    #[test]
    fn new_reminder_is_pending() {
        let reminder = Reminder::new(
            sample_user_id(),
            ReminderSource::CalendarEvent,
            "Team standup",
            Utc::now() + Duration::hours(1),
        );
        assert_eq!(reminder.status, ReminderStatus::Pending);
        assert_eq!(reminder.snooze_count, 0);
        assert_eq!(reminder.max_snooze, 3);
        assert!(reminder.source_id.is_none());
    }

    #[test]
    fn builder_methods() {
        let event_time = Utc::now() + Duration::hours(2);
        let reminder = Reminder::new(
            sample_user_id(),
            ReminderSource::CalendarEvent,
            "Meeting",
            Utc::now() + Duration::hours(1),
        )
        .with_source_id("caldav-uid-123")
        .with_event_time(event_time)
        .with_description("Weekly sync")
        .with_location("Room 42")
        .with_max_snooze(5);

        assert_eq!(reminder.source_id.as_deref(), Some("caldav-uid-123"));
        assert_eq!(reminder.event_time, Some(event_time));
        assert_eq!(reminder.description.as_deref(), Some("Weekly sync"));
        assert_eq!(reminder.location.as_deref(), Some("Room 42"));
        assert_eq!(reminder.max_snooze, 5);
    }

    #[test]
    fn is_due_when_past_remind_at() {
        let reminder = Reminder::new(
            sample_user_id(),
            ReminderSource::Custom,
            "Past due",
            Utc::now() - Duration::minutes(5),
        );
        assert!(reminder.is_due());
    }

    #[test]
    fn not_due_when_future() {
        let reminder = Reminder::new(
            sample_user_id(),
            ReminderSource::Custom,
            "Future",
            Utc::now() + Duration::hours(1),
        );
        assert!(!reminder.is_due());
    }

    #[test]
    fn mark_sent() {
        let mut reminder =
            Reminder::new(sample_user_id(), ReminderSource::Custom, "Test", Utc::now());
        reminder.mark_sent();
        assert_eq!(reminder.status, ReminderStatus::Sent);
    }

    #[test]
    fn acknowledge() {
        let mut reminder =
            Reminder::new(sample_user_id(), ReminderSource::Custom, "Test", Utc::now());
        reminder.acknowledge();
        assert_eq!(reminder.status, ReminderStatus::Acknowledged);
        assert!(reminder.status.is_terminal());
    }

    #[test]
    fn cancel() {
        let mut reminder =
            Reminder::new(sample_user_id(), ReminderSource::Custom, "Test", Utc::now());
        reminder.cancel();
        assert_eq!(reminder.status, ReminderStatus::Cancelled);
        assert!(reminder.status.is_terminal());
    }

    #[test]
    fn expire() {
        let mut reminder =
            Reminder::new(sample_user_id(), ReminderSource::Custom, "Test", Utc::now());
        reminder.expire();
        assert_eq!(reminder.status, ReminderStatus::Expired);
        assert!(reminder.status.is_terminal());
    }

    #[test]
    fn snooze_success() {
        let mut reminder = Reminder::new(
            sample_user_id(),
            ReminderSource::Custom,
            "Snooze me",
            Utc::now(),
        );
        let new_time = Utc::now() + Duration::minutes(15);
        assert!(reminder.snooze(new_time));
        assert_eq!(reminder.status, ReminderStatus::Snoozed);
        assert_eq!(reminder.snooze_count, 1);
        assert_eq!(reminder.remind_at, new_time);
    }

    #[test]
    fn snooze_respects_max() {
        let mut reminder = Reminder::new(
            sample_user_id(),
            ReminderSource::Custom,
            "Snooze limit",
            Utc::now(),
        )
        .with_max_snooze(2);

        let t1 = Utc::now() + Duration::minutes(15);
        let t2 = Utc::now() + Duration::minutes(30);
        let t3 = Utc::now() + Duration::minutes(45);

        assert!(reminder.snooze(t1));
        assert!(reminder.snooze(t2));
        assert!(!reminder.snooze(t3)); // exceeds max
        assert_eq!(reminder.snooze_count, 2);
    }

    #[test]
    fn cannot_snooze_terminal_status() {
        let mut reminder =
            Reminder::new(sample_user_id(), ReminderSource::Custom, "Done", Utc::now());
        reminder.acknowledge();
        let new_time = Utc::now() + Duration::minutes(15);
        assert!(!reminder.snooze(new_time));
    }

    #[test]
    fn has_location() {
        let with_loc = Reminder::new(
            sample_user_id(),
            ReminderSource::CalendarEvent,
            "Meeting",
            Utc::now(),
        )
        .with_location("Berlin Hbf");
        assert!(with_loc.has_location());

        let without = Reminder::new(sample_user_id(), ReminderSource::Custom, "Call", Utc::now());
        assert!(!without.has_location());

        let empty = Reminder::new(
            sample_user_id(),
            ReminderSource::Custom,
            "Empty",
            Utc::now(),
        )
        .with_location("  ");
        assert!(!empty.has_location());
    }

    #[test]
    fn status_is_active() {
        assert!(ReminderStatus::Pending.is_active());
        assert!(ReminderStatus::Snoozed.is_active());
        assert!(!ReminderStatus::Sent.is_active());
        assert!(!ReminderStatus::Acknowledged.is_active());
        assert!(!ReminderStatus::Cancelled.is_active());
        assert!(!ReminderStatus::Expired.is_active());
    }

    #[test]
    fn source_display() {
        assert_eq!(ReminderSource::CalendarEvent.to_string(), "Calendar Event");
        assert_eq!(ReminderSource::CalendarTask.to_string(), "Task");
        assert_eq!(ReminderSource::Custom.to_string(), "Custom");
    }

    #[test]
    fn status_display() {
        assert_eq!(ReminderStatus::Pending.to_string(), "Pending");
        assert_eq!(ReminderStatus::Snoozed.to_string(), "Snoozed");
        assert_eq!(ReminderStatus::Acknowledged.to_string(), "Done");
    }

    #[test]
    fn display_format() {
        let reminder = Reminder::new(
            sample_user_id(),
            ReminderSource::CalendarEvent,
            "Team standup",
            Utc::now(),
        );
        let display = format!("{reminder}");
        assert!(display.contains("Calendar Event"));
        assert!(display.contains("Team standup"));
        assert!(display.contains("Pending"));
    }

    #[test]
    fn serialization_roundtrip() {
        let reminder = Reminder::new(
            sample_user_id(),
            ReminderSource::Custom,
            "Serde test",
            Utc::now(),
        )
        .with_description("Testing serialization")
        .with_location("Home");

        let json = serde_json::to_string(&reminder).unwrap();
        let deserialized: Reminder = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.title, "Serde test");
        assert_eq!(deserialized.source, ReminderSource::Custom);
        assert_eq!(
            deserialized.description.as_deref(),
            Some("Testing serialization")
        );
        assert_eq!(deserialized.location.as_deref(), Some("Home"));
    }
}
