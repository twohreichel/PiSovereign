//! CalDAV task (VTODO) support
//!
//! Provides types and operations for CalDAV tasks using the VTODO component.

use std::fmt::Write as _;

use async_trait::async_trait;
use chrono::{DateTime, NaiveDate, Utc};
use icalendar::{CalendarComponent, Component, parser};
use reqwest::StatusCode;
use serde::{Deserialize, Serialize};
use tracing::{debug, instrument};

use crate::client::{CalDavError, HttpCalDavClient};

/// Task priority levels
///
/// Maps to iCalendar PRIORITY values:
/// - High: 1-3
/// - Medium: 4-6
/// - Low: 7-9
/// - None: 0 or absent
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum TaskPriority {
    /// High priority (urgent)
    High,
    /// Medium priority (normal)
    Medium,
    /// Low priority (can wait)
    #[default]
    Low,
}

impl TaskPriority {
    /// Convert from iCalendar PRIORITY value (0-9)
    ///
    /// RFC 5545 specifies:
    /// - 0: undefined
    /// - 1: highest
    /// - 9: lowest
    #[must_use]
    pub const fn from_ical_priority(priority: u8) -> Self {
        match priority {
            1..=3 => Self::High,
            4..=6 => Self::Medium,
            _ => Self::Low,
        }
    }

    /// Convert to iCalendar PRIORITY value
    #[must_use]
    pub const fn to_ical_priority(self) -> u8 {
        match self {
            Self::High => 1,
            Self::Medium => 5,
            Self::Low => 9,
        }
    }
}

impl std::fmt::Display for TaskPriority {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::High => write!(f, "high"),
            Self::Medium => write!(f, "medium"),
            Self::Low => write!(f, "low"),
        }
    }
}

/// Task status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "SCREAMING-KEBAB-CASE")]
pub enum TaskStatus {
    /// Task needs action
    #[default]
    NeedsAction,
    /// Task is in progress
    InProgress,
    /// Task is completed
    Completed,
    /// Task is cancelled
    Cancelled,
}

impl TaskStatus {
    /// Parse from iCalendar STATUS value
    #[must_use]
    pub fn from_ical_status(status: &str) -> Self {
        match status.to_uppercase().as_str() {
            "COMPLETED" => Self::Completed,
            "IN-PROGRESS" => Self::InProgress,
            "CANCELLED" => Self::Cancelled,
            _ => Self::NeedsAction,
        }
    }

    /// Convert to iCalendar STATUS value
    #[must_use]
    pub const fn to_ical_status(self) -> &'static str {
        match self {
            Self::NeedsAction => "NEEDS-ACTION",
            Self::InProgress => "IN-PROGRESS",
            Self::Completed => "COMPLETED",
            Self::Cancelled => "CANCELLED",
        }
    }

    /// Check if task is complete
    #[must_use]
    pub const fn is_complete(self) -> bool {
        matches!(self, Self::Completed | Self::Cancelled)
    }
}

impl std::fmt::Display for TaskStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.to_ical_status())
    }
}

/// A calendar task (VTODO component)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CalendarTask {
    /// Unique task ID (UID)
    pub id: String,
    /// Task summary/title
    pub summary: String,
    /// Task description
    pub description: Option<String>,
    /// Due date
    pub due: Option<NaiveDate>,
    /// Due date-time (if time is specified)
    pub due_datetime: Option<DateTime<Utc>>,
    /// Start date
    pub start: Option<NaiveDate>,
    /// Priority level
    pub priority: TaskPriority,
    /// Task status
    pub status: TaskStatus,
    /// Percent complete (0-100)
    pub percent_complete: u8,
    /// Categories/tags
    pub categories: Vec<String>,
    /// Parent task ID (for subtasks)
    pub parent_id: Option<String>,
    /// Creation timestamp
    pub created: Option<DateTime<Utc>>,
    /// Last modified timestamp
    pub last_modified: Option<DateTime<Utc>>,
    /// Completion timestamp
    pub completed: Option<DateTime<Utc>>,
}

