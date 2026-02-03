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
    async fn update_event(
        &self,
        calendar: &str,
        event: &CalendarEvent,
    ) -> Result<(), CalDavError>;

    /// Delete an event
    async fn delete_event(&self, calendar: &str, event_id: &str) -> Result<(), CalDavError>;
}

// TODO: Implement actual CalDAV client using reqwest and icalendar crate
