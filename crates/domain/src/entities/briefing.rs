//! Briefing entities
//!
//! Structures for morning briefings with calendar, email, tasks, and weather.

use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};

use crate::value_objects::Priority;

/// Summary of weather conditions for briefing
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WeatherSummary {
    /// Current temperature in Celsius
    pub temperature: f32,
    /// Feels like temperature in Celsius
    pub feels_like: f32,
    /// Weather condition description
    pub condition: String,
    /// Weather condition emoji
    pub emoji: String,
    /// Daily high temperature
    pub high: f32,
    /// Daily low temperature
    pub low: f32,
    /// Humidity percentage
    pub humidity: u8,
    /// Wind speed in km/h
    pub wind_speed: f32,
    /// Precipitation probability (0-100)
    pub precipitation_chance: Option<u8>,
    /// UV index
    pub uv_index: f32,
    /// Brief forecast summary
    pub forecast_summary: Option<String>,
}

impl WeatherSummary {
    /// Create a new weather summary
    #[must_use]
    pub fn new(temperature: f32, condition: impl Into<String>, high: f32, low: f32) -> Self {
        Self {
            temperature,
            feels_like: temperature,
            condition: condition.into(),
            emoji: "🌤️".to_string(),
            high,
            low,
            humidity: 50,
            wind_speed: 0.0,
            precipitation_chance: None,
            uv_index: 0.0,
            forecast_summary: None,
        }
    }

    /// Get a formatted temperature string
    #[must_use]
    pub fn temperature_display(&self) -> String {
        format!("{:.0}°C", self.temperature)
    }

    /// Get a formatted high/low string
    #[must_use]
    pub fn high_low_display(&self) -> String {
        format!("{:.0}°C / {:.0}°C", self.high, self.low)
    }

    /// Get a concise summary
    #[must_use]
    pub fn summary(&self) -> String {
        format!(
            "{} {} {} (H: {:.0}° L: {:.0}°)",
            self.emoji,
            self.condition,
            self.temperature_display(),
            self.high,
            self.low
        )
    }
}

impl Default for WeatherSummary {
    fn default() -> Self {
        Self {
            temperature: 20.0,
            feels_like: 20.0,
            condition: "Unknown".to_string(),
            emoji: "❓".to_string(),
            high: 20.0,
            low: 10.0,
            humidity: 50,
            wind_speed: 0.0,
            precipitation_chance: None,
            uv_index: 0.0,
            forecast_summary: None,
        }
    }
}

/// A single task item for briefing
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskItem {
    /// Task ID
    pub id: String,
    /// Task title
    pub title: String,
    /// Task priority
    pub priority: Priority,
    /// Due date
    pub due: Option<NaiveDate>,
    /// Whether the task is overdue
    pub is_overdue: bool,
}

impl TaskItem {
    /// Create a new task item
    #[must_use]
    pub fn new(id: impl Into<String>, title: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            title: title.into(),
            priority: Priority::default(),
            due: None,
            is_overdue: false,
        }
    }

    /// Set the priority
    #[must_use]
    pub const fn with_priority(mut self, priority: Priority) -> Self {
        self.priority = priority;
        self
    }

    /// Set the due date
    #[must_use]
    pub const fn with_due(mut self, due: NaiveDate) -> Self {
        self.due = Some(due);
        self
    }

    /// Mark as overdue
    #[must_use]
    pub const fn overdue(mut self) -> Self {
        self.is_overdue = true;
        self
    }
}

/// Summary of tasks for briefing
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TaskBrief {
    /// Number of tasks due today
    pub due_today: u32,
    /// Number of overdue tasks
    pub overdue: u32,
    /// High priority tasks that need attention
    pub high_priority: Vec<TaskItem>,
    /// Tasks due today (up to limit)
    pub today_tasks: Vec<TaskItem>,
    /// Overdue tasks (up to limit)
    pub overdue_tasks: Vec<TaskItem>,
}

