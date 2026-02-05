//! Calendar port for application layer
//!
//! Defines the interface for calendar operations (read/write events).
//! Implemented by adapters in the infrastructure layer.

use async_trait::async_trait;
use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Calendar port errors
#[derive(Debug, Error)]
pub enum CalendarError {
    #[error("Calendar service unavailable")]
    ServiceUnavailable,

    #[error("Authentication failed")]
    AuthenticationFailed,

    #[error("Calendar not found: {0}")]
    CalendarNotFound(String),

    #[error("Event not found: {0}")]
    EventNotFound(String),

    #[error("Operation failed: {0}")]
    OperationFailed(String),

    #[error("Invalid date/time: {0}")]
    InvalidDateTime(String),
}

/// Calendar event representation
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CalendarEvent {
    /// Unique event identifier
    pub id: String,
    /// Event title/summary
    pub title: String,
    /// Event description
    pub description: Option<String>,
    /// Start time (RFC 3339)
    pub start: String,
    /// End time (RFC 3339)
    pub end: String,
    /// Location
    pub location: Option<String>,
    /// Whether this is an all-day event
    pub all_day: bool,
    /// Attendees (email addresses)
    pub attendees: Vec<String>,
}

impl CalendarEvent {
    /// Create a new calendar event
    pub fn new(
        id: impl Into<String>,
        title: impl Into<String>,
        start: impl Into<String>,
        end: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            title: title.into(),
            description: None,
            start: start.into(),
            end: end.into(),
            location: None,
            all_day: false,
            attendees: Vec::new(),
        }
    }

    /// Set the event description
    #[must_use]
    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    /// Set the event location
    #[must_use]
    pub fn with_location(mut self, location: impl Into<String>) -> Self {
        self.location = Some(location.into());
        self
    }

    /// Mark as all-day event
    #[must_use]
    pub const fn as_all_day(mut self) -> Self {
        self.all_day = true;
        self
    }

    /// Add an attendee
    #[must_use]
    pub fn with_attendee(mut self, attendee: impl Into<String>) -> Self {
        self.attendees.push(attendee.into());
        self
    }
}

/// New event request (for creating events)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewEvent {
    /// Event title/summary
    pub title: String,
    /// Event description
    pub description: Option<String>,
    /// Start time (RFC 3339 or "YYYY-MM-DD" for all-day)
    pub start: String,
    /// End time (RFC 3339 or "YYYY-MM-DD" for all-day)
    pub end: String,
    /// Location
    pub location: Option<String>,
    /// Whether this is an all-day event
    pub all_day: bool,
    /// Attendees to invite
    pub attendees: Vec<String>,
}

impl NewEvent {
    /// Create a new event request
    pub fn new(title: impl Into<String>, start: impl Into<String>, end: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            description: None,
            start: start.into(),
            end: end.into(),
            location: None,
            all_day: false,
            attendees: Vec::new(),
        }
    }

    /// Set the description
    #[must_use]
    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    /// Set the location
    #[must_use]
    pub fn with_location(mut self, location: impl Into<String>) -> Self {
        self.location = Some(location.into());
        self
    }

    /// Mark as all-day event
    #[must_use]
    pub const fn as_all_day(mut self) -> Self {
        self.all_day = true;
        self
    }
}

/// Calendar information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CalendarInfo {
    /// Calendar identifier
    pub id: String,
    /// Calendar display name
    pub name: String,
    /// Calendar color (hex)
    pub color: Option<String>,
    /// Whether this is the default calendar
    pub is_default: bool,
}

/// Calendar port trait
///
/// Defines operations for calendar management.
/// Implemented by adapters that connect to calendar services (CalDAV, etc).
#[async_trait]
pub trait CalendarPort: Send + Sync {
    /// List available calendars
    async fn list_calendars(&self) -> Result<Vec<CalendarInfo>, CalendarError>;

    /// Get events for a specific date
    async fn get_events_for_date(
        &self,
        date: NaiveDate,
    ) -> Result<Vec<CalendarEvent>, CalendarError>;

    /// Get events in a date range
    ///
    /// # Arguments
    /// * `start` - Range start (inclusive)
    /// * `end` - Range end (inclusive)
    async fn get_events_in_range(
        &self,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
    ) -> Result<Vec<CalendarEvent>, CalendarError>;

