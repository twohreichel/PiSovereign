//! Calendar service
//!
//! Business logic for calendar operations including event management.

use std::{fmt, sync::Arc};

use chrono::{DateTime, Duration, NaiveDate, NaiveTime, Utc};
use tracing::{debug, info, instrument};

use crate::{
    error::ApplicationError,
    ports::{CalendarError, CalendarEvent, CalendarPort, NewEvent},
    services::{CalendarBrief, EventSummary},
};

/// Calendar service for handling calendar operations
pub struct CalendarService {
    calendar_port: Arc<dyn CalendarPort>,
}

impl fmt::Debug for CalendarService {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CalendarService").finish_non_exhaustive()
    }
}

impl CalendarService {
    /// Create a new calendar service
    pub fn new(calendar_port: Arc<dyn CalendarPort>) -> Self {
        Self { calendar_port }
    }

    /// Get events for today
    #[instrument(skip(self))]
    pub async fn get_today_events(&self) -> Result<Vec<CalendarEvent>, ApplicationError> {
        info!("Getting today's events");
        self.calendar_port
            .get_today_events()
            .await
            .map_err(map_error)
    }

    /// Get events for a specific date
    #[instrument(skip(self))]
    pub async fn get_events_for_date(
        &self,
        date: NaiveDate,
    ) -> Result<Vec<CalendarEvent>, ApplicationError> {
        info!(date = %date, "Getting events for date");
        self.calendar_port
            .get_events_for_date(date)
            .await
            .map_err(map_error)
    }

    /// Get events in a date range
    #[instrument(skip(self))]
    pub async fn get_events_in_range(
        &self,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
    ) -> Result<Vec<CalendarEvent>, ApplicationError> {
        info!(start = %start, end = %end, "Getting events in range");
        self.calendar_port
            .get_events_in_range(start, end)
            .await
            .map_err(map_error)
    }

    /// Get the next upcoming event
    #[instrument(skip(self))]
    pub async fn get_next_event(&self) -> Result<Option<CalendarEvent>, ApplicationError> {
        debug!("Getting next event");
        self.calendar_port.get_next_event().await.map_err(map_error)
    }

    /// Create a new calendar event
    ///
    /// # Arguments
    /// * `title` - Event title
    /// * `date` - Event date
    /// * `time` - Event start time
    /// * `duration_minutes` - Duration in minutes
    /// * `location` - Optional location
    /// * `description` - Optional description
    #[instrument(skip(self))]
    pub async fn create_event(
        &self,
        title: &str,
        date: NaiveDate,
        time: NaiveTime,
        duration_minutes: u32,
        location: Option<&str>,
        description: Option<&str>,
    ) -> Result<String, ApplicationError> {
        info!(title, date = %date, time = %time, "Creating event");

        let start = format!("{}T{}", date, time);
        let end_time = time + Duration::minutes(i64::from(duration_minutes));
        let end = format!("{}T{}", date, end_time);

        let mut event = NewEvent::new(title, start, end);

        if let Some(loc) = location {
            event = event.with_location(loc);
        }

        if let Some(desc) = description {
            event = event.with_description(desc);
        }

        self.calendar_port
            .create_event(&event)
            .await
            .map_err(map_error)
    }

    /// Create an all-day event
    #[instrument(skip(self))]
    pub async fn create_all_day_event(
        &self,
        title: &str,
        date: NaiveDate,
        location: Option<&str>,
        description: Option<&str>,
    ) -> Result<String, ApplicationError> {
        info!(title, date = %date, "Creating all-day event");

        let date_str = date.to_string();
        let mut event = NewEvent::new(title, &date_str, &date_str).as_all_day();

        if let Some(loc) = location {
            event = event.with_location(loc);
        }

        if let Some(desc) = description {
            event = event.with_description(desc);
        }

        self.calendar_port
            .create_event(&event)
            .await
            .map_err(map_error)
    }

    /// Delete a calendar event
    #[instrument(skip(self))]
    pub async fn delete_event(&self, event_id: &str) -> Result<(), ApplicationError> {
        info!(event_id, "Deleting event");
        self.calendar_port
            .delete_event(event_id)
            .await
            .map_err(map_error)
    }

