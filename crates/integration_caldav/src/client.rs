//! CalDAV client
//!
//! Connects to CalDAV servers for calendar operations.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// CalDAV client errors
#[derive(Debug, Error)]
pub enum CalDavError {
    #[error("Connection failed: {0}")]
    ConnectionFailed(String),

    #[error("Authentication failed")]
    AuthenticationFailed,

    #[error("Calendar not found: {0}")]
    CalendarNotFound(String),

    #[error("Event not found: {0}")]
    EventNotFound(String),

    #[error("Request failed: {0}")]
    RequestFailed(String),
}

/// CalDAV server configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CalDavConfig {
    /// Server URL (e.g., https://cal.example.com)
    pub server_url: String,
    /// Username
    pub username: String,
    /// Password
    pub password: String,
    /// Default calendar path
    pub calendar_path: Option<String>,
}

/// A calendar event
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CalendarEvent {
    /// Unique event ID
    pub id: String,
    /// Event summary/title
    pub summary: String,
    /// Event description
    pub description: Option<String>,
    /// Start time (ISO 8601)
    pub start: String,
    /// End time (ISO 8601)
    pub end: String,
    /// Location
    pub location: Option<String>,
    /// Attendees
    pub attendees: Vec<String>,
}

/// CalDAV client trait
#[async_trait]
pub trait CalDavClient: Send + Sync {
    /// List calendars
    async fn list_calendars(&self) -> Result<Vec<String>, CalDavError>;

    /// Get events in a date range
    async fn get_events(
        &self,
        calendar: &str,
        start: &str,
        end: &str,
    ) -> Result<Vec<CalendarEvent>, CalDavError>;

    /// Create a new event
    async fn create_event(
        &self,
        calendar: &str,
        event: &CalendarEvent,
    ) -> Result<String, CalDavError>;

    /// Update an existing event
    async fn update_event(&self, calendar: &str, event: &CalendarEvent) -> Result<(), CalDavError>;

    /// Delete an event
    async fn delete_event(&self, calendar: &str, event_id: &str) -> Result<(), CalDavError>;
}

// TODO: Implement actual CalDAV client using reqwest and icalendar crate

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn caldav_error_connection_failed() {
        let err = CalDavError::ConnectionFailed("timeout".to_string());
        assert_eq!(err.to_string(), "Connection failed: timeout");
    }

    #[test]
    fn caldav_error_authentication_failed() {
        let err = CalDavError::AuthenticationFailed;
        assert_eq!(err.to_string(), "Authentication failed");
    }

    #[test]
    fn caldav_error_calendar_not_found() {
        let err = CalDavError::CalendarNotFound("work".to_string());
        assert_eq!(err.to_string(), "Calendar not found: work");
    }

    #[test]
    fn caldav_error_event_not_found() {
        let err = CalDavError::EventNotFound("abc123".to_string());
        assert_eq!(err.to_string(), "Event not found: abc123");
    }

    #[test]
    fn caldav_error_request_failed() {
        let err = CalDavError::RequestFailed("500 error".to_string());
        assert_eq!(err.to_string(), "Request failed: 500 error");
    }

    #[test]
    fn caldav_config_creation() {
        let config = CalDavConfig {
            server_url: "https://cal.example.com".to_string(),
            username: "user".to_string(),
            password: "pass".to_string(),
            calendar_path: Some("/calendars/main".to_string()),
        };
        assert_eq!(config.server_url, "https://cal.example.com");
        assert_eq!(config.username, "user");
    }

    #[test]
    fn caldav_config_serialization() {
        let config = CalDavConfig {
            server_url: "https://cal.example.com".to_string(),
            username: "user".to_string(),
            password: "pass".to_string(),
            calendar_path: None,
        };
        let json = serde_json::to_string(&config).unwrap();
        assert!(json.contains("server_url"));
        assert!(json.contains("username"));
    }

    #[test]
    fn caldav_config_deserialization() {
        let json =
            r#"{"server_url":"https://cal.example.com","username":"user","password":"pass"}"#;
        let config: CalDavConfig = serde_json::from_str(json).unwrap();
        assert_eq!(config.server_url, "https://cal.example.com");
        assert!(config.calendar_path.is_none());
    }

    #[test]
    fn calendar_event_creation() {
        let event = CalendarEvent {
            id: "evt123".to_string(),
            summary: "Meeting".to_string(),
            description: Some("Important meeting".to_string()),
            start: "2025-02-01T10:00:00".to_string(),
            end: "2025-02-01T11:00:00".to_string(),
            location: Some("Room 1".to_string()),
            attendees: vec!["user@example.com".to_string()],
        };
        assert_eq!(event.id, "evt123");
        assert_eq!(event.summary, "Meeting");
    }

    #[test]
    fn calendar_event_serialization() {
        let event = CalendarEvent {
            id: "evt123".to_string(),
            summary: "Meeting".to_string(),
            description: None,
            start: "2025-02-01T10:00:00".to_string(),
            end: "2025-02-01T11:00:00".to_string(),
            location: None,
            attendees: vec![],
        };
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("summary"));
        assert!(json.contains("Meeting"));
    }

    #[test]
    fn calendar_event_deserialization() {
        let json = r#"{"id":"1","summary":"Test","description":null,"start":"2025-01-01T09:00:00","end":"2025-01-01T10:00:00","location":null,"attendees":[]}"#;
        let event: CalendarEvent = serde_json::from_str(json).unwrap();
        assert_eq!(event.summary, "Test");
    }

    #[test]
    fn calendar_event_has_debug() {
        let event = CalendarEvent {
            id: "1".to_string(),
            summary: "Test".to_string(),
            description: None,
            start: "2025-01-01T09:00:00".to_string(),
            end: "2025-01-01T10:00:00".to_string(),
            location: None,
            attendees: vec![],
        };
        let debug = format!("{event:?}");
        assert!(debug.contains("CalendarEvent"));
        assert!(debug.contains("summary"));
    }

    #[test]
    fn calendar_event_clone() {
        let event = CalendarEvent {
            id: "1".to_string(),
            summary: "Test".to_string(),
            description: None,
            start: "2025-01-01T09:00:00".to_string(),
            end: "2025-01-01T10:00:00".to_string(),
            location: None,
            attendees: vec!["a@b.com".to_string()],
        };
        #[allow(clippy::redundant_clone)]
        let cloned = event.clone();
        assert_eq!(event.id, cloned.id);
        assert_eq!(event.attendees, cloned.attendees);
    }
}