impl CalendarTask {
    /// Create a new task with required fields
    #[must_use]
    pub fn new(id: impl Into<String>, summary: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            summary: summary.into(),
            description: None,
            due: None,
            due_datetime: None,
            start: None,
            priority: TaskPriority::default(),
            status: TaskStatus::default(),
            percent_complete: 0,
            categories: Vec::new(),
            parent_id: None,
            created: None,
            last_modified: None,
            completed: None,
        }
    }

    /// Check if task is overdue
    #[must_use]
    pub fn is_overdue(&self) -> bool {
        if self.status.is_complete() {
            return false;
        }

        if let Some(due) = self.due {
            let today = Utc::now().date_naive();
            return due < today;
        }

        if let Some(due_dt) = self.due_datetime {
            return due_dt < Utc::now();
        }

        false
    }

    /// Check if task is due today
    #[must_use]
    pub fn is_due_today(&self) -> bool {
        if self.status.is_complete() {
            return false;
        }

        let today = Utc::now().date_naive();

        if let Some(due) = self.due {
            return due == today;
        }

        if let Some(due_dt) = self.due_datetime {
            return due_dt.date_naive() == today;
        }

        false
    }

    /// Builder: set description
    #[must_use]
    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    /// Builder: set due date
    #[must_use]
    pub const fn with_due(mut self, due: NaiveDate) -> Self {
        self.due = Some(due);
        self
    }

    /// Builder: set priority
    #[must_use]
    pub const fn with_priority(mut self, priority: TaskPriority) -> Self {
        self.priority = priority;
        self
    }

    /// Builder: set status
    #[must_use]
    pub const fn with_status(mut self, status: TaskStatus) -> Self {
        self.status = status;
        self
    }

    /// Builder: add category
    #[must_use]
    pub fn with_category(mut self, category: impl Into<String>) -> Self {
        self.categories.push(category.into());
        self
    }
}

/// CalDAV task client trait
#[async_trait]
pub trait CalDavTaskClient: Send + Sync {
    /// List all tasks from a calendar
    async fn list_tasks(&self, calendar: &str) -> Result<Vec<CalendarTask>, CalDavError>;

    /// Get tasks due within a date range
    async fn get_tasks_in_range(
        &self,
        calendar: &str,
        start: NaiveDate,
        end: NaiveDate,
    ) -> Result<Vec<CalendarTask>, CalDavError>;

    /// Get overdue tasks
    async fn get_overdue_tasks(&self, calendar: &str) -> Result<Vec<CalendarTask>, CalDavError>;

    /// Get tasks due today
    async fn get_tasks_due_today(&self, calendar: &str) -> Result<Vec<CalendarTask>, CalDavError>;

    /// Get high priority incomplete tasks
    async fn get_high_priority_tasks(
        &self,
        calendar: &str,
    ) -> Result<Vec<CalendarTask>, CalDavError>;

    /// Create a new task
    async fn create_task(&self, calendar: &str, task: &CalendarTask)
    -> Result<String, CalDavError>;

    /// Update an existing task
    async fn update_task(&self, calendar: &str, task: &CalendarTask) -> Result<(), CalDavError>;

    /// Complete a task
    async fn complete_task(&self, calendar: &str, task_id: &str) -> Result<(), CalDavError>;

    /// Delete a task
    async fn delete_task(&self, calendar: &str, task_id: &str) -> Result<(), CalDavError>;
}

impl HttpCalDavClient {
    /// Parse VTODO components from iCalendar data
    #[allow(clippy::unused_self)]
    pub(crate) fn parse_vtodo(&self, ical_data: &str) -> Result<Vec<CalendarTask>, CalDavError> {
        let calendars = parser::unfold(ical_data);
        let parsed = parser::read_calendar(&calendars)
            .map_err(|e| CalDavError::ParseError(format!("iCalendar parse error: {e}")))?;

        let mut tasks = Vec::new();

        for component in parsed.components {
            let cal_component = CalendarComponent::from(component);

            if let CalendarComponent::Todo(todo) = cal_component {
                let id = todo.get_uid().unwrap_or_default().to_string();
                let summary = todo.get_summary().unwrap_or_default().to_string();
                let description = todo.get_description().map(ToString::to_string);

                // Parse priority
                let priority = todo
                    .property_value("PRIORITY")
                    .and_then(|p| p.parse::<u8>().ok())
                    .map(TaskPriority::from_ical_priority)
                    .unwrap_or_default();

                // Parse status
                let status = todo
                    .property_value("STATUS")
                    .map(TaskStatus::from_ical_status)
                    .unwrap_or_default();

                // Parse percent complete
                let percent_complete = todo
                    .property_value("PERCENT-COMPLETE")
                    .and_then(|p| p.parse::<u8>().ok())
                    .unwrap_or(0);

                // Parse due date
                let (due, due_datetime) = Self::parse_due_date(&todo);

                // Parse start date
                let start = todo
                    .property_value("DTSTART")
                    .and_then(Self::parse_date_value);

                // Parse categories
                let categories: Vec<String> = todo
                    .property_value("CATEGORIES")
                    .map(|c| c.split(',').map(|s| s.trim().to_string()).collect())
                    .unwrap_or_default();

                // Parse parent (RELATED-TO)
                let parent_id = todo.property_value("RELATED-TO").map(ToString::to_string);

                // Parse timestamps
                let created = todo
                    .property_value("CREATED")
                    .and_then(Self::parse_datetime_value);
                let last_modified = todo
                    .property_value("LAST-MODIFIED")
                    .and_then(Self::parse_datetime_value);
                let completed = todo
                    .property_value("COMPLETED")
                    .and_then(Self::parse_datetime_value);

                if !id.is_empty() && !summary.is_empty() {
                    tasks.push(CalendarTask {
                        id,
                        summary,
                        description,
                        due,
                        due_datetime,
                        start,
                        priority,
                        status,
                        percent_complete,
                        categories,
                        parent_id,
                        created,
                        last_modified,
                        completed,
                    });
                }
            }
        }

        Ok(tasks)
    }