    /// Get calendar brief for briefing service
    #[instrument(skip(self))]
    pub async fn get_calendar_brief(
        &self,
        date: NaiveDate,
    ) -> Result<CalendarBrief, ApplicationError> {
        let events = self
            .calendar_port
            .get_events_for_date(date)
            .await
            .map_err(map_error)?;

        let next_event = self
            .calendar_port
            .get_next_event()
            .await
            .map_err(map_error)?;

        let event_summaries: Vec<EventSummary> = events
            .iter()
            .map(|e| EventSummary {
                title: e.title.clone(),
                start_time: extract_time(&e.start),
                end_time: extract_time(&e.end),
                location: e.location.clone(),
                all_day: e.all_day,
            })
            .collect();

        let conflicts = detect_conflicts(&events);

        Ok(CalendarBrief {
            event_count: events.len() as u32,
            next_event: next_event.map(|e| EventSummary {
                title: e.title,
                start_time: extract_time(&e.start),
                end_time: extract_time(&e.end),
                location: e.location,
                all_day: e.all_day,
            }),
            events: event_summaries,
            conflicts,
        })
    }

    /// Check if calendar service is available
    pub async fn is_available(&self) -> bool {
        self.calendar_port.is_available().await
    }

    /// List available calendars
    pub async fn list_calendars(&self) -> Result<Vec<String>, ApplicationError> {
        let calendars = self
            .calendar_port
            .list_calendars()
            .await
            .map_err(map_error)?;
        Ok(calendars.into_iter().map(|c| c.name).collect())
    }
}

/// Extract time portion from ISO datetime string
fn extract_time(datetime: &str) -> String {
    // Try to parse as full datetime
    if let Ok(dt) = DateTime::parse_from_rfc3339(datetime) {
        return dt.format("%H:%M").to_string();
    }

    // Try to extract time from "YYYY-MM-DDTHH:MM:SS" format
    if let Some(t_idx) = datetime.find('T') {
        let time_part = &datetime[t_idx + 1..];
        if time_part.len() >= 5 {
            return time_part[..5].to_string();
        }
    }

    // For all-day events, just return empty or "all-day"
    "all-day".to_string()
}

/// Detect scheduling conflicts
fn detect_conflicts(events: &[CalendarEvent]) -> Vec<String> {
    let mut conflicts = Vec::new();

    for i in 0..events.len() {
        for j in (i + 1)..events.len() {
            let e1 = &events[i];
            let e2 = &events[j];

            // Skip all-day events
            if e1.all_day || e2.all_day {
                continue;
            }

            // Check for overlap
            if times_overlap(&e1.start, &e1.end, &e2.start, &e2.end) {
                conflicts.push(format!("'{}' and '{}' overlap", e1.title, e2.title));
            }
        }
    }

    conflicts
}

/// Check if two time ranges overlap
fn times_overlap(start1: &str, end1: &str, start2: &str, end2: &str) -> bool {
    // Parse as RFC3339
    let s1 = DateTime::parse_from_rfc3339(start1).ok();
    let e1 = DateTime::parse_from_rfc3339(end1).ok();
    let s2 = DateTime::parse_from_rfc3339(start2).ok();
    let e2 = DateTime::parse_from_rfc3339(end2).ok();

    match (s1, e1, s2, e2) {
        (Some(s1), Some(e1), Some(s2), Some(e2)) => {
            // Overlap if: start1 < end2 AND start2 < end1
            s1 < e2 && s2 < e1
        },
        _ => false,
    }
}

/// Map calendar error to application error
fn map_error(err: CalendarError) -> ApplicationError {
    match err {
        CalendarError::ServiceUnavailable => {
            ApplicationError::ExternalService("Calendar service unavailable".to_string())
        },
        CalendarError::AuthenticationFailed => {
            ApplicationError::NotAuthorized("Calendar authentication failed".to_string())
        },
        CalendarError::CalendarNotFound(name) => {
            ApplicationError::ExternalService(format!("Calendar not found: {name}"))
        },
        CalendarError::EventNotFound(id) => {
            ApplicationError::ExternalService(format!("Event not found: {id}"))
        },
        CalendarError::OperationFailed(msg) => ApplicationError::ExternalService(msg),
        CalendarError::InvalidDateTime(msg) => {
            ApplicationError::CommandFailed(format!("Invalid date/time: {msg}"))
        },
    }
}

#[cfg(test)]
mod tests {
    use async_trait::async_trait;

    use super::*;
    use crate::ports::CalendarInfo;

    struct MockCalendarPort {
        events: Vec<CalendarEvent>,
    }

    impl MockCalendarPort {
        fn new(events: Vec<CalendarEvent>) -> Self {
            Self { events }
        }
    }

