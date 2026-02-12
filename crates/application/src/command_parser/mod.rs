//! Command parser - Parse natural language into typed commands
//!
//! This module is split into focused sub-modules:
//! - [`quick_patterns`]: Fast keyword-based pattern matching (no LLM needed)
//! - [`llm`]: LLM-powered intent detection and JSON parsing
//! - [`intent_mapping`]: Mapping parsed intents to typed `AgentCommand` values

mod intent_mapping;
mod llm;
mod quick_patterns;

use std::fmt;

use domain::AgentCommand;
use serde::Deserialize;
use tracing::debug;

/// System prompt for intent detection
pub(super) const INTENT_SYSTEM_PROMPT: &str = r#"You are an intent classifier for a personal assistant.
Analyze the user input and extract the intent as JSON.

Possible intents:
- "morning_briefing": Request morning briefing (e.g., "What's on today?", "Briefing")
- "create_calendar_event": Create appointment (requires: date, time, title)
- "update_calendar_event": Update existing appointment (requires: event_id; optional: date, time, title, location, duration_minutes)
- "list_tasks": List tasks (optional: status, priority, list filters)
- "create_task": Create a task (requires: title; optional: date for due date, priority, description, list)
- "complete_task": Mark task done (requires: task_id)
- "update_task": Update task (requires: task_id; optional: title, date, priority, description)
- "delete_task": Delete task (requires: task_id)
- "list_task_lists": List all available task lists/calendars
- "create_task_list": Create a new task list (requires: name)
- "summarize_inbox": Email summary (e.g., "What's new?", "Mails")
- "draft_email": Draft email (requires: to, body; optional: subject)
- "send_email": Send email (requires: draft_id)
- "web_search": Search the internet (requires: query; optional: max_results)
- "create_reminder": Create a reminder (requires: title, remind_at datetime; optional: description)
- "list_reminders": List active reminders (optional: include_done)
- "snooze_reminder": Snooze a reminder (requires: reminder_id; optional: duration_minutes, default 15)
- "acknowledge_reminder": Mark reminder done (requires: reminder_id)
- "delete_reminder": Delete a reminder (requires: reminder_id)
- "search_transit": Search public transit (requires: from, to locations; optional: departure datetime)
- "ask": General question (if nothing else matches)

Reply ONLY with valid JSON:
{
  "intent": "<intent_name>",
  "date": "YYYY-MM-DD" (optional, for appointments/tasks),
  "time": "HH:MM" (optional, for appointments),
  "title": "..." (optional, for appointments/tasks),
  "event_id": "..." (required for update_calendar_event),
  "task_id": "..." (required for complete_task/update_task/delete_task),
  "priority": "high|medium|low" (optional, for tasks),
  "status": "needs_action|in_progress|completed|cancelled" (optional, for list_tasks),
  "description": "..." (optional, for tasks),
  "list": "..." (optional, for tasks - target list/calendar name),
  "name": "..." (required for create_task_list),
  "location": "..." (optional, for appointments),
  "duration_minutes": 60 (optional, for appointments),
  "to": "email@example.com" (optional, for emails),
  "subject": "..." (optional, for emails),
  "body": "..." (optional, for emails),
  "question": "..." (only for ask intent),
  "count": 10 (optional, for inbox),
  "draft_id": "..." (optional, for send_email),
  "query": "..." (only for web_search intent),
  "max_results": 5 (optional, for web_search, default 5),
  "reminder_id": "..." (for snooze/acknowledge/delete_reminder),
  "remind_at": "YYYY-MM-DD HH:MM" (for create_reminder, when to fire),
  "include_done": false (optional, for list_reminders),
  "from": "..." (origin address for search_transit),
  "to_address": "..." (destination address for search_transit),
  "departure": "YYYY-MM-DD HH:MM" (optional, for search_transit)
}