impl TaskBrief {
    /// Create a new task brief
    #[must_use]
    pub const fn new() -> Self {
        Self {
            due_today: 0,
            overdue: 0,
            high_priority: Vec::new(),
            today_tasks: Vec::new(),
            overdue_tasks: Vec::new(),
        }
    }

    /// Check if there are any tasks requiring attention
    #[must_use]
    pub fn has_attention_items(&self) -> bool {
        self.overdue > 0 || !self.high_priority.is_empty()
    }

    /// Get total count of items needing attention
    #[must_use]
    pub fn attention_count(&self) -> u32 {
        self.overdue + u32::try_from(self.high_priority.len()).unwrap_or(0)
    }

    /// Get a summary string
    #[must_use]
    pub fn summary(&self) -> String {
        let mut parts = Vec::new();

        if self.due_today > 0 {
            parts.push(format!("{} due today", self.due_today));
        }
        if self.overdue > 0 {
            parts.push(format!("{} overdue", self.overdue));
        }
        if !self.high_priority.is_empty() {
            parts.push(format!("{} high priority", self.high_priority.len()));
        }

        if parts.is_empty() {
            "No tasks requiring attention".to_string()
        } else {
            parts.join(", ")
        }
    }
}

/// Summary of calendar events for briefing
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CalendarBrief {
    /// Total events today
    pub event_count: u32,
    /// Events happening now or very soon
    pub upcoming: Vec<CalendarItem>,
    /// First event of the day
    pub first_event: Option<CalendarItem>,
    /// Next free slot duration in minutes
    pub next_free_minutes: Option<u32>,
}

/// A calendar item for briefing
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CalendarItem {
    /// Event title
    pub title: String,
    /// Start time
    pub start: DateTime<Utc>,
    /// End time
    pub end: DateTime<Utc>,
    /// Location if any
    pub location: Option<String>,
    /// Whether the event is happening now
    pub is_now: bool,
}

impl CalendarItem {
    /// Create a new calendar item
    #[must_use]
    pub fn new(title: impl Into<String>, start: DateTime<Utc>, end: DateTime<Utc>) -> Self {
        Self {
            title: title.into(),
            start,
            end,
            location: None,
            is_now: false,
        }
    }

    /// Get the duration in minutes
    #[must_use]
    pub fn duration_minutes(&self) -> i64 {
        (self.end - self.start).num_minutes()
    }
}

impl CalendarBrief {
    /// Create a new calendar brief
    #[must_use]
    pub const fn new() -> Self {
        Self {
            event_count: 0,
            upcoming: Vec::new(),
            first_event: None,
            next_free_minutes: None,
        }
    }

    /// Check if there are events today
    #[must_use]
    pub const fn has_events(&self) -> bool {
        self.event_count > 0
    }
}

/// Summary of emails for briefing
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct EmailBrief {
    /// Total unread emails
    pub unread_count: u32,
    /// High priority/flagged emails
    pub priority_count: u32,
    /// Recent important senders
    pub important_senders: Vec<String>,
    /// Brief summary of important emails
    pub summary: Option<String>,
}

impl EmailBrief {
    /// Create a new email brief
    #[must_use]
    pub const fn new() -> Self {
        Self {
            unread_count: 0,
            priority_count: 0,
            important_senders: Vec::new(),
            summary: None,
        }
    }

    /// Check if there are unread emails
    #[must_use]
    pub const fn has_unread(&self) -> bool {
        self.unread_count > 0
    }
}

/// Complete morning briefing
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MorningBriefing {
    /// Briefing generation time
    pub generated_at: DateTime<Utc>,
    /// Date the briefing is for
    pub briefing_date: NaiveDate,
    /// Weather information
    pub weather: Option<WeatherSummary>,
    /// Calendar summary
    pub calendar: CalendarBrief,
    /// Email summary
    pub email: EmailBrief,
    /// Task summary
    pub tasks: TaskBrief,
    /// AI-generated summary text
    pub ai_summary: Option<String>,
}