    #[async_trait]
    impl CalendarPort for MockCalendarPort {
        async fn list_calendars(&self) -> Result<Vec<CalendarInfo>, CalendarError> {
            Ok(vec![CalendarInfo {
                id: "cal-1".to_string(),
                name: "Personal".to_string(),
                color: None,
                is_default: true,
            }])
        }

        async fn get_events_for_date(
            &self,
            _date: NaiveDate,
        ) -> Result<Vec<CalendarEvent>, CalendarError> {
            Ok(self.events.clone())
        }

        async fn get_events_in_range(
            &self,
            _start: DateTime<Utc>,
            _end: DateTime<Utc>,
        ) -> Result<Vec<CalendarEvent>, CalendarError> {
            Ok(self.events.clone())
        }

        async fn get_event(&self, event_id: &str) -> Result<CalendarEvent, CalendarError> {
            self.events
                .iter()
                .find(|e| e.id == event_id)
                .cloned()
                .ok_or_else(|| CalendarError::EventNotFound(event_id.to_string()))
        }

        async fn create_event(&self, _event: &NewEvent) -> Result<String, CalendarError> {
            Ok("evt-new".to_string())
        }

        async fn update_event(
            &self,
            _event_id: &str,
            _event: &NewEvent,
        ) -> Result<(), CalendarError> {
            Ok(())
        }

        async fn delete_event(&self, _event_id: &str) -> Result<(), CalendarError> {
            Ok(())
        }

        async fn is_available(&self) -> bool {
            true
        }

        async fn get_next_event(&self) -> Result<Option<CalendarEvent>, CalendarError> {
            Ok(self.events.first().cloned())
        }
    }

    fn sample_event() -> CalendarEvent {
        CalendarEvent::new(
            "evt-1",
            "Meeting",
            "2024-01-15T10:00:00+00:00",
            "2024-01-15T11:00:00+00:00",
        )
    }

    #[test]
    fn calendar_service_creation() {
        let port = Arc::new(MockCalendarPort::new(vec![]));
        let service = CalendarService::new(port);
        assert!(format!("{service:?}").contains("CalendarService"));
    }

    #[tokio::test]
    async fn get_today_events_empty() {
        let port = Arc::new(MockCalendarPort::new(vec![]));
        let service = CalendarService::new(port);

        let events = service.get_today_events().await.unwrap();
        assert!(events.is_empty());
    }