Examples:
- "Briefing for tomorrow" → {"intent":"morning_briefing","date":"2025-02-02"}
- "Appointment tomorrow 14:00 Team Meeting" → {"intent":"create_calendar_event","date":"2025-02-02","time":"14:00","title":"Team Meeting"}
- "Move event abc123 to 15:00" → {"intent":"update_calendar_event","event_id":"abc123","time":"15:00"}
- "What are my tasks?" → {"intent":"list_tasks"}
- "Show high priority tasks" → {"intent":"list_tasks","priority":"high"}
- "Tasks on list Work" → {"intent":"list_tasks","list":"Work"}
- "Add task buy groceries" → {"intent":"create_task","title":"buy groceries"}
- "Create task call mom due Friday priority high" → {"intent":"create_task","title":"call mom","date":"2025-02-07","priority":"high"}
- "Add task meeting prep on list Work" → {"intent":"create_task","title":"meeting prep","list":"Work"}
- "Mark task abc done" → {"intent":"complete_task","task_id":"abc"}
- "Delete task xyz" → {"intent":"delete_task","task_id":"xyz"}
- "What lists do I have?" → {"intent":"list_task_lists"}
- "Create list Vacation" → {"intent":"create_task_list","name":"Vacation"}
- "Summarize my mails" → {"intent":"summarize_inbox"}
- "Search the internet for Rust async patterns" → {"intent":"web_search","query":"Rust async patterns"}
- "Remind me to call mom in 30 minutes" → {"intent":"create_reminder","title":"call mom","remind_at":"2025-01-15 10:30"}
- "Erinner mich morgen um 9 Uhr an Arzttermin" → {"intent":"create_reminder","title":"Arzttermin","remind_at":"2025-01-16 09:00"}
- "What are my reminders?" → {"intent":"list_reminders"}
- "Zeig meine Erinnerungen" → {"intent":"list_reminders"}
- "Snooze reminder abc for 15 minutes" → {"intent":"snooze_reminder","reminder_id":"abc","duration_minutes":15}
- "Reminder abc done" → {"intent":"acknowledge_reminder","reminder_id":"abc"}
- "Delete reminder xyz" → {"intent":"delete_reminder","reminder_id":"xyz"}
- "How do I get from Alexanderplatz to TU Berlin?" → {"intent":"search_transit","from":"Alexanderplatz, Berlin","to_address":"TU Berlin"}
- "ÖPNV von Hauptbahnhof nach Potsdamer Platz um 14:00" → {"intent":"search_transit","from":"Hauptbahnhof Berlin","to_address":"Potsdamer Platz","departure":"2025-01-15 14:00"}
- "What's the weather like?" → {"intent":"ask","question":"What's the weather like?"}"#;

/// Parsed intent from LLM
#[derive(Debug, Deserialize)]
pub(super) struct ParsedIntent {
    pub intent: String,
    #[serde(default)]
    pub date: Option<String>,
    #[serde(default)]
    pub time: Option<String>,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub to: Option<String>,
    #[serde(default)]
    pub subject: Option<String>,
    #[serde(default)]
    pub body: Option<String>,
    #[serde(default)]
    pub question: Option<String>,
    #[serde(default)]
    pub count: Option<u32>,
    #[serde(default)]
    pub draft_id: Option<String>,
    #[serde(default)]
    pub query: Option<String>,
    #[serde(default)]
    pub max_results: Option<u32>,
    #[serde(default)]
    pub event_id: Option<String>,
    #[serde(default)]
    pub location: Option<String>,
    #[serde(default)]
    pub duration_minutes: Option<u32>,
    #[serde(default)]
    pub task_id: Option<String>,
    #[serde(default)]
    pub priority: Option<String>,
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub list: Option<String>,
    #[serde(default)]
    pub name: Option<String>,
    // Reminder fields
    #[serde(default)]
    pub reminder_id: Option<String>,
    #[serde(default)]
    pub remind_at: Option<String>,
    #[serde(default)]
    pub include_done: Option<bool>,
    // Transit fields
    #[serde(default)]
    pub from: Option<String>,
    #[serde(default)]
    pub to_address: Option<String>,
    #[serde(default)]
    pub departure: Option<String>,
}