    /// Parse DUE property which can be date or datetime
    fn parse_due_date(todo: &icalendar::Todo) -> (Option<NaiveDate>, Option<DateTime<Utc>>) {
        if let Some(due_str) = todo.property_value("DUE") {
            // Check if it's a date-only value (8 chars: YYYYMMDD)
            if due_str.len() == 8 {
                if let Some(date) = Self::parse_date_value(due_str) {
                    return (Some(date), None);
                }
            }
            // Try parsing as datetime
            if let Some(dt) = Self::parse_datetime_value(due_str) {
                return (Some(dt.date_naive()), Some(dt));
            }
        }
        (None, None)
    }

    /// Parse a date value (YYYYMMDD)
    fn parse_date_value(s: &str) -> Option<NaiveDate> {
        NaiveDate::parse_from_str(s, "%Y%m%d").ok()
    }

    /// Parse a datetime value (YYYYMMDDTHHMMSS or YYYYMMDDTHHMMSSZ)
    fn parse_datetime_value(s: &str) -> Option<DateTime<Utc>> {
        use chrono::TimeZone;

        // Try with Z suffix
        if let Ok(dt) = chrono::NaiveDateTime::parse_from_str(s, "%Y%m%dT%H%M%SZ") {
            return Some(Utc.from_utc_datetime(&dt));
        }

        // Try without Z suffix (treat as UTC)
        if let Ok(dt) = chrono::NaiveDateTime::parse_from_str(s, "%Y%m%dT%H%M%S") {
            return Some(Utc.from_utc_datetime(&dt));
        }

        None
    }

    /// Build iCalendar VTODO from `CalendarTask`
    #[allow(clippy::unused_self)]
    fn build_vtodo(&self, task: &CalendarTask) -> String {
        let now: DateTime<Utc> = Utc::now();
        let dtstamp = now.format("%Y%m%dT%H%M%SZ").to_string();

        let mut ical = String::new();
        ical.push_str("BEGIN:VCALENDAR\r\n");
        ical.push_str("VERSION:2.0\r\n");
        ical.push_str("PRODID:-//PiSovereign//CalDAV Client//EN\r\n");
        ical.push_str("BEGIN:VTODO\r\n");
        let _ = writeln!(ical, "UID:{}\r", task.id);
        let _ = writeln!(ical, "DTSTAMP:{dtstamp}\r");
        let _ = writeln!(ical, "SUMMARY:{}\r", task.summary);

        if let Some(desc) = &task.description {
            let _ = writeln!(ical, "DESCRIPTION:{desc}\r");
        }

        // Due date
        if let Some(due_dt) = task.due_datetime {
            let _ = writeln!(ical, "DUE:{}\r", due_dt.format("%Y%m%dT%H%M%SZ"));
        } else if let Some(due) = task.due {
            let _ = writeln!(ical, "DUE;VALUE=DATE:{}\r", due.format("%Y%m%d"));
        }

        // Start date
        if let Some(start) = task.start {
            let _ = writeln!(ical, "DTSTART;VALUE=DATE:{}\r", start.format("%Y%m%d"));
        }

        // Priority
        let _ = writeln!(ical, "PRIORITY:{}\r", task.priority.to_ical_priority());

        // Status
        let _ = writeln!(ical, "STATUS:{}\r", task.status.to_ical_status());

        // Percent complete
        let _ = writeln!(ical, "PERCENT-COMPLETE:{}\r", task.percent_complete);

        // Categories
        if !task.categories.is_empty() {
            let _ = writeln!(ical, "CATEGORIES:{}\r", task.categories.join(","));
        }

        // Parent
        if let Some(parent) = &task.parent_id {
            let _ = writeln!(ical, "RELATED-TO:{parent}\r");
        }

        // Completed timestamp
        if let Some(completed) = task.completed {
            let _ = writeln!(ical, "COMPLETED:{}\r", completed.format("%Y%m%dT%H%M%SZ"));
        }

        ical.push_str("END:VTODO\r\n");
        ical.push_str("END:VCALENDAR\r\n");
        ical
    }

