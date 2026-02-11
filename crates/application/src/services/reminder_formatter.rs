//! Reminder message formatting utilities
//!
//! Pure functions for formatting reminder notifications into beautiful
//! messenger messages with emoji, Google Maps links, and transit info.

use chrono::{DateTime, Utc};
use domain::entities::{Reminder, ReminderSource};

use super::location_helper::{format_location_with_link, generate_maps_link};
use crate::ports::{TransitConnection, format_connections};

// ── Calendar event reminder ─────────────────────────────────────

/// Format a calendar event reminder with optional transit connections
#[must_use]
pub fn format_calendar_event_reminder(
    reminder: &Reminder,
    transit_connections: Option<&[TransitConnection]>,
) -> String {
    let mut parts = Vec::new();

    // Header with event emoji
    parts.push(format!("🔔 *Reminder: {}*", reminder.title));

    // Event time
    if let Some(event_time) = reminder.event_time {
        parts.push(format!("🕐 {}", format_event_time(event_time)));
        parts.push(format!("⏰ {}", format_time_until(event_time)));
    }

    // Description
    if let Some(ref desc) = reminder.description {
        if !desc.is_empty() {
            parts.push(format!("📝 {desc}"));
        }
    }

    // Location with Maps link
    if let Some(ref location) = reminder.location {
        parts.push(String::new());
        parts.push(format_location_with_link(location));
    }

    // Transit connections
    if let Some(connections) = transit_connections {
        if !connections.is_empty() {
            parts.push(String::new());
            parts.push("🚆 *ÖPNV-Verbindungen:*".to_string());
            parts.push(format_connections(connections));
        }
    }

    parts.join("\n")
}

// ── Calendar task reminder ──────────────────────────────────────

/// Format a calendar task/todo reminder
#[must_use]
pub fn format_calendar_task_reminder(reminder: &Reminder) -> String {
    let mut parts = Vec::new();

    // Header with task emoji
    parts.push(format!("📋 *Aufgabe: {}*", reminder.title));

    // Due time
    if let Some(event_time) = reminder.event_time {
        parts.push(format!("📅 Fällig: {}", format_event_time(event_time)));
    }

    // Description
    if let Some(ref desc) = reminder.description {
        if !desc.is_empty() {
            parts.push(format!("📝 {desc}"));
        }
    }

    // Location
    if let Some(ref location) = reminder.location {
        parts.push(format_location_with_link(location));
    }

    parts.join("\n")
}

// ── Custom reminder ─────────────────────────────────────────────

/// Format a custom user-created reminder
#[must_use]
pub fn format_custom_reminder(reminder: &Reminder) -> String {
    let mut parts = Vec::new();

    // Header
    parts.push(format!("⏰ *Erinnerung: {}*", reminder.title));

    // Description
    if let Some(ref desc) = reminder.description {
        if !desc.is_empty() {
            parts.push(format!("📝 {desc}"));
        }
    }

    // Location
    if let Some(ref location) = reminder.location {
        parts.push(String::new());
        parts.push(format_location_with_link(location));
    }

    parts.join("\n")
}

// ── Generic reminder dispatch ───────────────────────────────────

/// Format any reminder based on its source type
#[must_use]
pub fn format_reminder(
    reminder: &Reminder,
    transit_connections: Option<&[TransitConnection]>,
) -> String {
    match reminder.source {
        ReminderSource::CalendarEvent => {
            format_calendar_event_reminder(reminder, transit_connections)
        },
        ReminderSource::CalendarTask => format_calendar_task_reminder(reminder),
        ReminderSource::Custom => format_custom_reminder(reminder),
    }
}

// ── Morning briefing ────────────────────────────────────────────

/// Data for a single event in the morning briefing
#[derive(Debug, Clone)]
pub struct BriefingEvent {
    /// Event title
    pub title: String,
    /// Event start time
    pub start_time: String,
    /// Event end time
    pub end_time: String,
    /// Optional location
    pub location: Option<String>,
}

/// Data for the morning briefing
#[derive(Debug, Clone)]
pub struct MorningBriefingData {
    /// Today's date (formatted)
    pub date: String,
    /// Today's calendar events
    pub events: Vec<BriefingEvent>,
    /// Active reminder count
    pub reminder_count: u64,
    /// Optional weather summary
    pub weather_summary: Option<String>,
}

/// Format a morning briefing message
#[must_use]
pub fn format_morning_briefing(data: &MorningBriefingData) -> String {
    let mut parts = Vec::new();

    // Header
    parts.push(format!("☀️ *Guten Morgen!*\n📅 {}", data.date));

    // Weather
    if let Some(ref weather) = data.weather_summary {
        parts.push(String::new());
        parts.push(weather.clone());
    }

    // Events
    parts.push(String::new());
    if data.events.is_empty() {
        parts.push("📋 Keine Termine heute.".to_string());
    } else {
        parts.push(format!("📋 *{} Termin(e) heute:*", data.events.len()));
        for event in &data.events {
            let mut event_line = format!(
                "  • {} – {} {}",
                event.start_time, event.end_time, event.title
            );
            if let Some(ref loc) = event.location {
                let link = generate_maps_link(loc);
                event_line.push_str(&format!("\n    📍 {loc} ({link})"));
            }
            parts.push(event_line);
        }
    }

    // Reminders
    if data.reminder_count > 0 {
        parts.push(String::new());
        parts.push(format!("⏰ {} aktive Erinnerung(en)", data.reminder_count));
    }

    parts.join("\n")
}