/// Parser for converting natural language to AgentCommand
pub struct CommandParser {
    /// Patterns for quick command matching (without LLM)
    quick_patterns: Vec<QuickPattern>,
}

impl fmt::Debug for CommandParser {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CommandParser")
            .field("quick_patterns_count", &self.quick_patterns.len())
            .finish()
    }
}

/// A pattern for quick matching without LLM
struct QuickPattern {
    /// Keywords that trigger this pattern
    keywords: Vec<&'static str>,
    /// Function to build the command
    builder: fn(&str) -> Option<AgentCommand>,
}

impl CommandParser {
    /// Create a new command parser
    pub fn new() -> Self {
        Self {
            quick_patterns: Self::build_quick_patterns(),
        }
    }

    /// Try to parse using quick patterns (no LLM needed)
    pub fn parse_quick(&self, input: &str) -> Option<AgentCommand> {
        let lower = input.to_lowercase();

        for pattern in &self.quick_patterns {
            if pattern.keywords.iter().any(|kw| lower.contains(kw)) {
                if let Some(cmd) = (pattern.builder)(input) {
                    debug!(command = ?cmd, "Quick-parsed command");
                    return Some(cmd);
                }
            }
        }

        None
    }
}

impl Default for CommandParser {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_echo_command() {
        let parser = CommandParser::new();
        let cmd = parser.parse_quick("echo Hello World").unwrap();

        let AgentCommand::Echo { message } = cmd else {
            unreachable!("Expected Echo command")
        };
        assert_eq!(message, "Hello World");
    }

    #[test]
    fn parses_echo_with_sag() {
        let parser = CommandParser::new();
        let cmd = parser.parse_quick("sag Hello World").unwrap();

        let AgentCommand::Echo { message } = cmd else {
            unreachable!("Expected Echo command")
        };
        assert_eq!(message, "Hello World");
    }

    #[test]
    fn parses_echo_with_sage() {
        let parser = CommandParser::new();
        let cmd = parser.parse_quick("sage Hallo Welt").unwrap();

        let AgentCommand::Echo { message } = cmd else {
            unreachable!("Expected Echo command")
        };
        assert_eq!(message, "Hallo Welt");
    }

    #[test]
    fn parses_help_command() {
        let parser = CommandParser::new();

        // Test "help" alone
        let cmd = parser.parse_quick("help").unwrap();
        assert!(matches!(cmd, AgentCommand::Help { command: None }));

        // Test "help <topic>"
        let cmd = parser.parse_quick("help briefing").unwrap();
        let AgentCommand::Help { command } = cmd else {
            unreachable!("Expected Help command")
        };
        assert_eq!(command, Some("briefing".to_string()));
    }

    #[test]
    fn parses_help_with_help_keyword() {
        let parser = CommandParser::new();
        let cmd = parser.parse_quick("help");
        assert!(cmd.is_some());
    }

    #[test]
    fn parses_help_with_question_mark() {
        let parser = CommandParser::new();
        let cmd = parser.parse_quick("?").unwrap();
        assert!(matches!(cmd, AgentCommand::Help { command: None }));
    }

    #[test]
    fn parses_status_command() {
        let parser = CommandParser::new();
        let cmd = parser.parse_quick("status").unwrap();
        assert!(matches!(
            cmd,
            AgentCommand::System(domain::SystemCommand::Status)
        ));
    }

    #[test]
    fn parses_briefing_command() {
        let parser = CommandParser::new();
        let cmd = parser.parse_quick("briefing").unwrap();
        assert!(matches!(cmd, AgentCommand::MorningBriefing { .. }));
    }

    #[test]
    fn parses_briefing_with_good_morning() {
        let parser = CommandParser::new();
        let cmd = parser.parse_quick("good morning").unwrap();
        assert!(matches!(cmd, AgentCommand::MorningBriefing { .. }));
    }

    #[test]
    fn unknown_input_returns_none() {
        let parser = CommandParser::new();
        let cmd = parser.parse_quick("some random text that matches nothing");
        assert!(cmd.is_none());
    }

