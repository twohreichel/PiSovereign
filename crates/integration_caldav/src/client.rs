//! CalDAV client
//!
//! Connects to CalDAV servers for calendar operations.
//! Supports standard CalDAV protocol with PROPFIND, REPORT, PUT, DELETE.

use std::fmt::Write as _;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use quick_xml::{Reader, events::Event};
use reqwest::{Client, StatusCode};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tracing::{debug, instrument};

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

    #[error("Parse error: {0}")]
    ParseError(String),
}

/// CalDAV server configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CalDavConfig {
    /// Server URL (e.g., <https://cal.example.com>)
    pub server_url: String,
    /// Username
    pub username: String,
    /// Password
    pub password: String,
    /// Default calendar path
    pub calendar_path: Option<String>,
    /// Verify TLS certificates (default: true)
    #[serde(default = "default_true")]
    pub verify_certs: bool,
    /// Connection timeout in seconds (default: 30)
    #[serde(default = "default_timeout")]
    pub timeout_secs: u64,
}

const fn default_true() -> bool {
    true
}

const fn default_timeout() -> u64 {
    30
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

/// HTTP-based CalDAV client implementation
#[derive(Debug)]
pub struct HttpCalDavClient {
    pub(crate) client: Client,
    pub(crate) config: CalDavConfig,
}

impl HttpCalDavClient {
    /// Create a new CalDAV client
    pub fn new(config: CalDavConfig) -> Result<Self, CalDavError> {
        let client = Client::builder()
            .timeout(std::time::Duration::from_secs(config.timeout_secs))
            .danger_accept_invalid_certs(!config.verify_certs)
            .build()
            .map_err(|e| CalDavError::ConnectionFailed(e.to_string()))?;

        Ok(Self { client, config })
    }

    /// Build a CalDAV request with proper authentication
    pub(crate) fn build_request(&self, method: &str, url: &str) -> reqwest::RequestBuilder {
        let request = match method {
            "PROPFIND" => self.client.request(
                reqwest::Method::from_bytes(b"PROPFIND").unwrap_or(reqwest::Method::GET),
                url,
            ),
            "REPORT" => self.client.request(
                reqwest::Method::from_bytes(b"REPORT").unwrap_or(reqwest::Method::GET),
                url,
            ),
            _ => self.client.request(
                reqwest::Method::from_bytes(method.as_bytes()).unwrap_or(reqwest::Method::GET),
                url,
            ),
        };

        request
            .basic_auth(&self.config.username, Some(&self.config.password))
            .header("Content-Type", "application/xml; charset=utf-8")
    }

    /// Build the calendar URL
    pub(crate) fn calendar_url(&self, calendar: &str) -> String {
        if calendar.starts_with("http") {
            calendar.to_string()
        } else {
            format!(
                "{}/{}",
                self.config.server_url.trim_end_matches('/'),
                calendar.trim_start_matches('/')
            )
        }
    }

    /// Parse iCalendar data to extract events
    fn parse_icalendar(ical_data: &str) -> Result<Vec<CalendarEvent>, CalDavError> {
        use icalendar::{CalendarComponent, Component, parser};

        let calendars = parser::unfold(ical_data);
        let parsed = parser::read_calendar(&calendars)
            .map_err(|e| CalDavError::ParseError(format!("iCalendar parse error: {e}")))?;

        let mut events = Vec::new();

        for component in parsed.components {
            // Convert parser::Component to CalendarComponent
            let cal_component = CalendarComponent::from(component);

            if let CalendarComponent::Event(event) = cal_component {
                let id = event.get_uid().unwrap_or_default().to_string();
                let summary = event.get_summary().unwrap_or_default().to_string();
                let description = event.get_description().map(ToString::to_string);
                let location = event.property_value("LOCATION").map(ToString::to_string);

                // Get start/end times as strings
                let start = event
                    .property_value("DTSTART")
                    .unwrap_or_default()
                    .to_string();
                let end = event
                    .property_value("DTEND")
                    .unwrap_or_default()
                    .to_string();

                // Collect attendees from multi_properties
                let attendees: Vec<String> = event
                    .multi_properties()
                    .get("ATTENDEE")
                    .map(|props| props.iter().map(|p| p.value().to_string()).collect())
                    .unwrap_or_default();

                if !id.is_empty() && !summary.is_empty() {
                    events.push(CalendarEvent {
                        id,
                        summary,
                        description,
                        start,
                        end,
                        location,
                        attendees,
                    });
                }
            }
        }

        Ok(events)
    }

    /// Extract calendar data from CalDAV XML response
    ///
    /// Properly parses XML and decodes CDATA sections containing iCalendar data
    pub(crate) fn extract_calendar_data_from_xml(xml_body: &str) -> Vec<String> {
        let mut reader = Reader::from_str(xml_body);
        reader.config_mut().trim_text(true);

        let mut ical_data_list = Vec::new();
        let mut buf = Vec::new();
        let mut inside_calendar_data = false;
        let mut current_ical = String::new();

        loop {
            match reader.read_event_into(&mut buf) {
                Ok(Event::Start(e)) => {
                    if e.name().as_ref() == b"C:calendar-data"
                        || e.name().as_ref() == b"calendar-data"
                        || e.name().as_ref() == b"D:calendar-data"
                    {
                        inside_calendar_data = true;
                        current_ical.clear();
                    }
                },
                Ok(Event::Text(e)) => {
                    if inside_calendar_data {
                        if let Ok(text) = e.unescape() {
                            current_ical.push_str(&text);
                        }
                    }
                },
                Ok(Event::CData(e)) => {
                    if inside_calendar_data {
                        if let Ok(text) = std::str::from_utf8(e.as_ref()) {
                            current_ical.push_str(text);
                        }
                    }
                },
                Ok(Event::End(e)) => {
                    if inside_calendar_data
                        && (e.name().as_ref() == b"C:calendar-data"
                            || e.name().as_ref() == b"calendar-data"
                            || e.name().as_ref() == b"D:calendar-data")
                    {
                        inside_calendar_data = false;
                        if !current_ical.trim().is_empty() {
                            ical_data_list.push(current_ical.clone());
                        }
                    }
                },
                Ok(Event::Eof) => break,
                Err(e) => {
                    debug!(error = ?e, "XML parsing error, falling back to string search");
                    break;
                },
                _ => {},
            }
            buf.clear();
        }

        ical_data_list
    }

    /// Build iCalendar VEVENT from `CalendarEvent`
    fn build_icalendar(event: &CalendarEvent) -> String {
        use chrono::{NaiveDateTime, TimeZone};

        let now: DateTime<Utc> = Utc::now();
        let dtstamp = now.format("%Y%m%dT%H%M%SZ").to_string();

        let mut ical = String::new();
        ical.push_str("BEGIN:VCALENDAR\r\n");
        ical.push_str("VERSION:2.0\r\n");
        ical.push_str("PRODID:-//PiSovereign//CalDAV Client//EN\r\n");
        ical.push_str("BEGIN:VEVENT\r\n");
        let _ = writeln!(ical, "UID:{}\r", event.id);
        let _ = writeln!(ical, "DTSTAMP:{dtstamp}\r");
        let _ = writeln!(ical, "SUMMARY:{}\r", event.summary);

        // Format dates - try to parse ISO 8601 and convert to iCalendar format
        #[allow(clippy::option_if_let_else)]
        let format_date = |date_str: &str| -> String {
            if let Ok(dt) = NaiveDateTime::parse_from_str(date_str, "%Y-%m-%dT%H:%M:%S") {
                Utc.from_utc_datetime(&dt)
                    .format("%Y%m%dT%H%M%SZ")
                    .to_string()
            } else if let Ok(dt) = DateTime::parse_from_rfc3339(date_str) {
                dt.with_timezone(&Utc).format("%Y%m%dT%H%M%SZ").to_string()
            } else {
                // Return as-is if we can't parse
                date_str.to_string()
            }
        };

        let _ = writeln!(ical, "DTSTART:{}\r", format_date(&event.start));
        let _ = writeln!(ical, "DTEND:{}\r", format_date(&event.end));

        if let Some(desc) = &event.description {
            let _ = writeln!(ical, "DESCRIPTION:{desc}\r");
        }
        if let Some(loc) = &event.location {
            let _ = writeln!(ical, "LOCATION:{loc}\r");
        }
        for attendee in &event.attendees {
            let _ = writeln!(ical, "ATTENDEE:mailto:{attendee}\r");
        }

        ical.push_str("END:VEVENT\r\n");
        ical.push_str("END:VCALENDAR\r\n");
        ical
    }
}

#[async_trait]
impl CalDavClient for HttpCalDavClient {
    #[instrument(skip(self))]
    async fn list_calendars(&self) -> Result<Vec<String>, CalDavError> {
        let url = &self.config.server_url;

        // PROPFIND request to discover calendars
        let body = r#"<?xml version="1.0" encoding="utf-8" ?>
<D:propfind xmlns:D="DAV:" xmlns:C="urn:ietf:params:xml:ns:caldav">
  <D:prop>
    <D:displayname/>
    <D:resourcetype/>
  </D:prop>
</D:propfind>"#;

        let response = self
            .build_request("PROPFIND", url)
            .header("Depth", "1")
            .body(body)
            .send()
            .await
            .map_err(|e| CalDavError::ConnectionFailed(e.to_string()))?;

        match response.status() {
            StatusCode::UNAUTHORIZED => return Err(CalDavError::AuthenticationFailed),
            status if !status.is_success() && status != StatusCode::MULTI_STATUS => {
                return Err(CalDavError::RequestFailed(format!("HTTP {status}")));
            },
            _ => {},
        }

        let body = response
            .text()
            .await
            .map_err(|e| CalDavError::RequestFailed(e.to_string()))?;

        debug!(response_len = body.len(), "PROPFIND response received");

        // Simple XML parsing to extract hrefs (a full implementation would use a proper XML parser)
        let mut calendars = Vec::new();
        for line in body.lines() {
            if line.contains("<D:href>") || line.contains("<d:href>") {
                if let Some(start) = line.find('>') {
                    if let Some(end) = line[start..].find('<') {
                        let href = &line[start + 1..start + end];
                        if href.contains("calendar") || href.ends_with('/') {
                            calendars.push(href.to_string());
                        }
                    }
                }
            }
        }

        Ok(calendars)
    }

    #[instrument(skip(self))]
    async fn get_events(
        &self,
        calendar: &str,
        start: &str,
        end: &str,
    ) -> Result<Vec<CalendarEvent>, CalDavError> {
        let url = self.calendar_url(calendar);

        // REPORT request with calendar-query
        let body = format!(
            r#"<?xml version="1.0" encoding="utf-8" ?>
<C:calendar-query xmlns:D="DAV:" xmlns:C="urn:ietf:params:xml:ns:caldav">
  <D:prop>
    <D:getetag/>
    <C:calendar-data/>
  </D:prop>
  <C:filter>
    <C:comp-filter name="VCALENDAR">
      <C:comp-filter name="VEVENT">
        <C:time-range start="{start}" end="{end}"/>
      </C:comp-filter>
    </C:comp-filter>
  </C:filter>
</C:calendar-query>"#
        );

        let response = self
            .build_request("REPORT", &url)
            .header("Depth", "1")
            .body(body)
            .send()
            .await
            .map_err(|e| CalDavError::ConnectionFailed(e.to_string()))?;

        match response.status() {
            StatusCode::UNAUTHORIZED => return Err(CalDavError::AuthenticationFailed),
            StatusCode::NOT_FOUND => {
                return Err(CalDavError::CalendarNotFound(calendar.to_string()));
            },
            status if !status.is_success() && status != StatusCode::MULTI_STATUS => {
                return Err(CalDavError::RequestFailed(format!("HTTP {status}")));
            },
            _ => {},
        }

        let body = response
            .text()
            .await
            .map_err(|e| CalDavError::RequestFailed(e.to_string()))?;

        debug!(response_len = body.len(), "REPORT response received");

        // Extract iCalendar data using proper XML parsing
        let ical_data_list = Self::extract_calendar_data_from_xml(&body);

        let mut all_events = Vec::new();
        for ical_data in ical_data_list {
            if let Ok(events) = Self::parse_icalendar(&ical_data) {
                all_events.extend(events);
            }
        }

        Ok(all_events)
    }

    #[instrument(skip(self, event))]
    async fn create_event(
        &self,
        calendar: &str,
        event: &CalendarEvent,
    ) -> Result<String, CalDavError> {
        let url = format!(
            "{}/{}.ics",
            self.calendar_url(calendar).trim_end_matches('/'),
            event.id
        );
        let ical = Self::build_icalendar(event);

        let response = self
            .client
            .put(&url)
            .basic_auth(&self.config.username, Some(&self.config.password))
            .header("Content-Type", "text/calendar; charset=utf-8")
            .body(ical)
            .send()
            .await
            .map_err(|e| CalDavError::ConnectionFailed(e.to_string()))?;

        match response.status() {
            StatusCode::UNAUTHORIZED => Err(CalDavError::AuthenticationFailed),
            StatusCode::NOT_FOUND => Err(CalDavError::CalendarNotFound(calendar.to_string())),
            StatusCode::CREATED | StatusCode::NO_CONTENT | StatusCode::OK => {
                debug!(event_id = %event.id, "Event created successfully");
                Ok(event.id.clone())
            },
            status => Err(CalDavError::RequestFailed(format!("HTTP {status}"))),
        }
    }

    #[instrument(skip(self, event))]
    async fn update_event(&self, calendar: &str, event: &CalendarEvent) -> Result<(), CalDavError> {
        // CalDAV uses PUT for both create and update
        self.create_event(calendar, event).await?;
        Ok(())
    }

    #[instrument(skip(self))]
    async fn delete_event(&self, calendar: &str, event_id: &str) -> Result<(), CalDavError> {
        let url = format!(
            "{}/{}.ics",
            self.calendar_url(calendar).trim_end_matches('/'),
            event_id
        );

        let response = self
            .client
            .delete(&url)
            .basic_auth(&self.config.username, Some(&self.config.password))
            .send()
            .await
            .map_err(|e| CalDavError::ConnectionFailed(e.to_string()))?;

        match response.status() {
            StatusCode::UNAUTHORIZED => Err(CalDavError::AuthenticationFailed),
            StatusCode::NOT_FOUND => Err(CalDavError::EventNotFound(event_id.to_string())),
            StatusCode::NO_CONTENT | StatusCode::OK => {
                debug!(event_id = %event_id, "Event deleted successfully");
                Ok(())
            },
            status => Err(CalDavError::RequestFailed(format!("HTTP {status}"))),
        }
    }
}

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

    // Helper function to create test CalDavConfig with default security settings
    fn test_caldav_config(
        server_url: &str,
        username: &str,
        password: &str,
        calendar_path: Option<String>,
    ) -> CalDavConfig {
        CalDavConfig {
            server_url: server_url.to_string(),
            username: username.to_string(),
            password: password.to_string(),
            calendar_path,
            verify_certs: true,
            timeout_secs: 30,
        }
    }

    #[test]
    fn caldav_config_creation() {
        let config = test_caldav_config(
            "https://cal.example.com",
            "user",
            "pass",
            Some("/calendars/main".to_string()),
        );
        assert_eq!(config.server_url, "https://cal.example.com");
        assert_eq!(config.username, "user");
    }

    #[test]
    fn caldav_config_serialization() {
        let config = test_caldav_config("https://cal.example.com", "user", "pass", None);
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

    #[test]
    fn http_client_creation() {
        let config = test_caldav_config("https://cal.example.com", "user", "pass", None);
        let client = HttpCalDavClient::new(config);
        assert!(client.is_ok());
    }

    #[test]
    fn http_client_calendar_url_full_url() {
        let config = test_caldav_config("https://cal.example.com", "user", "pass", None);
        let client = HttpCalDavClient::new(config).unwrap();
        let url = client.calendar_url("https://other.com/calendar");
        assert_eq!(url, "https://other.com/calendar");
    }

    #[test]
    fn http_client_calendar_url_relative_path() {
        let config = test_caldav_config("https://cal.example.com/", "user", "pass", None);
        let client = HttpCalDavClient::new(config).unwrap();
        let url = client.calendar_url("/calendars/main");
        assert_eq!(url, "https://cal.example.com/calendars/main");
    }

    #[test]
    fn http_client_build_icalendar_basic() {
        let config = test_caldav_config("https://cal.example.com", "user", "pass", None);
        let _client = HttpCalDavClient::new(config).unwrap();

        let event = CalendarEvent {
            id: "test-event-123".to_string(),
            summary: "Test Meeting".to_string(),
            description: Some("Important discussion".to_string()),
            start: "2025-02-01T10:00:00".to_string(),
            end: "2025-02-01T11:00:00".to_string(),
            location: Some("Room A".to_string()),
            attendees: vec!["user@example.com".to_string()],
        };

        let ical = HttpCalDavClient::build_icalendar(&event);

        assert!(ical.starts_with("BEGIN:VCALENDAR\r\n"));
        assert!(ical.ends_with("END:VCALENDAR\r\n"));
        assert!(ical.contains("BEGIN:VEVENT\r\n"));
        assert!(ical.contains("END:VEVENT\r\n"));
        assert!(ical.contains("UID:test-event-123\r\n"));
        assert!(ical.contains("SUMMARY:Test Meeting\r\n"));
        assert!(ical.contains("DESCRIPTION:Important discussion\r\n"));
        assert!(ical.contains("LOCATION:Room A\r\n"));
        assert!(ical.contains("ATTENDEE:mailto:user@example.com\r\n"));
    }

    #[test]
    fn http_client_build_icalendar_no_optional_fields() {
        let config = test_caldav_config("https://cal.example.com", "user", "pass", None);
        let _client = HttpCalDavClient::new(config).unwrap();

        let event = CalendarEvent {
            id: "simple-event".to_string(),
            summary: "Simple Event".to_string(),
            description: None,
            start: "2025-02-01T09:00:00".to_string(),
            end: "2025-02-01T10:00:00".to_string(),
            location: None,
            attendees: vec![],
        };

        let ical = HttpCalDavClient::build_icalendar(&event);

        assert!(ical.contains("UID:simple-event\r\n"));
        assert!(ical.contains("SUMMARY:Simple Event\r\n"));
        assert!(!ical.contains("DESCRIPTION:"));
        assert!(!ical.contains("LOCATION:"));
        assert!(!ical.contains("ATTENDEE:"));
    }

    #[test]
    fn http_client_parse_icalendar_valid() {
        let config = test_caldav_config("https://cal.example.com", "user", "pass", None);
        let _client = HttpCalDavClient::new(config).unwrap();

        let ical_data = r"BEGIN:VCALENDAR
VERSION:2.0
PRODID:-//Test//Test//EN
BEGIN:VEVENT
UID:event-123
SUMMARY:Test Event
DESCRIPTION:Test Description
DTSTART:20250201T100000Z
DTEND:20250201T110000Z
LOCATION:Test Location
END:VEVENT
END:VCALENDAR";

        let events = HttpCalDavClient::parse_icalendar(ical_data).unwrap();
        assert_eq!(events.len(), 1);

        let event = &events[0];
        assert_eq!(event.id, "event-123");
        assert_eq!(event.summary, "Test Event");
        assert_eq!(event.description.as_deref(), Some("Test Description"));
        assert_eq!(event.location.as_deref(), Some("Test Location"));
        assert_eq!(event.start, "20250201T100000Z");
        assert_eq!(event.end, "20250201T110000Z");
    }

    #[test]
    fn http_client_parse_icalendar_multiple_events() {
        let config = test_caldav_config("https://cal.example.com", "user", "pass", None);
        let _client = HttpCalDavClient::new(config).unwrap();

        let ical_data = r"BEGIN:VCALENDAR
VERSION:2.0
BEGIN:VEVENT
UID:event-1
SUMMARY:First Event
DTSTART:20250201T090000Z
DTEND:20250201T100000Z
END:VEVENT
BEGIN:VEVENT
UID:event-2
SUMMARY:Second Event
DTSTART:20250201T110000Z
DTEND:20250201T120000Z
END:VEVENT
END:VCALENDAR";

        let events = HttpCalDavClient::parse_icalendar(ical_data).unwrap();
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].id, "event-1");
        assert_eq!(events[0].summary, "First Event");
        assert_eq!(events[1].id, "event-2");
        assert_eq!(events[1].summary, "Second Event");
    }

    #[test]
    fn http_client_parse_icalendar_skips_incomplete_events() {
        let config = test_caldav_config("https://cal.example.com", "user", "pass", None);
        let _client = HttpCalDavClient::new(config).unwrap();

        // Event without UID should be skipped
        let ical_data = r"BEGIN:VCALENDAR
VERSION:2.0
BEGIN:VEVENT
SUMMARY:No UID Event
DTSTART:20250201T090000Z
DTEND:20250201T100000Z
END:VEVENT
BEGIN:VEVENT
UID:valid-event
SUMMARY:Valid Event
DTSTART:20250201T110000Z
DTEND:20250201T120000Z
END:VEVENT
END:VCALENDAR";

        let events = HttpCalDavClient::parse_icalendar(ical_data).unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].id, "valid-event");
    }

    #[test]
    fn http_client_parse_icalendar_empty_result() {
        let config = test_caldav_config("https://cal.example.com", "user", "pass", None);
        let _client = HttpCalDavClient::new(config).unwrap();

        // Parser is lenient; non-calendar data returns empty events
        let result = HttpCalDavClient::parse_icalendar("not valid icalendar data");
        assert!(result.is_ok());
        assert!(result.unwrap().is_empty());
    }

    #[test]
    fn extract_calendar_data_from_xml_single_event() {
        let config = test_caldav_config("https://cal.example.com", "user", "pass", None);
        let _client = HttpCalDavClient::new(config).unwrap();

        let xml_response = r#"<?xml version="1.0" encoding="utf-8" ?>
<D:multistatus xmlns:D="DAV:" xmlns:C="urn:ietf:params:xml:ns:caldav">
  <D:response>
    <C:calendar-data><![CDATA[BEGIN:VCALENDAR
VERSION:2.0
BEGIN:VEVENT
UID:test-123
SUMMARY:Test Event
DTSTART:20240215T100000Z
DTEND:20240215T110000Z
END:VEVENT
END:VCALENDAR]]></C:calendar-data>
  </D:response>
</D:multistatus>"#;

        let ical_data = HttpCalDavClient::extract_calendar_data_from_xml(xml_response);
        assert_eq!(ical_data.len(), 1);
        assert!(ical_data[0].contains("BEGIN:VCALENDAR"));
        assert!(ical_data[0].contains("SUMMARY:Test Event"));
    }

    #[test]
    fn extract_calendar_data_from_xml_multiple_events() {
        let config = test_caldav_config("https://cal.example.com", "user", "pass", None);
        let _client = HttpCalDavClient::new(config).unwrap();

        let xml_response = r#"<?xml version="1.0" encoding="utf-8" ?>
<D:multistatus xmlns:D="DAV:" xmlns:C="urn:ietf:params:xml:ns:caldav">
  <D:response>
    <C:calendar-data>BEGIN:VCALENDAR
VERSION:2.0
BEGIN:VEVENT
UID:event-1
SUMMARY:Event 1
END:VEVENT
END:VCALENDAR</C:calendar-data>
  </D:response>
  <D:response>
    <C:calendar-data>BEGIN:VCALENDAR
VERSION:2.0
BEGIN:VEVENT
UID:event-2
SUMMARY:Event 2
END:VEVENT
END:VCALENDAR</C:calendar-data>
  </D:response>
</D:multistatus>"#;

        let ical_data = HttpCalDavClient::extract_calendar_data_from_xml(xml_response);
        assert_eq!(ical_data.len(), 2);
        assert!(ical_data[0].contains("Event 1"));
        assert!(ical_data[1].contains("Event 2"));
    }

    #[test]
    fn extract_calendar_data_from_xml_with_entities() {
        let config = test_caldav_config("https://cal.example.com", "user", "pass", None);
        let _client = HttpCalDavClient::new(config).unwrap();

        let xml_response = r#"<?xml version="1.0" encoding="utf-8" ?>
<D:multistatus xmlns:D="DAV:" xmlns:C="urn:ietf:params:xml:ns:caldav">
  <D:response>
    <C:calendar-data>BEGIN:VCALENDAR
DESCRIPTION:Meeting &lt;important&gt; &amp; urgent
END:VCALENDAR</C:calendar-data>
  </D:response>
</D:multistatus>"#;

        let ical_data = HttpCalDavClient::extract_calendar_data_from_xml(xml_response);
        assert_eq!(ical_data.len(), 1);
        // XML entities should be decoded by quick-xml
        assert!(ical_data[0].contains("<important>"));
        assert!(ical_data[0].contains("& urgent"));
    }

    #[test]
    fn extract_calendar_data_from_xml_empty_response() {
        let config = test_caldav_config("https://cal.example.com", "user", "pass", None);
        let _client = HttpCalDavClient::new(config).unwrap();

        let xml_response = r#"<?xml version="1.0" encoding="utf-8" ?>
<D:multistatus xmlns:D="DAV:">
</D:multistatus>"#;

        let ical_data = HttpCalDavClient::extract_calendar_data_from_xml(xml_response);
        assert_eq!(ical_data.len(), 0);
    }

    #[test]
    fn caldav_config_clone() {
        let config = test_caldav_config(
            "https://cal.example.com",
            "user",
            "pass",
            Some("/cal".to_string()),
        );
        #[allow(clippy::redundant_clone)]
        let cloned = config.clone();
        assert_eq!(config.server_url, cloned.server_url);
        assert_eq!(config.calendar_path, cloned.calendar_path);
    }

    #[test]
    fn caldav_config_debug() {
        let config = test_caldav_config("https://cal.example.com", "user", "pass", None);
        let debug = format!("{config:?}");
        assert!(debug.contains("CalDavConfig"));
        assert!(debug.contains("server_url"));
    }

    #[test]
    fn caldav_error_parse_error() {
        let err = CalDavError::ParseError("invalid format".to_string());
        assert_eq!(err.to_string(), "Parse error: invalid format");
    }

    #[test]
    fn http_client_has_debug() {
        let config = test_caldav_config("https://cal.example.com", "user", "pass", None);
        let client = HttpCalDavClient::new(config).unwrap();
        let debug = format!("{client:?}");
        assert!(debug.contains("HttpCalDavClient"));
    }
}
