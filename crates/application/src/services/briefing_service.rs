//! Morning Briefing Service
//!
//! Aggregates calendar events and emails into a daily summary.

use chrono::{DateTime, Utc};
use domain::value_objects::Timezone;
use serde::{Deserialize, Serialize};

/// Morning briefing data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MorningBriefing {
    /// When the briefing was generated
    pub generated_at: DateTime<Utc>,
    /// Date this briefing is for
    pub briefing_date: String,
    /// Weather summary (optional)
    pub weather: Option<WeatherSummary>,
    /// Calendar events for today
    pub calendar: CalendarBrief,
    /// Email summary
    pub email: EmailBrief,
    /// Task summary
    pub tasks: TaskBrief,
    /// Natural language summary
    pub summary: String,
}

/// Weather summary
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WeatherSummary {
    /// Temperature in Celsius
    pub temperature: f32,
    /// Weather condition (sunny, cloudy, rain, etc.)
    pub condition: String,
    /// High temperature
    pub high: f32,
    /// Low temperature
    pub low: f32,
}

/// Calendar portion of briefing
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CalendarBrief {
    /// Number of events today
    pub event_count: u32,
    /// Next upcoming event
    pub next_event: Option<EventSummary>,
    /// All events for today
    pub events: Vec<EventSummary>,
    /// Any conflicts detected
    pub conflicts: Vec<String>,
}

/// Summary of a calendar event
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventSummary {
    /// Event title
    pub title: String,
    /// Start time (HH:MM format)
    pub start_time: String,
    /// End time (HH:MM format)  
    pub end_time: String,
    /// Location if available
    pub location: Option<String>,
    /// Whether it's an all-day event
    pub all_day: bool,
}

/// Email portion of briefing
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct EmailBrief {
    /// Number of unread emails
    pub unread_count: u32,
    /// Important/flagged emails
    pub important_count: u32,
    /// Top senders today
    pub top_senders: Vec<String>,
    /// Subject lines of important emails
    pub highlights: Vec<EmailHighlight>,
}

/// Highlighted email for briefing
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmailHighlight {
    /// Sender name/email
    pub from: String,
    /// Subject line
    pub subject: String,
    /// Brief preview
    pub preview: String,
}

/// Task portion of briefing  
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TaskBrief {
    /// Number of tasks due today
    pub due_today: u32,
    /// Number of overdue tasks
    pub overdue: u32,
    /// High priority tasks
    pub high_priority: Vec<String>,
}

/// Briefing service for generating morning summaries
#[derive(Debug, Clone)]
pub struct BriefingService {
    /// User's timezone
    pub timezone: Timezone,
}

impl Default for BriefingService {
    fn default() -> Self {
        Self {
            timezone: Timezone::utc(),
        }
    }
}

impl BriefingService {
    /// Create a new briefing service with a timezone
    #[must_use]
    pub const fn new(timezone: Timezone) -> Self {
        Self { timezone }
    }