    #[tokio::test]
    async fn get_today_events_returns_events() {
        let port = Arc::new(MockCalendarPort::new(vec![sample_event()]));
        let service = CalendarService::new(port);

        let events = service.get_today_events().await.unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].title, "Meeting");
    }

    #[tokio::test]
    async fn get_next_event_returns_first() {
        let port = Arc::new(MockCalendarPort::new(vec![sample_event()]));
        let service = CalendarService::new(port);

        let event = service.get_next_event().await.unwrap();
        assert!(event.is_some());
        assert_eq!(event.unwrap().title, "Meeting");
    }

    #[tokio::test]
    async fn create_event_returns_id() {
        let port = Arc::new(MockCalendarPort::new(vec![]));
        let service = CalendarService::new(port);

        let date = NaiveDate::from_ymd_opt(2024, 1, 15).unwrap();
        let time = NaiveTime::from_hms_opt(10, 0, 0).unwrap();

        let id = service
            .create_event("Meeting", date, time, 60, Some("Room A"), None)
            .await
            .unwrap();

        assert_eq!(id, "evt-new");
    }

    #[tokio::test]
    async fn create_all_day_event_returns_id() {
        let port = Arc::new(MockCalendarPort::new(vec![]));
        let service = CalendarService::new(port);

        let date = NaiveDate::from_ymd_opt(2024, 1, 15).unwrap();

        let id = service
            .create_all_day_event("Holiday", date, None, Some("Day off"))
            .await
            .unwrap();

        assert_eq!(id, "evt-new");
    }

    #[tokio::test]
    async fn delete_event_succeeds() {
        let port = Arc::new(MockCalendarPort::new(vec![]));
        let service = CalendarService::new(port);

        let result = service.delete_event("evt-1").await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn get_calendar_brief_empty() {
        let port = Arc::new(MockCalendarPort::new(vec![]));
        let service = CalendarService::new(port);

        let date = NaiveDate::from_ymd_opt(2024, 1, 15).unwrap();
        let brief = service.get_calendar_brief(date).await.unwrap();

        assert_eq!(brief.event_count, 0);
        assert!(brief.events.is_empty());
    }

    #[tokio::test]
    async fn get_calendar_brief_with_events() {
        let port = Arc::new(MockCalendarPort::new(vec![sample_event()]));
        let service = CalendarService::new(port);

        let date = NaiveDate::from_ymd_opt(2024, 1, 15).unwrap();
        let brief = service.get_calendar_brief(date).await.unwrap();

        assert_eq!(brief.event_count, 1);
        assert!(brief.next_event.is_some());
    }

    #[tokio::test]
    async fn is_available_returns_true() {
        let port = Arc::new(MockCalendarPort::new(vec![]));
        let service = CalendarService::new(port);

        assert!(service.is_available().await);
    }

    #[tokio::test]
    async fn list_calendars_returns_list() {
        let port = Arc::new(MockCalendarPort::new(vec![]));
        let service = CalendarService::new(port);

        let calendars = service.list_calendars().await.unwrap();
        assert!(calendars.contains(&"Personal".to_string()));
    }

    #[test]
    fn extract_time_from_rfc3339() {
        let time = extract_time("2024-01-15T10:30:00+00:00");
        assert_eq!(time, "10:30");
    }

    #[test]
    fn extract_time_from_iso() {
        let time = extract_time("2024-01-15T10:30:00");
        assert_eq!(time, "10:30");
    }

    #[test]
    fn extract_time_all_day() {
        let time = extract_time("2024-01-15");
        assert_eq!(time, "all-day");
    }

    #[test]
    fn detect_conflicts_none() {
        let events = vec![
            CalendarEvent::new(
                "1",
                "E1",
                "2024-01-15T10:00:00+00:00",
                "2024-01-15T11:00:00+00:00",
            ),
            CalendarEvent::new(
                "2",
                "E2",
                "2024-01-15T11:00:00+00:00",
                "2024-01-15T12:00:00+00:00",
            ),
        ];
        let conflicts = detect_conflicts(&events);
        assert!(conflicts.is_empty());
    }

    #[test]
    fn detect_conflicts_overlap() {
        let events = vec![
            CalendarEvent::new(
                "1",
                "E1",
                "2024-01-15T10:00:00+00:00",
                "2024-01-15T11:30:00+00:00",
            ),
            CalendarEvent::new(
                "2",
                "E2",
                "2024-01-15T11:00:00+00:00",
                "2024-01-15T12:00:00+00:00",
            ),
        ];
        let conflicts = detect_conflicts(&events);
        assert_eq!(conflicts.len(), 1);
        assert!(conflicts[0].contains("overlap"));
    }

    #[test]
    fn detect_conflicts_skips_all_day() {
        let events = vec![
            CalendarEvent::new("1", "E1", "2024-01-15", "2024-01-15").as_all_day(),
            CalendarEvent::new(
                "2",
                "E2",
                "2024-01-15T10:00:00+00:00",
                "2024-01-15T11:00:00+00:00",
            ),
        ];
        let conflicts = detect_conflicts(&events);
        assert!(conflicts.is_empty());
    }

    #[test]
    fn times_overlap_yes() {
        assert!(times_overlap(
            "2024-01-15T10:00:00+00:00",
            "2024-01-15T11:30:00+00:00",
            "2024-01-15T11:00:00+00:00",
            "2024-01-15T12:00:00+00:00"
        ));
    }

    #[test]
    fn times_overlap_no() {
        assert!(!times_overlap(
            "2024-01-15T10:00:00+00:00",
            "2024-01-15T11:00:00+00:00",
            "2024-01-15T11:00:00+00:00",
            "2024-01-15T12:00:00+00:00"
        ));
    }

    #[test]
    fn map_error_service_unavailable() {
        let err = map_error(CalendarError::ServiceUnavailable);
        assert!(matches!(err, ApplicationError::ExternalService(_)));
    }

    #[test]
    fn map_error_auth_failed() {
        let err = map_error(CalendarError::AuthenticationFailed);
        assert!(matches!(err, ApplicationError::NotAuthorized(_)));
    }

    #[test]
    fn map_error_event_not_found() {
        let err = map_error(CalendarError::EventNotFound("evt-1".to_string()));
        assert!(matches!(err, ApplicationError::ExternalService(_)));
    }

    #[test]
    fn map_error_invalid_datetime() {
        let err = map_error(CalendarError::InvalidDateTime("bad".to_string()));
        assert!(matches!(err, ApplicationError::CommandFailed(_)));
    }
}