    /// Build the task URL
    fn task_url(&self, calendar: &str, task_id: &str) -> String {
        format!(
            "{}/{}.ics",
            self.calendar_url(calendar).trim_end_matches('/'),
            task_id
        )
    }
}

#[async_trait]
impl CalDavTaskClient for HttpCalDavClient {
    #[instrument(skip(self))]
    async fn list_tasks(&self, calendar: &str) -> Result<Vec<CalendarTask>, CalDavError> {
        let url = self.calendar_url(calendar);

        // REPORT request with calendar-query for VTODO
        let body = r#"<?xml version="1.0" encoding="utf-8" ?>
<C:calendar-query xmlns:D="DAV:" xmlns:C="urn:ietf:params:xml:ns:caldav">
  <D:prop>
    <D:getetag/>
    <C:calendar-data/>
  </D:prop>
  <C:filter>
    <C:comp-filter name="VCALENDAR">
      <C:comp-filter name="VTODO"/>
    </C:comp-filter>
  </C:filter>
</C:calendar-query>"#;

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

        debug!(
            response_len = body.len(),
            "REPORT response received for tasks"
        );

        // Extract iCalendar data using proper XML parsing
        let ical_data_list = Self::extract_calendar_data_from_xml(&body);

        let mut all_tasks = Vec::new();
        for ical_data in ical_data_list {
            if let Ok(tasks) = self.parse_vtodo(&ical_data) {
                all_tasks.extend(tasks);
            }
        }

        Ok(all_tasks)
    }

    #[instrument(skip(self))]
    async fn get_tasks_in_range(
        &self,
        calendar: &str,
        start: NaiveDate,
        end: NaiveDate,
    ) -> Result<Vec<CalendarTask>, CalDavError> {
        // Get all tasks and filter by date range
        let tasks = self.list_tasks(calendar).await?;

        Ok(tasks
            .into_iter()
            .filter(|t| t.due.is_some_and(|due| due >= start && due <= end))
            .collect())
    }

    #[instrument(skip(self))]
    async fn get_overdue_tasks(&self, calendar: &str) -> Result<Vec<CalendarTask>, CalDavError> {
        let tasks = self.list_tasks(calendar).await?;
        Ok(tasks.into_iter().filter(CalendarTask::is_overdue).collect())
    }

    #[instrument(skip(self))]
    async fn get_tasks_due_today(&self, calendar: &str) -> Result<Vec<CalendarTask>, CalDavError> {
        let tasks = self.list_tasks(calendar).await?;
        Ok(tasks
            .into_iter()
            .filter(CalendarTask::is_due_today)
            .collect())
    }

    #[instrument(skip(self))]
    async fn get_high_priority_tasks(
        &self,
        calendar: &str,
    ) -> Result<Vec<CalendarTask>, CalDavError> {
        let tasks = self.list_tasks(calendar).await?;
        Ok(tasks
            .into_iter()
            .filter(|t| t.priority == TaskPriority::High && !t.status.is_complete())
            .collect())
    }