    /// Get a specific event by ID
    async fn get_event(&self, event_id: &str) -> Result<CalendarEvent, CalendarError>;

    /// Create a new event
    ///
    /// # Returns
    /// The created event's ID
    async fn create_event(&self, event: &NewEvent) -> Result<String, CalendarError>;

    /// Update an existing event
    async fn update_event(&self, event_id: &str, event: &NewEvent) -> Result<(), CalendarError>;

    /// Delete an event
    async fn delete_event(&self, event_id: &str) -> Result<(), CalendarError>;

    /// Check if the calendar service is available
    async fn is_available(&self) -> bool;

    /// Get the next upcoming event
    async fn get_next_event(&self) -> Result<Option<CalendarEvent>, CalendarError>;

    /// Get today's events (convenience method)
    async fn get_today_events(&self) -> Result<Vec<CalendarEvent>, CalendarError> {
        let today = chrono::Local::now().date_naive();
        self.get_events_for_date(today).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn calendar_event_creation() {
        let event = CalendarEvent::new(
            "evt-1",
            "Meeting",
            "2024-01-15T10:00:00Z",
            "2024-01-15T11:00:00Z",
        );
        assert_eq!(event.id, "evt-1");
        assert_eq!(event.title, "Meeting");
        assert!(!event.all_day);
    }

    #[test]
    fn calendar_event_builder_pattern() {
        let event = CalendarEvent::new(
            "evt-1",
            "Meeting",
            "2024-01-15T10:00:00Z",
            "2024-01-15T11:00:00Z",
        )
        .with_description("Weekly sync")
        .with_location("Room A")
        .with_attendee("alice@example.com")
        .with_attendee("bob@example.com");

        assert_eq!(event.description, Some("Weekly sync".to_string()));
        assert_eq!(event.location, Some("Room A".to_string()));
        assert_eq!(event.attendees.len(), 2);
    }

    #[test]
    fn calendar_event_all_day() {
        let event = CalendarEvent::new("evt-1", "Holiday", "2024-01-15", "2024-01-15").as_all_day();
        assert!(event.all_day);
    }

    #[test]
    fn new_event_creation() {
        let event = NewEvent::new("Meeting", "2024-01-15T10:00:00Z", "2024-01-15T11:00:00Z");
        assert_eq!(event.title, "Meeting");
        assert!(!event.all_day);
    }

    #[test]
    fn new_event_builder_pattern() {
        let event = NewEvent::new("Meeting", "2024-01-15T10:00:00Z", "2024-01-15T11:00:00Z")
            .with_description("Weekly sync")
            .with_location("Room A")
            .as_all_day();

        assert_eq!(event.description, Some("Weekly sync".to_string()));
        assert!(event.all_day);
    }

    #[test]
    fn calendar_error_display() {
        let error = CalendarError::ServiceUnavailable;
        assert_eq!(error.to_string(), "Calendar service unavailable");

        let error = CalendarError::EventNotFound("evt-123".to_string());
        assert_eq!(error.to_string(), "Event not found: evt-123");
    }

    #[test]
    fn calendar_event_serialization() {
        let event = CalendarEvent::new(
            "evt-1",
            "Meeting",
            "2024-01-15T10:00:00Z",
            "2024-01-15T11:00:00Z",
        );
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("\"id\":\"evt-1\""));
        assert!(json.contains("\"title\":\"Meeting\""));
    }

    #[test]
    fn calendar_event_deserialization() {
        let json = r#"{
            "id": "evt-1",
            "title": "Meeting",
            "description": null,
            "start": "2024-01-15T10:00:00Z",
            "end": "2024-01-15T11:00:00Z",
            "location": null,
            "all_day": false,
            "attendees": []
        }"#;
        let event: CalendarEvent = serde_json::from_str(json).unwrap();
        assert_eq!(event.id, "evt-1");
    }

    #[test]
    fn calendar_info_serialization() {
        let info = CalendarInfo {
            id: "cal-1".to_string(),
            name: "Personal".to_string(),
            color: Some("#ff0000".to_string()),
            is_default: true,
        };
        let json = serde_json::to_string(&info).unwrap();
        assert!(json.contains("\"name\":\"Personal\""));
    }
}