    /// Create a new briefing service with an offset (hours from UTC)
    ///
    /// This is a convenience method for backwards compatibility.
    /// Prefer using `new()` with a proper `Timezone` for accurate handling.
    #[must_use]
    #[deprecated(
        since = "0.3.0",
        note = "Use BriefingService::new() with Timezone instead"
    )]
    pub fn with_offset(offset_hours: i32) -> Self {
        // Map common offsets to IANA timezones
        let tz = match offset_hours {
            0 => Timezone::utc(),
            1 | 2 => Timezone::berlin(), // CET/CEST (approximate)
            -5 => Timezone::new_york(),
            _ => Timezone::utc(),
        };
        Self { timezone: tz }
    }

    /// Get the configured timezone
    #[must_use]
    pub const fn timezone(&self) -> &Timezone {
        &self.timezone
    }

    /// Generate a morning briefing
    ///
    /// This combines calendar, email, and task data into a summary.
    #[must_use]
    pub fn generate_briefing(
        &self,
        calendar: CalendarBrief,
        email: EmailBrief,
        tasks: TaskBrief,
        weather: Option<WeatherSummary>,
    ) -> MorningBriefing {
        let now = Utc::now();
        let briefing_date = now.format("%Y-%m-%d").to_string();

        let summary = self.generate_summary(&calendar, &email, &tasks, weather.as_ref());

        MorningBriefing {
            generated_at: now,
            briefing_date,
            weather,
            calendar,
            email,
            tasks,
            summary,
        }
    }

    /// Generate natural language summary
    #[allow(clippy::unused_self)]
    fn generate_summary(
        &self,
        calendar: &CalendarBrief,
        email: &EmailBrief,
        tasks: &TaskBrief,
        weather: Option<&WeatherSummary>,
    ) -> String {
        let mut parts = Vec::new();

        // Weather summary
        if let Some(w) = weather {
            parts.push(format!(
                "Today's weather: {} with a high of {:.0}°C.",
                w.condition, w.high
            ));
        }

        // Calendar summary
        match calendar.event_count {
            0 => parts.push("Your calendar is clear today.".to_string()),
            1 => {
                if let Some(next) = &calendar.next_event {
                    parts.push(format!(
                        "You have 1 event today: {} at {}.",
                        next.title, next.start_time
                    ));
                }
            },
            n => {
                if let Some(next) = &calendar.next_event {
                    parts.push(format!(
                        "You have {n} events today. Next up: {} at {}.",
                        next.title, next.start_time
                    ));
                } else {
                    parts.push(format!("You have {n} events scheduled today."));
                }
            },
        }

        // Email summary
        if email.unread_count > 0 {
            let important_note = if email.important_count > 0 {
                format!(", {} marked important", email.important_count)
            } else {
                String::new()
            };
            parts.push(format!(
                "You have {} unread emails{}.",
                email.unread_count, important_note
            ));
        }

        // Task summary
        if tasks.due_today > 0 || tasks.overdue > 0 {
            let mut task_parts = Vec::new();
            if tasks.due_today > 0 {
                task_parts.push(format!("{} due today", tasks.due_today));
            }
            if tasks.overdue > 0 {
                task_parts.push(format!("{} overdue", tasks.overdue));
            }
            parts.push(format!("Tasks: {}.", task_parts.join(", ")));
        }

        // Conflicts warning
        if !calendar.conflicts.is_empty() {
            parts.push(format!(
                "⚠️ Calendar conflict detected: {}",
                calendar.conflicts.join(", ")
            ));
        }

        parts.join(" ")
    }

    /// Format time from ISO 8601 to HH:MM
    #[must_use]
    pub fn format_time(iso_time: &str) -> String {
        // Try to extract time from various formats
        if let Some(time_part) = iso_time.split('T').nth(1) {
            if let Some(hhmm) = time_part.get(0..5) {
                return hhmm.to_string();
            }
        }
        // Fallback to original
        iso_time.to_string()
    }

    /// Check for calendar conflicts (overlapping events)
    #[must_use]
    pub fn detect_conflicts(events: &[EventSummary]) -> Vec<String> {
        let mut conflicts = Vec::new();

        for i in 0..events.len() {
            for j in (i + 1)..events.len() {
                let a = &events[i];
                let b = &events[j];

                // Skip all-day events
                if a.all_day || b.all_day {
                    continue;
                }

                // Simple overlap check (assumes HH:MM format)
                if a.start_time < b.end_time && b.start_time < a.end_time {
                    conflicts.push(format!("'{}' overlaps with '{}'", a.title, b.title));
                }
            }
        }

        conflicts
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn briefing_service_creation() {
        let service = BriefingService::new(Timezone::berlin());
        assert_eq!(service.timezone().as_str(), "Europe/Berlin");
    }

    #[test]
    fn briefing_service_default() {
        let service = BriefingService::default();
        assert_eq!(service.timezone().as_str(), "UTC");
    }

    #[test]
    #[allow(deprecated)]
    fn briefing_service_with_offset() {
        let service = BriefingService::with_offset(1);
        assert_eq!(service.timezone().as_str(), "Europe/Berlin");
    }

    #[test]
    fn generate_briefing_empty_data() {
        let service = BriefingService::new(Timezone::utc());
        let briefing = service.generate_briefing(
            CalendarBrief::default(),
            EmailBrief::default(),
            TaskBrief::default(),
            None,
        );

        assert!(briefing.summary.contains("calendar is clear"));
        assert!(!briefing.briefing_date.is_empty());
    }

    #[test]
    fn generate_briefing_with_events() {
        let service = BriefingService::new(Timezone::utc());

        let calendar = CalendarBrief {
            event_count: 2,
            next_event: Some(EventSummary {
                title: "Team Standup".to_string(),
                start_time: "09:00".to_string(),
                end_time: "09:30".to_string(),
                location: Some("Conference Room".to_string()),
                all_day: false,
            }),
            events: vec![],
            conflicts: vec![],
        };

        let briefing =
            service.generate_briefing(calendar, EmailBrief::default(), TaskBrief::default(), None);

        assert!(briefing.summary.contains("2 events"));
        assert!(briefing.summary.contains("Team Standup"));
        assert!(briefing.summary.contains("09:00"));
    }

    #[test]
    fn generate_briefing_with_weather() {
        let service = BriefingService::new(Timezone::utc());

        let weather = Some(WeatherSummary {
            temperature: 18.0,
            condition: "Partly cloudy".to_string(),
            high: 22.0,
            low: 14.0,
        });

        let briefing = service.generate_briefing(
            CalendarBrief::default(),
            EmailBrief::default(),
            TaskBrief::default(),
            weather,
        );

        assert!(briefing.summary.contains("Partly cloudy"));
        assert!(briefing.summary.contains("22"));
    }

    #[test]
    fn generate_briefing_with_emails() {
        let service = BriefingService::new(Timezone::utc());

        let email = EmailBrief {
            unread_count: 5,
            important_count: 2,
            top_senders: vec!["boss@company.com".to_string()],
            highlights: vec![],
        };

        let briefing =
            service.generate_briefing(CalendarBrief::default(), email, TaskBrief::default(), None);

        assert!(briefing.summary.contains("5 unread"));
        assert!(briefing.summary.contains("2 marked important"));
    }

    #[test]
    fn generate_briefing_with_tasks() {
        let service = BriefingService::new(Timezone::utc());

        let tasks = TaskBrief {
            due_today: 3,
            overdue: 1,
            high_priority: vec!["Submit report".to_string()],
        };

        let briefing =
            service.generate_briefing(CalendarBrief::default(), EmailBrief::default(), tasks, None);

        assert!(briefing.summary.contains("3 due today"));
        assert!(briefing.summary.contains("1 overdue"));
    }

    #[test]
    fn generate_briefing_with_conflicts() {
        let service = BriefingService::new(Timezone::utc());

        let calendar = CalendarBrief {
            event_count: 2,
            next_event: None,
            events: vec![],
            conflicts: vec!["Meeting A overlaps with Meeting B".to_string()],
        };

        let briefing =
            service.generate_briefing(calendar, EmailBrief::default(), TaskBrief::default(), None);

        assert!(briefing.summary.contains("conflict"));
        assert!(briefing.summary.contains("Meeting A"));
    }

    #[test]
    fn format_time_iso() {
        assert_eq!(BriefingService::format_time("2025-02-01T09:30:00"), "09:30");
        assert_eq!(
            BriefingService::format_time("2025-02-01T14:00:00Z"),
            "14:00"
        );
    }

    #[test]
    fn format_time_fallback() {
        assert_eq!(BriefingService::format_time("09:30"), "09:30");
        assert_eq!(BriefingService::format_time("invalid"), "invalid");
    }

    #[test]
    fn detect_conflicts_none() {
        let events = vec![
            EventSummary {
                title: "Event A".to_string(),
                start_time: "09:00".to_string(),
                end_time: "10:00".to_string(),
                location: None,
                all_day: false,
            },
            EventSummary {
                title: "Event B".to_string(),
                start_time: "10:00".to_string(),
                end_time: "11:00".to_string(),
                location: None,
                all_day: false,
            },
        ];

        let conflicts = BriefingService::detect_conflicts(&events);
        assert!(conflicts.is_empty());
    }

    #[test]
    fn detect_conflicts_overlap() {
        let events = vec![
            EventSummary {
                title: "Event A".to_string(),
                start_time: "09:00".to_string(),
                end_time: "10:30".to_string(),
                location: None,
                all_day: false,
            },
            EventSummary {
                title: "Event B".to_string(),
                start_time: "10:00".to_string(),
                end_time: "11:00".to_string(),
                location: None,
                all_day: false,
            },
        ];

        let conflicts = BriefingService::detect_conflicts(&events);
        assert_eq!(conflicts.len(), 1);
        assert!(conflicts[0].contains("Event A"));
        assert!(conflicts[0].contains("Event B"));
    }

    #[test]
    fn detect_conflicts_all_day_ignored() {
        let events = vec![
            EventSummary {
                title: "All Day Event".to_string(),
                start_time: "00:00".to_string(),
                end_time: "23:59".to_string(),
                location: None,
                all_day: true,
            },
            EventSummary {
                title: "Regular Event".to_string(),
                start_time: "10:00".to_string(),
                end_time: "11:00".to_string(),
                location: None,
                all_day: false,
            },
        ];

        let conflicts = BriefingService::detect_conflicts(&events);
        assert!(conflicts.is_empty());
    }

    #[test]
    fn calendar_brief_default() {
        let brief = CalendarBrief::default();
        assert_eq!(brief.event_count, 0);
        assert!(brief.next_event.is_none());
        assert!(brief.events.is_empty());
        assert!(brief.conflicts.is_empty());
    }

    #[test]
    fn email_brief_default() {
        let brief = EmailBrief::default();
        assert_eq!(brief.unread_count, 0);
        assert_eq!(brief.important_count, 0);
        assert!(brief.top_senders.is_empty());
        assert!(brief.highlights.is_empty());
    }

    #[test]
    fn task_brief_default() {
        let brief = TaskBrief::default();
        assert_eq!(brief.due_today, 0);
        assert_eq!(brief.overdue, 0);
        assert!(brief.high_priority.is_empty());
    }

    #[test]
    fn morning_briefing_serialization() {
        let service = BriefingService::new(Timezone::utc());
        let briefing = service.generate_briefing(
            CalendarBrief::default(),
            EmailBrief::default(),
            TaskBrief::default(),
            None,
        );

        let json = serde_json::to_string(&briefing).unwrap();
        assert!(json.contains("generated_at"));
        assert!(json.contains("briefing_date"));
        assert!(json.contains("summary"));
    }

    #[test]
    fn event_summary_clone() {
        let event = EventSummary {
            title: "Test".to_string(),
            start_time: "09:00".to_string(),
            end_time: "10:00".to_string(),
            location: Some("Room 1".to_string()),
            all_day: false,
        };
        #[allow(clippy::redundant_clone)]
        let cloned = event.clone();
        assert_eq!(event.title, cloned.title);
    }

    #[test]
    fn weather_summary_clone() {
        let weather = WeatherSummary {
            temperature: 20.0,
            condition: "Sunny".to_string(),
            high: 25.0,
            low: 15.0,
        };
        #[allow(clippy::redundant_clone)]
        let cloned = weather.clone();
        // Use string comparison to avoid float_cmp lint
        assert_eq!(weather.condition, cloned.condition);
        assert_eq!(weather.high.to_string(), cloned.high.to_string());
    }

    #[test]
    fn email_highlight_creation() {
        let highlight = EmailHighlight {
            from: "boss@company.com".to_string(),
            subject: "Important Update".to_string(),
            preview: "Please review...".to_string(),
        };
        assert_eq!(highlight.from, "boss@company.com");
    }

    #[test]
    fn email_highlight_serialization() {
        let highlight = EmailHighlight {
            from: "test@example.com".to_string(),
            subject: "Test".to_string(),
            preview: "Preview text".to_string(),
        };
        let json = serde_json::to_string(&highlight).unwrap();
        assert!(json.contains("from"));
        assert!(json.contains("subject"));
    }

    #[test]
    fn briefing_service_has_debug() {
        let service = BriefingService::new(Timezone::utc());
        let debug = format!("{service:?}");
        assert!(debug.contains("BriefingService"));
    }

    #[test]
    fn single_event_summary() {
        let service = BriefingService::new(Timezone::utc());

        let calendar = CalendarBrief {
            event_count: 1,
            next_event: Some(EventSummary {
                title: "Dentist Appointment".to_string(),
                start_time: "14:00".to_string(),
                end_time: "15:00".to_string(),
                location: None,
                all_day: false,
            }),
            events: vec![],
            conflicts: vec![],
        };

        let briefing =
            service.generate_briefing(calendar, EmailBrief::default(), TaskBrief::default(), None);

        assert!(briefing.summary.contains("1 event"));
        assert!(briefing.summary.contains("Dentist Appointment"));
    }
}