    #[test]
    fn parse_quick_is_case_insensitive() {
        let parser = CommandParser::new();

        let tests = vec![
            ("ECHO hello", true),
            ("Echo hello", true),
            ("HELP", true),
            ("Help", true),
            ("STATUS", true),
            ("Status", true),
            ("BRIEFING", true),
        ];

        for (input, should_match) in tests {
            assert_eq!(
                parser.parse_quick(input).is_some(),
                should_match,
                "parse_quick({input}) failed"
            );
        }
    }

    #[test]
    fn parse_quick_preserves_original_case_in_message() {
        let parser = CommandParser::new();
        let cmd = parser.parse_quick("echo Hello WORLD Mixed Case").unwrap();

        let AgentCommand::Echo { message } = cmd else {
            unreachable!("Expected Echo command")
        };
        assert_eq!(message, "Hello WORLD Mixed Case");
    }

    #[test]
    fn command_parser_debug_output() {
        let parser = CommandParser::new();
        let debug = format!("{parser:?}");
        assert!(debug.contains("CommandParser"));
        assert!(debug.contains("quick_patterns_count"));
    }

    #[test]
    fn parses_version_command() {
        let parser = CommandParser::new();
        let cmd = parser.parse_quick("version").unwrap();
        assert!(matches!(
            cmd,
            AgentCommand::System(domain::SystemCommand::Version)
        ));
    }

    #[test]
    fn parses_models_command() {
        let parser = CommandParser::new();
        let cmd = parser.parse_quick("models").unwrap();
        assert!(matches!(
            cmd,
            AgentCommand::System(domain::SystemCommand::ListModels)
        ));
    }

    #[test]
    fn parses_models_command_uppercase() {
        let parser = CommandParser::new();
        let cmd = parser.parse_quick("Models").unwrap();
        assert!(matches!(
            cmd,
            AgentCommand::System(domain::SystemCommand::ListModels)
        ));
    }

    #[test]
    fn parses_inbox_command() {
        let parser = CommandParser::new();
        let cmd = parser.parse_quick("inbox").unwrap();
        let AgentCommand::SummarizeInbox {
            count,
            only_important,
        } = cmd
        else {
            unreachable!("Expected SummarizeInbox command")
        };
        assert_eq!(count, None);
        assert_eq!(only_important, None);
    }

    #[test]
    fn parses_summarize_mails() {
        let parser = CommandParser::new();
        let cmd = parser.parse_quick("summarize mails").unwrap();
        assert!(matches!(cmd, AgentCommand::SummarizeInbox { .. }));
    }

    #[test]
    fn parses_important_mails() {
        let parser = CommandParser::new();
        let cmd = parser.parse_quick("inbox important").unwrap();
        let AgentCommand::SummarizeInbox { only_important, .. } = cmd else {
            unreachable!("Expected SummarizeInbox")
        };
        assert_eq!(only_important, Some(true));
    }

    #[test]
    fn parses_whats_on() {
        let parser = CommandParser::new();
        let cmd = parser.parse_quick("what's on").unwrap();
        assert!(matches!(cmd, AgentCommand::MorningBriefing { .. }));
    }

    #[test]
    fn parses_ping_as_status() {
        let parser = CommandParser::new();
        let cmd = parser.parse_quick("ping").unwrap();
        assert!(matches!(
            cmd,
            AgentCommand::System(domain::SystemCommand::Status)
        ));
    }

    #[test]
    fn default_creates_parser() {
        let parser = CommandParser::default();
        let debug = format!("{parser:?}");
        assert!(debug.contains("CommandParser"));
    }

    #[test]
    fn parser_has_quick_patterns() {
        let parser = CommandParser::new();
        let debug = format!("{parser:?}");
        assert!(debug.contains("quick_patterns_count"));
        // Should have at least a few patterns
        assert!(parser.parse_quick("help").is_some());
    }