// ── Snooze confirmation ─────────────────────────────────────────

/// Format a snooze confirmation message
#[must_use]
pub fn format_snooze_confirmation(reminder: &Reminder, new_time: DateTime<Utc>) -> String {
    format!(
        "💤 *Verschoben:* {}\n⏰ Neue Erinnerung: {}",
        reminder.title,
        format_event_time(new_time)
    )
}

/// Format an acknowledgement confirmation
#[must_use]
pub fn format_acknowledge_confirmation(reminder: &Reminder) -> String {
    format!("✅ *Erledigt:* {}", reminder.title)
}

/// Format a list of active reminders
#[must_use]
pub fn format_reminder_list(reminders: &[Reminder]) -> String {
    if reminders.is_empty() {
        return "📭 Keine aktiven Erinnerungen.".to_string();
    }

    let mut parts = Vec::new();
    parts.push(format!("📋 *{} Erinnerung(en):*", reminders.len()));

    for reminder in reminders {
        let source_emoji = match reminder.source {
            ReminderSource::CalendarEvent => "📅",
            ReminderSource::CalendarTask => "📋",
            ReminderSource::Custom => "⏰",
        };
        let time_str = reminder
            .event_time
            .map_or_else(|| format_event_time(reminder.remind_at), format_event_time);
        parts.push(format!("  {source_emoji} {time_str} — {}", reminder.title));
    }

    parts.join("\n")
}

// ── Time formatting helpers ─────────────────────────────────────

/// Format a datetime for display in messages (German locale style)
#[must_use]
pub fn format_event_time(dt: DateTime<Utc>) -> String {
    dt.format("%d.%m. %H:%M Uhr").to_string()
}

/// Format a human-readable "time until" string
#[must_use]
pub fn format_time_until(target: DateTime<Utc>) -> String {
    let now = Utc::now();
    let duration = target.signed_duration_since(now);
    let minutes = duration.num_minutes();

    if minutes < 0 {
        return "jetzt".to_string();
    }
    if minutes < 1 {
        return "in weniger als 1 Minute".to_string();
    }
    if minutes < 60 {
        return format!("in {minutes} Minuten");
    }

    let hours = minutes / 60;
    let remaining_mins = minutes % 60;

    if hours < 24 {
        if remaining_mins == 0 {
            return format!("in {hours} Stunde(n)");
        }
        return format!("in {hours}h {remaining_mins}min");
    }

    let days = hours / 24;
    format!("in {days} Tag(en)")
}

#[cfg(test)]
mod tests {
    use domain::value_objects::UserId;

    use super::*;

    fn make_reminder(source: ReminderSource, title: &str) -> Reminder {
        Reminder::new(
            UserId::new(),
            source,
            title,
            Utc::now() + chrono::Duration::hours(1),
        )
    }

    #[test]
    fn format_custom_reminder_basic() {
        let reminder = make_reminder(ReminderSource::Custom, "Buy milk");
        let output = format_custom_reminder(&reminder);
        assert!(output.contains("⏰ *Erinnerung: Buy milk*"));
    }

    #[test]
    fn format_custom_with_description() {
        let reminder = make_reminder(ReminderSource::Custom, "Call dentist")
            .with_description("Dr. Schmidt, 030-12345");
        let output = format_custom_reminder(&reminder);
        assert!(output.contains("📝 Dr. Schmidt, 030-12345"));
    }

    #[test]
    fn format_custom_with_location() {
        let reminder = make_reminder(ReminderSource::Custom, "Pickup package")
            .with_location("DHL Packstation, Alexanderplatz, Berlin");
        let output = format_custom_reminder(&reminder);
        assert!(output.contains("📍 DHL Packstation"));
        assert!(output.contains("maps.google.com"));
    }

    #[test]
    fn format_calendar_event_basic() {
        let event_time = Utc::now() + chrono::Duration::hours(2);
        let reminder = make_reminder(ReminderSource::CalendarEvent, "Team meeting")
            .with_event_time(event_time);
        let output = format_calendar_event_reminder(&reminder, None);
        assert!(output.contains("🔔 *Reminder: Team meeting*"));
        assert!(output.contains("🕐"));
    }

    #[test]
    fn format_calendar_event_with_location_and_transit() {
        use crate::ports::TransitConnection;

        let event_time = Utc::now() + chrono::Duration::hours(2);
        let reminder = make_reminder(ReminderSource::CalendarEvent, "Conference")
            .with_event_time(event_time)
            .with_location("TU Berlin, Straße des 17. Juni 135");

        let connections = vec![TransitConnection {
            departure_time: Utc::now() + chrono::Duration::hours(1),
            arrival_time: event_time,
            duration_minutes: 45,
            transfers: 1,
            legs: vec![],
            delay_info: None,
        }];

        let output = format_calendar_event_reminder(&reminder, Some(&connections));
        assert!(output.contains("📍 TU Berlin"));
        assert!(output.contains("maps.google.com"));
        assert!(output.contains("ÖPNV"));
    }