    #[instrument(skip(self, task))]
    async fn create_task(
        &self,
        calendar: &str,
        task: &CalendarTask,
    ) -> Result<String, CalDavError> {
        let url = self.task_url(calendar, &task.id);
        let ical = self.build_vtodo(task);

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
                debug!(task_id = %task.id, "Task created successfully");
                Ok(task.id.clone())
            },
            status => Err(CalDavError::RequestFailed(format!("HTTP {status}"))),
        }
    }

    #[instrument(skip(self, task))]
    async fn update_task(&self, calendar: &str, task: &CalendarTask) -> Result<(), CalDavError> {
        // CalDAV uses PUT for both create and update
        self.create_task(calendar, task).await?;
        Ok(())
    }

    #[instrument(skip(self))]
    async fn complete_task(&self, calendar: &str, task_id: &str) -> Result<(), CalDavError> {
        // Get current task
        let tasks = self.list_tasks(calendar).await?;
        let task = tasks
            .into_iter()
            .find(|t| t.id == task_id)
            .ok_or_else(|| CalDavError::EventNotFound(task_id.to_string()))?;

        // Update to completed
        let updated_task = CalendarTask {
            status: TaskStatus::Completed,
            percent_complete: 100,
            completed: Some(Utc::now()),
            ..task
        };

        self.update_task(calendar, &updated_task).await
    }

    #[instrument(skip(self))]
    async fn delete_task(&self, calendar: &str, task_id: &str) -> Result<(), CalDavError> {
        let url = self.task_url(calendar, task_id);

        let response = self
            .client
            .delete(&url)
            .basic_auth(&self.config.username, Some(&self.config.password))
            .send()
            .await
            .map_err(|e| CalDavError::ConnectionFailed(e.to_string()))?;

        match response.status() {
            StatusCode::UNAUTHORIZED => Err(CalDavError::AuthenticationFailed),
            StatusCode::NOT_FOUND => Err(CalDavError::EventNotFound(task_id.to_string())),
            StatusCode::NO_CONTENT | StatusCode::OK => {
                debug!(task_id = %task_id, "Task deleted successfully");
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
    fn test_task_priority_from_ical() {
        assert_eq!(TaskPriority::from_ical_priority(1), TaskPriority::High);
        assert_eq!(TaskPriority::from_ical_priority(2), TaskPriority::High);
        assert_eq!(TaskPriority::from_ical_priority(3), TaskPriority::High);
        assert_eq!(TaskPriority::from_ical_priority(4), TaskPriority::Medium);
        assert_eq!(TaskPriority::from_ical_priority(5), TaskPriority::Medium);
        assert_eq!(TaskPriority::from_ical_priority(6), TaskPriority::Medium);
        assert_eq!(TaskPriority::from_ical_priority(7), TaskPriority::Low);
        assert_eq!(TaskPriority::from_ical_priority(8), TaskPriority::Low);
        assert_eq!(TaskPriority::from_ical_priority(9), TaskPriority::Low);
        assert_eq!(TaskPriority::from_ical_priority(0), TaskPriority::Low);
    }

    #[test]
    fn test_task_priority_to_ical() {
        assert_eq!(TaskPriority::High.to_ical_priority(), 1);
        assert_eq!(TaskPriority::Medium.to_ical_priority(), 5);
        assert_eq!(TaskPriority::Low.to_ical_priority(), 9);
    }

    #[test]
    fn test_task_priority_display() {
        assert_eq!(format!("{}", TaskPriority::High), "high");
        assert_eq!(format!("{}", TaskPriority::Medium), "medium");
        assert_eq!(format!("{}", TaskPriority::Low), "low");
    }

    #[test]
    fn test_task_status_from_ical() {
        assert_eq!(
            TaskStatus::from_ical_status("NEEDS-ACTION"),
            TaskStatus::NeedsAction
        );
        assert_eq!(
            TaskStatus::from_ical_status("IN-PROGRESS"),
            TaskStatus::InProgress
        );
        assert_eq!(
            TaskStatus::from_ical_status("COMPLETED"),
            TaskStatus::Completed
        );
        assert_eq!(
            TaskStatus::from_ical_status("CANCELLED"),
            TaskStatus::Cancelled
        );
        assert_eq!(
            TaskStatus::from_ical_status("unknown"),
            TaskStatus::NeedsAction
        );
    }

    #[test]
    fn test_task_status_to_ical() {
        assert_eq!(TaskStatus::NeedsAction.to_ical_status(), "NEEDS-ACTION");
        assert_eq!(TaskStatus::InProgress.to_ical_status(), "IN-PROGRESS");
        assert_eq!(TaskStatus::Completed.to_ical_status(), "COMPLETED");
        assert_eq!(TaskStatus::Cancelled.to_ical_status(), "CANCELLED");
    }

    #[test]
    fn test_task_status_is_complete() {
        assert!(!TaskStatus::NeedsAction.is_complete());
        assert!(!TaskStatus::InProgress.is_complete());
        assert!(TaskStatus::Completed.is_complete());
        assert!(TaskStatus::Cancelled.is_complete());
    }

    #[test]
    fn test_calendar_task_new() {
        let task = CalendarTask::new("task-123", "Buy groceries");
        assert_eq!(task.id, "task-123");
        assert_eq!(task.summary, "Buy groceries");
        assert_eq!(task.priority, TaskPriority::Low);
        assert_eq!(task.status, TaskStatus::NeedsAction);
        assert_eq!(task.percent_complete, 0);
        assert!(task.description.is_none());
        assert!(task.due.is_none());
    }

    #[test]
    fn test_calendar_task_builder() {
        let due_date = NaiveDate::from_ymd_opt(2026, 2, 10).expect("valid date");
        let task = CalendarTask::new("task-456", "Finish report")
            .with_description("Complete the quarterly report")
            .with_due(due_date)
            .with_priority(TaskPriority::High)
            .with_status(TaskStatus::InProgress)
            .with_category("work");

        assert_eq!(task.id, "task-456");
        assert_eq!(task.summary, "Finish report");
        assert_eq!(
            task.description,
            Some("Complete the quarterly report".to_string())
        );
        assert_eq!(task.due, Some(due_date));
        assert_eq!(task.priority, TaskPriority::High);
        assert_eq!(task.status, TaskStatus::InProgress);
        assert_eq!(task.categories, vec!["work".to_string()]);
    }

    #[test]
    fn test_task_is_overdue() {
        let yesterday = Utc::now().date_naive() - chrono::Duration::days(1);
        let tomorrow = Utc::now().date_naive() + chrono::Duration::days(1);

        let overdue_task = CalendarTask::new("1", "Overdue").with_due(yesterday);
        assert!(overdue_task.is_overdue());

        let future_task = CalendarTask::new("2", "Future").with_due(tomorrow);
        assert!(!future_task.is_overdue());

        let completed_task = CalendarTask::new("3", "Done")
            .with_due(yesterday)
            .with_status(TaskStatus::Completed);
        assert!(!completed_task.is_overdue());

        let no_due_task = CalendarTask::new("4", "No due");
        assert!(!no_due_task.is_overdue());
    }

    #[test]
    fn test_task_is_due_today() {
        let today = Utc::now().date_naive();
        let yesterday = today - chrono::Duration::days(1);
        let tomorrow = today + chrono::Duration::days(1);

        let today_task = CalendarTask::new("1", "Today").with_due(today);
        assert!(today_task.is_due_today());

        let yesterday_task = CalendarTask::new("2", "Yesterday").with_due(yesterday);
        assert!(!yesterday_task.is_due_today());

        let tomorrow_task = CalendarTask::new("3", "Tomorrow").with_due(tomorrow);
        assert!(!tomorrow_task.is_due_today());

        let completed_task = CalendarTask::new("4", "Done")
            .with_due(today)
            .with_status(TaskStatus::Completed);
        assert!(!completed_task.is_due_today());
    }

    #[test]
    fn test_parse_date_value() {
        let date = HttpCalDavClient::parse_date_value("20260210");
        assert_eq!(
            date,
            Some(NaiveDate::from_ymd_opt(2026, 2, 10).expect("valid date"))
        );

        let invalid = HttpCalDavClient::parse_date_value("invalid");
        assert!(invalid.is_none());
    }

    #[test]
    fn test_parse_datetime_value() {
        let dt = HttpCalDavClient::parse_datetime_value("20260210T143000Z");
        assert!(dt.is_some());
        let dt = dt.expect("parsed datetime");
        assert_eq!(
            dt.format("%Y-%m-%d %H:%M:%S").to_string(),
            "2026-02-10 14:30:00"
        );

        let dt_no_z = HttpCalDavClient::parse_datetime_value("20260210T143000");
        assert!(dt_no_z.is_some());

        let invalid = HttpCalDavClient::parse_datetime_value("invalid");
        assert!(invalid.is_none());
    }

    #[test]
    fn test_task_serialization() {
        let task = CalendarTask::new("task-123", "Test task")
            .with_priority(TaskPriority::High)
            .with_status(TaskStatus::InProgress);

        let json = serde_json::to_string(&task).expect("serialization should work");
        assert!(json.contains("\"priority\":\"high\""));
        assert!(json.contains("\"status\":\"IN-PROGRESS\""));

        let deserialized: CalendarTask =
            serde_json::from_str(&json).expect("deserialization should work");
        assert_eq!(deserialized.id, "task-123");
        assert_eq!(deserialized.priority, TaskPriority::High);
        assert_eq!(deserialized.status, TaskStatus::InProgress);
    }
}