    #[test]
    fn help_with_topic_email() {
        let parser = CommandParser::new();
        let cmd = parser.parse_quick("help email").unwrap();
        let AgentCommand::Help { command } = cmd else {
            unreachable!("Expected Help command")
        };
        assert_eq!(command, Some("email".to_string()));
    }

    #[test]
    fn help_with_topic_calendar() {
        let parser = CommandParser::new();
        let cmd = parser.parse_quick("help calendar").unwrap();
        let AgentCommand::Help { command } = cmd else {
            unreachable!("Expected Help command")
        };
        assert_eq!(command, Some("calendar".to_string()));
    }

    #[test]
    fn parses_good_morning() {
        let parser = CommandParser::new();
        let cmd = parser.parse_quick("good morning");
        assert!(cmd.is_some());
    }

    #[test]
    fn parses_what_is_on_today() {
        let parser = CommandParser::new();
        let cmd = parser.parse_quick("what is on today").unwrap();
        assert!(matches!(cmd, AgentCommand::MorningBriefing { .. }));
    }

    #[test]
    fn parses_emails_inbox() {
        let parser = CommandParser::new();
        // Test various email keywords
        assert!(parser.parse_quick("inbox summary").is_some());
    }

    #[test]
    fn parses_list_reminders_german() {
        let parser = CommandParser::new();
        let cmd = parser.parse_quick("zeig meine erinnerungen").unwrap();
        let AgentCommand::ListReminders { include_done } = cmd else {
            unreachable!("Expected ListReminders")
        };
        assert_eq!(include_done, Some(false));
    }

    #[test]
    fn parses_list_reminders_english() {
        let parser = CommandParser::new();
        let cmd = parser.parse_quick("show reminders").unwrap();
        let AgentCommand::ListReminders { include_done } = cmd else {
            unreachable!("Expected ListReminders")
        };
        assert_eq!(include_done, Some(false));
    }

    #[test]
    fn parses_list_reminders_with_all() {
        let parser = CommandParser::new();
        let cmd = parser.parse_quick("alle erinnerungen").unwrap();
        let AgentCommand::ListReminders { include_done } = cmd else {
            unreachable!("Expected ListReminders")
        };
        assert_eq!(include_done, Some(true));
    }

    #[test]
    fn parses_transit_german_wie_komme_ich() {
        let parser = CommandParser::new();

        let cmd = parser.parse_quick("wie komme ich nach TU Berlin").unwrap();
        let AgentCommand::SearchTransit { from, to, .. } = cmd else {
            unreachable!("Expected SearchTransit")
        };
        assert_eq!(to, "TU Berlin");
        assert!(from.is_empty());

        // With "zum" variant
        let cmd = parser
            .parse_quick("wie komme ich zum Hauptbahnhof")
            .unwrap();
        let AgentCommand::SearchTransit { to, .. } = cmd else {
            unreachable!("Expected SearchTransit")
        };
        assert_eq!(to, "Hauptbahnhof");
    }

    #[test]
    fn parses_transit_german_verbindung_nach() {
        let parser = CommandParser::new();
        let cmd = parser
            .parse_quick("verbindung nach Alexanderplatz")
            .unwrap();
        let AgentCommand::SearchTransit { to, .. } = cmd else {
            unreachable!("Expected SearchTransit")
        };
        assert_eq!(to, "Alexanderplatz");
    }

    #[test]
    fn parses_transit_english_how_do_i_get_to() {
        let parser = CommandParser::new();
        let cmd = parser
            .parse_quick("how do i get to Brandenburg Gate")
            .unwrap();
        let AgentCommand::SearchTransit { to, .. } = cmd else {
            unreachable!("Expected SearchTransit")
        };
        assert_eq!(to, "Brandenburg Gate");
    }

    #[test]
    fn parses_transit_oepnv_keyword() {
        let parser = CommandParser::new();
        let cmd = parser.parse_quick("öpnv nach Alexanderplatz").unwrap();
        let AgentCommand::SearchTransit { to, .. } = cmd else {
            unreachable!("Expected SearchTransit")
        };
        assert_eq!(to, "Alexanderplatz");
    }
}