impl MorningBriefing {
    /// Create a new briefing for today
    #[must_use]
    pub fn new() -> Self {
        Self {
            generated_at: Utc::now(),
            briefing_date: Utc::now().date_naive(),
            weather: None,
            calendar: CalendarBrief::new(),
            email: EmailBrief::new(),
            tasks: TaskBrief::new(),
            ai_summary: None,
        }
    }

    /// Create a briefing for a specific date
    #[must_use]
    pub fn for_date(date: NaiveDate) -> Self {
        Self {
            generated_at: Utc::now(),
            briefing_date: date,
            weather: None,
            calendar: CalendarBrief::new(),
            email: EmailBrief::new(),
            tasks: TaskBrief::new(),
            ai_summary: None,
        }
    }

    /// Set weather information
    #[must_use]
    pub fn with_weather(mut self, weather: WeatherSummary) -> Self {
        self.weather = Some(weather);
        self
    }

    /// Set calendar information
    #[must_use]
    pub fn with_calendar(mut self, calendar: CalendarBrief) -> Self {
        self.calendar = calendar;
        self
    }

    /// Set email information
    #[must_use]
    pub fn with_email(mut self, email: EmailBrief) -> Self {
        self.email = email;
        self
    }

    /// Set task information
    #[must_use]
    pub fn with_tasks(mut self, tasks: TaskBrief) -> Self {
        self.tasks = tasks;
        self
    }

    /// Set AI summary
    #[must_use]
    pub fn with_ai_summary(mut self, summary: impl Into<String>) -> Self {
        self.ai_summary = Some(summary.into());
        self
    }

    /// Check if there are items requiring attention
    #[must_use]
    pub fn has_attention_items(&self) -> bool {
        self.tasks.has_attention_items()
            || self.email.priority_count > 0
            || self.calendar.upcoming.iter().any(|e| e.is_now)
    }
}

impl Default for MorningBriefing {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_weather_summary_new() {
        let weather = WeatherSummary::new(20.5, "Sunny", 25.0, 15.0);
        assert!((weather.temperature - 20.5).abs() < f32::EPSILON);
        assert_eq!(weather.condition, "Sunny");
    }

    #[test]
    fn test_weather_summary_display() {
        let weather = WeatherSummary::new(20.5, "Sunny", 25.0, 15.0);
        // f32 to string with "{:.0}" rounds, so 20.5 becomes "20"
        assert!(
            weather.temperature_display().contains("20")
                || weather.temperature_display().contains("21")
        );
        assert!(weather.high_low_display().contains("25"));
    }

    #[test]
    fn test_task_item_builder() {
        let today = Utc::now().date_naive();
        let task = TaskItem::new("1", "Test task")
            .with_priority(Priority::High)
            .with_due(today)
            .overdue();

        assert_eq!(task.id, "1");
        assert_eq!(task.priority, Priority::High);
        assert_eq!(task.due, Some(today));
        assert!(task.is_overdue);
    }

    #[test]
    fn test_task_brief_summary() {
        let brief = TaskBrief {
            due_today: 3,
            overdue: 1,
            high_priority: vec![TaskItem::new("1", "Important")],
            today_tasks: vec![],
            overdue_tasks: vec![],
        };

        let summary = brief.summary();
        assert!(summary.contains("3 due today"));
        assert!(summary.contains("1 overdue"));
        assert!(summary.contains("1 high priority"));
    }

    #[test]
    fn test_task_brief_attention() {
        let mut brief = TaskBrief::new();
        assert!(!brief.has_attention_items());
        assert_eq!(brief.attention_count(), 0);

        brief.overdue = 2;
        assert!(brief.has_attention_items());
        assert_eq!(brief.attention_count(), 2);
    }

    #[test]
    fn test_calendar_item_duration() {
        let start = Utc::now();
        let end = start + chrono::Duration::hours(1);
        let item = CalendarItem::new("Meeting", start, end);

        assert_eq!(item.duration_minutes(), 60);
    }