    #[test]
    fn format_calendar_task_basic() {
        let reminder = make_reminder(ReminderSource::CalendarTask, "Submit report")
            .with_description("Quarterly sales report");
        let output = format_calendar_task_reminder(&reminder);
        assert!(output.contains("📋 *Aufgabe: Submit report*"));
        assert!(output.contains("📝 Quarterly sales report"));
    }

    #[test]
    fn format_reminder_dispatches_by_source() {
        let r1 = make_reminder(ReminderSource::Custom, "Custom");
        let r2 = make_reminder(ReminderSource::CalendarEvent, "Event");
        let r3 = make_reminder(ReminderSource::CalendarTask, "Task");

        assert!(format_reminder(&r1, None).contains("⏰"));
        assert!(format_reminder(&r2, None).contains("🔔"));
        assert!(format_reminder(&r3, None).contains("📋"));
    }

    #[test]
    fn format_time_until_now() {
        let t = Utc::now() - chrono::Duration::minutes(5);
        assert_eq!(format_time_until(t), "jetzt");
    }

    #[test]
    fn format_time_until_minutes() {
        let t = Utc::now() + chrono::Duration::minutes(30);
        let output = format_time_until(t);
        assert!(output.contains("Minuten"), "Got: {output}");
    }

    #[test]
    fn format_time_until_hours() {
        let t = Utc::now() + chrono::Duration::hours(3);
        let output = format_time_until(t);
        assert!(
            output.contains('h') || output.contains("Stunde"),
            "Got: {output}"
        );
    }

    #[test]
    fn format_time_until_days() {
        let t = Utc::now() + chrono::Duration::days(2);
        assert!(format_time_until(t).contains("Tag"));
    }

    #[test]
    fn format_morning_briefing_empty() {
        let data = MorningBriefingData {
            date: "Montag, 15. Januar 2025".to_string(),
            events: vec![],
            reminder_count: 0,
            weather_summary: None,
        };
        let output = format_morning_briefing(&data);
        assert!(output.contains("Guten Morgen"));
        assert!(output.contains("Keine Termine"));
    }

    #[test]
    fn format_morning_briefing_with_events() {
        let data = MorningBriefingData {
            date: "Montag, 15. Januar 2025".to_string(),
            events: vec![
                BriefingEvent {
                    title: "Standup".to_string(),
                    start_time: "09:00".to_string(),
                    end_time: "09:15".to_string(),
                    location: None,
                },
                BriefingEvent {
                    title: "Workshop".to_string(),
                    start_time: "14:00".to_string(),
                    end_time: "16:00".to_string(),
                    location: Some("Room 3B, TU Berlin".to_string()),
                },
            ],
            reminder_count: 3,
            weather_summary: Some("🌤️ 12°C, leicht bewölkt".to_string()),
        };
        let output = format_morning_briefing(&data);
        assert!(output.contains("2 Termin(e)"));
        assert!(output.contains("Standup"));
        assert!(output.contains("Workshop"));
        assert!(output.contains("📍 Room 3B"));
        assert!(output.contains("maps.google.com"));
        assert!(output.contains("3 aktive Erinnerung"));
        assert!(output.contains("12°C"));
    }

    #[test]
    fn format_snooze_confirmation_msg() {
        let r = make_reminder(ReminderSource::Custom, "Test");
        let new_time = Utc::now() + chrono::Duration::minutes(15);
        let output = format_snooze_confirmation(&r, new_time);
        assert!(output.contains("💤 *Verschoben:* Test"));
        assert!(output.contains("Neue Erinnerung"));
    }

    #[test]
    fn format_acknowledge_confirmation_msg() {
        let r = make_reminder(ReminderSource::Custom, "Done task");
        let output = format_acknowledge_confirmation(&r);
        assert!(output.contains("✅ *Erledigt:* Done task"));
    }

    #[test]
    fn format_reminder_list_empty() {
        let output = format_reminder_list(&[]);
        assert!(output.contains("Keine aktiven"));
    }

    #[test]
    fn format_reminder_list_multiple() {
        let reminders = vec![
            make_reminder(ReminderSource::CalendarEvent, "Meeting"),
            make_reminder(ReminderSource::Custom, "Groceries"),
        ];
        let output = format_reminder_list(&reminders);
        assert!(output.contains("2 Erinnerung"));
        assert!(output.contains("📅"));
        assert!(output.contains("⏰"));
        assert!(output.contains("Meeting"));
        assert!(output.contains("Groceries"));
    }

    #[test]
    fn format_event_time_format() {
        use chrono::TimeZone;
        let dt = Utc.with_ymd_and_hms(2025, 1, 15, 14, 30, 0).unwrap();
        let output = format_event_time(dt);
        assert_eq!(output, "15.01. 14:30 Uhr");
    }
}