    #[test]
    fn test_morning_briefing_builder() {
        let briefing = MorningBriefing::new()
            .with_weather(WeatherSummary::default())
            .with_ai_summary("Good morning!");

        assert!(briefing.weather.is_some());
        assert_eq!(briefing.ai_summary, Some("Good morning!".to_string()));
    }

    #[test]
    fn test_morning_briefing_attention() {
        let mut briefing = MorningBriefing::new();
        assert!(!briefing.has_attention_items());

        briefing.tasks.overdue = 1;
        assert!(briefing.has_attention_items());
    }

    #[test]
    fn test_morning_briefing_serialization() {
        let briefing =
            MorningBriefing::new().with_weather(WeatherSummary::new(20.0, "Sunny", 25.0, 15.0));

        let json = serde_json::to_string(&briefing).expect("serialize");
        let deserialized: MorningBriefing = serde_json::from_str(&json).expect("deserialize");

        assert!(deserialized.weather.is_some());
        assert_eq!(deserialized.briefing_date, briefing.briefing_date);
    }

    // === Additional tests for coverage ===

    #[test]
    fn test_weather_summary_default() {
        let weather = WeatherSummary::default();
        assert!((weather.temperature - 20.0).abs() < f32::EPSILON);
        assert_eq!(weather.condition, "Unknown");
        assert_eq!(weather.emoji, "❓");
    }

    #[test]
    fn test_weather_summary_summary() {
        let weather = WeatherSummary::new(22.0, "Cloudy", 26.0, 18.0);
        let summary = weather.summary();
        assert!(summary.contains("🌤️"));
        assert!(summary.contains("Cloudy"));
    }

    #[test]
    fn test_task_item_new_defaults() {
        let task = TaskItem::new("task-1", "Do something");
        assert_eq!(task.id, "task-1");
        assert_eq!(task.title, "Do something");
        assert_eq!(task.priority, Priority::Low);
        assert!(task.due.is_none());
        assert!(!task.is_overdue);
    }

    #[test]
    fn test_task_brief_new() {
        let brief = TaskBrief::new();
        assert_eq!(brief.due_today, 0);
        assert_eq!(brief.overdue, 0);
        assert!(brief.high_priority.is_empty());
        assert!(brief.today_tasks.is_empty());
        assert!(brief.overdue_tasks.is_empty());
    }

    #[test]
    fn test_task_brief_default() {
        let brief = TaskBrief::default();
        assert_eq!(brief.due_today, 0);
    }

    #[test]
    fn test_task_brief_summary_empty() {
        let brief = TaskBrief::new();
        let summary = brief.summary();
        assert_eq!(summary, "No tasks requiring attention");
    }

    #[test]
    fn test_calendar_brief_new() {
        let brief = CalendarBrief::new();
        assert_eq!(brief.event_count, 0);
        assert!(!brief.has_events());
        assert!(brief.upcoming.is_empty());
        assert!(brief.first_event.is_none());
    }

    #[test]
    fn test_calendar_brief_default() {
        let brief = CalendarBrief::default();
        assert_eq!(brief.event_count, 0);
    }

    #[test]
    fn test_calendar_brief_has_events() {
        let mut brief = CalendarBrief::new();
        assert!(!brief.has_events());
        brief.event_count = 5;
        assert!(brief.has_events());
    }

    #[test]
    fn test_calendar_item_with_location() {
        let start = Utc::now();
        let end = start + chrono::Duration::minutes(30);
        let mut item = CalendarItem::new("Standup", start, end);
        item.location = Some("Room A".to_string());
        item.is_now = true;

        assert_eq!(item.location, Some("Room A".to_string()));
        assert!(item.is_now);
        assert_eq!(item.duration_minutes(), 30);
    }

    #[test]
    fn test_email_brief_new() {
        let brief = EmailBrief::new();
        assert_eq!(brief.unread_count, 0);
        assert_eq!(brief.priority_count, 0);
        assert!(!brief.has_unread());
    }

    #[test]
    fn test_email_brief_default() {
        let brief = EmailBrief::default();
        assert_eq!(brief.unread_count, 0);
    }

    #[test]
    fn test_email_brief_has_unread() {
        let mut brief = EmailBrief::new();
        assert!(!brief.has_unread());
        brief.unread_count = 10;
        assert!(brief.has_unread());
    }

    #[test]
    fn test_morning_briefing_for_date() {
        let date = NaiveDate::from_ymd_opt(2026, 5, 15).unwrap();
        let briefing = MorningBriefing::for_date(date);
        assert_eq!(briefing.briefing_date, date);
    }

    #[test]
    fn test_morning_briefing_default() {
        let briefing = MorningBriefing::default();
        assert!(briefing.weather.is_none());
        assert!(briefing.ai_summary.is_none());
    }

    #[test]
    fn test_morning_briefing_with_calendar() {
        let mut calendar = CalendarBrief::new();
        calendar.event_count = 3;

        let briefing = MorningBriefing::new().with_calendar(calendar);
        assert_eq!(briefing.calendar.event_count, 3);
    }

    #[test]
    fn test_morning_briefing_with_email() {
        let mut email = EmailBrief::new();
        email.unread_count = 15;

        let briefing = MorningBriefing::new().with_email(email);
        assert_eq!(briefing.email.unread_count, 15);
    }

    #[test]
    fn test_morning_briefing_with_tasks() {
        let mut tasks = TaskBrief::new();
        tasks.due_today = 5;

        let briefing = MorningBriefing::new().with_tasks(tasks);
        assert_eq!(briefing.tasks.due_today, 5);
    }

    #[test]
    fn test_morning_briefing_has_attention_priority_email() {
        let mut briefing = MorningBriefing::new();
        briefing.email.priority_count = 2;
        assert!(briefing.has_attention_items());
    }

    #[test]
    fn test_morning_briefing_has_attention_calendar_now() {
        let mut briefing = MorningBriefing::new();
        let start = Utc::now();
        let end = start + chrono::Duration::hours(1);
        let mut item = CalendarItem::new("Meeting", start, end);
        item.is_now = true;
        briefing.calendar.upcoming.push(item);

        assert!(briefing.has_attention_items());
    }

    #[test]
    fn test_weather_summary_serialization() {
        let weather = WeatherSummary::new(18.5, "Rainy", 20.0, 15.0);
        let json = serde_json::to_string(&weather).expect("serialize");
        let parsed: WeatherSummary = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(parsed.condition, "Rainy");
    }

    #[test]
    fn test_task_item_serialization() {
        let task = TaskItem::new("t-1", "Test")
            .with_priority(Priority::High)
            .overdue();
        let json = serde_json::to_string(&task).expect("serialize");
        let parsed: TaskItem = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(parsed.priority, Priority::High);
        assert!(parsed.is_overdue);
    }

    #[test]
    fn test_calendar_item_serialization() {
        let start = Utc::now();
        let end = start + chrono::Duration::hours(2);
        let item = CalendarItem::new("Workshop", start, end);
        let json = serde_json::to_string(&item).expect("serialize");
        let parsed: CalendarItem = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(parsed.title, "Workshop");
    }

    #[test]
    fn test_email_brief_serialization() {
        let mut brief = EmailBrief::new();
        brief.unread_count = 5;
        brief.important_senders.push("boss@company.com".to_string());
        let json = serde_json::to_string(&brief).expect("serialize");
        let parsed: EmailBrief = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(parsed.unread_count, 5);
    }

    #[test]
    fn test_task_brief_has_attention_with_high_priority() {
        let mut brief = TaskBrief::new();
        brief.high_priority.push(TaskItem::new("1", "Urgent"));
        assert!(brief.has_attention_items());
        assert_eq!(brief.attention_count(), 1);
    }

    #[test]
    fn test_task_brief_attention_count_combined() {
        let mut brief = TaskBrief::new();
        brief.overdue = 2;
        brief.high_priority.push(TaskItem::new("1", "Urgent"));
        brief.high_priority.push(TaskItem::new("2", "Also urgent"));
        assert_eq!(brief.attention_count(), 4); // 2 overdue + 2 high priority
    }
}
