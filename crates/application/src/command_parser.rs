//! Command parser - Parse natural language into typed commands

use std::{fmt, sync::Arc};

use chrono::{NaiveDate, NaiveTime};
use domain::AgentCommand;
use serde::Deserialize;
use tracing::{debug, instrument, warn};

use crate::{error::ApplicationError, ports::InferencePort};

/// System prompt for intent detection
const INTENT_SYSTEM_PROMPT: &str = r#"You are an intent classifier for a personal assistant.
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
struct ParsedIntent {
    intent: String,
    #[serde(default)]
    date: Option<String>,
    #[serde(default)]
    time: Option<String>,
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    to: Option<String>,
    #[serde(default)]
    subject: Option<String>,
    #[serde(default)]
    body: Option<String>,
    #[serde(default)]
    question: Option<String>,
    #[serde(default)]
    count: Option<u32>,
    #[serde(default)]
    draft_id: Option<String>,
    #[serde(default)]
    query: Option<String>,
    #[serde(default)]
    max_results: Option<u32>,
    #[serde(default)]
    event_id: Option<String>,
    #[serde(default)]
    location: Option<String>,
    #[serde(default)]
    duration_minutes: Option<u32>,
    #[serde(default)]
    task_id: Option<String>,
    #[serde(default)]
    priority: Option<String>,
    #[serde(default)]
    status: Option<String>,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    list: Option<String>,
    #[serde(default)]
    name: Option<String>,
    // Reminder fields
    #[serde(default)]
    reminder_id: Option<String>,
    #[serde(default)]
    remind_at: Option<String>,
    #[serde(default)]
    include_done: Option<bool>,
    // Transit fields
    #[serde(default)]
    from: Option<String>,
    #[serde(default)]
    to_address: Option<String>,
    #[serde(default)]
    departure: Option<String>,
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

    /// Build the list of quick patterns
    fn build_quick_patterns() -> Vec<QuickPattern> {
        vec![
            // Echo command
            QuickPattern {
                keywords: vec!["echo", "sag", "sage"],
                builder: |input| {
                    let lower = input.to_lowercase();
                    for keyword in ["echo ", "sag ", "sage "] {
                        if lower.starts_with(keyword) {
                            // Get the original casing
                            let message = &input[keyword.len()..];
                            return Some(AgentCommand::Echo {
                                message: message.to_string(),
                            });
                        }
                    }
                    None
                },
            },
            // Help command
            QuickPattern {
                keywords: vec!["help", "?"],
                builder: |input| {
                    let lower = input.to_lowercase().trim().to_string();
                    if lower == "help" || lower == "?" {
                        return Some(AgentCommand::Help { command: None });
                    }
                    if let Some(topic) = lower.strip_prefix("help ") {
                        return Some(AgentCommand::Help {
                            command: Some(topic.trim().to_string()),
                        });
                    }
                    None
                },
            },
            // Status command
            QuickPattern {
                keywords: vec!["status", "ping"],
                builder: |input| {
                    let lower = input.to_lowercase().trim().to_string();
                    if lower == "status" || lower == "ping" {
                        return Some(AgentCommand::System(domain::SystemCommand::Status));
                    }
                    None
                },
            },
            // Version command
            QuickPattern {
                keywords: vec!["version"],
                builder: |input| {
                    if input.to_lowercase().trim() == "version" {
                        return Some(AgentCommand::System(domain::SystemCommand::Version));
                    }
                    None
                },
            },
            // Models command
            QuickPattern {
                keywords: vec!["models"],
                builder: |input| {
                    let lower = input.to_lowercase().trim().to_string();
                    if lower == "models" {
                        return Some(AgentCommand::System(domain::SystemCommand::ListModels));
                    }
                    None
                },
            },
            // Morning briefing
            QuickPattern {
                keywords: vec![
                    "briefing",
                    "morning",
                    "good morning",
                    "what's on",
                    "what is on",
                ],
                builder: |input| {
                    let lower = input.to_lowercase();
                    if lower.contains("briefing")
                        || lower == "good morning"
                        || lower.contains("what's on")
                        || lower.contains("what is on today")
                    {
                        // Parse date from input using date_parser
                        let date = crate::date_parser::extract_date_from_text(input);
                        return Some(AgentCommand::MorningBriefing { date });
                    }
                    None
                },
            },
            // Inbox summary
            QuickPattern {
                keywords: vec!["inbox", "mails", "e-mails", "emails"],
                builder: |input| {
                    let lower = input.to_lowercase();
                    if lower.contains("inbox")
                        || lower.contains("summarize mails")
                        || lower.contains("summarize email")
                    {
                        let only_important = lower.contains("important");
                        return Some(AgentCommand::SummarizeInbox {
                            count: None,
                            only_important: if only_important { Some(true) } else { None },
                        });
                    }
                    None
                },
            },
            // Web search
            QuickPattern {
                keywords: vec![
                    "suche im internet",
                    "such im internet",
                    "recherchiere",
                    "google",
                    "suche online",
                    "such online",
                    "finde heraus",
                    "was sagt das internet",
                    "search the web",
                    "search the internet",
                    "search online",
                    "look up",
                ],
                builder: |input| {
                    let lower = input.to_lowercase();

                    // Match patterns and extract the query
                    let query = Self::extract_search_query(&lower, input);
                    query.map(|q| AgentCommand::WebSearch {
                        query: q,
                        max_results: None,
                    })
                },
            },
            // List reminders
            QuickPattern {
                keywords: vec![
                    "erinnerungen",
                    "reminders",
                    "was steht an",
                ],
                builder: |input| {
                    let lower = input.to_lowercase();
                    if lower.contains("erinnerungen")
                        || lower.contains("reminders")
                        || lower.contains("was steht an")
                    {
                        let include_done = lower.contains("alle")
                            || lower.contains("all")
                            || lower.contains("erledigte")
                            || lower.contains("completed");
                        return Some(AgentCommand::ListReminders {
                            include_done: Some(include_done),
                        });
                    }
                    None
                },
            },
            // Transit search
            QuickPattern {
                keywords: vec![
                    "öpnv",
                    "verbindung",
                    "wie komme ich",
                    "how do i get to",
                    "transit to",
                    "directions to",
                    "route to",
                    "fahrt nach",
                    "bahn nach",
                    "bus nach",
                ],
                builder: |input| {
                    let lower = input.to_lowercase();

                    // Try to extract destination from common patterns
                    let destination = Self::extract_transit_destination(&lower, input);
                    destination.map(|to| AgentCommand::SearchTransit {
                        from: String::new(), // Empty means "from home/current location"
                        to,
                        departure: None,
                    })
                },
            },
        ]
    }

    /// Extract transit destination from input
    fn extract_transit_destination(lower: &str, original: &str) -> Option<String> {
        // Patterns to extract destination
        let prefixes = [
            "wie komme ich nach ",
            "wie komme ich zum ",
            "wie komme ich zur ",
            "wie komme ich zu ",
            "verbindung nach ",
            "verbindung zum ",
            "verbindung zur ",
            "verbindung zu ",
            "öpnv nach ",
            "öpnv zum ",
            "öpnv zur ",
            "öpnv zu ",
            "fahrt nach ",
            "fahrt zum ",
            "fahrt zur ",
            "bahn nach ",
            "bahn zum ",
            "bahn zur ",
            "bus nach ",
            "bus zum ",
            "bus zur ",
            "how do i get to ",
            "transit to ",
            "directions to ",
            "route to ",
        ];

        for prefix in prefixes {
            if lower.starts_with(prefix) {
                let dest = original[prefix.len()..].trim();
                // Remove trailing question mark
                let dest = dest.trim_end_matches('?').trim();
                if !dest.is_empty() {
                    return Some(dest.to_string());
                }
            }
        }

        // Also match patterns where trigger appears in middle
        for prefix in prefixes {
            if let Some(pos) = lower.find(prefix) {
                let dest = original[pos + prefix.len()..].trim();
                let dest = dest.trim_end_matches('?').trim();
                if !dest.is_empty() {
                    return Some(dest.to_string());
                }
            }
        }

        None
    }

    /// Extract search query from input based on matched pattern
    fn extract_search_query(lower: &str, original: &str) -> Option<String> {
        // Patterns with their prefixes to strip
        let prefixes = [
            "suche im internet nach ",
            "suche im internet ",
            "such im internet nach ",
            "such im internet ",
            "recherchiere nach ",
            "recherchiere ",
            "google nach ",
            "google ",
            "suche online nach ",
            "suche online ",
            "such online nach ",
            "such online ",
            "finde heraus ",
            "was sagt das internet zu ",
            "was sagt das internet über ",
            "was sagt das internet ",
            "search the web for ",
            "search the internet for ",
            "search online for ",
            "look up ",
        ];

        for prefix in prefixes {
            if lower.starts_with(prefix) {
                let query = original[prefix.len()..].trim().to_string();
                if !query.is_empty() {
                    return Some(query);
                }
            }
        }

        // Also match if keywords appear anywhere for shorter inputs
        let trigger_words = [
            "suche im internet",
            "such im internet",
            "recherchiere",
            "google",
            "suche online",
            "such online",
            "search the web",
            "search the internet",
            "search online",
        ];

        for trigger in trigger_words {
            if lower.contains(trigger) && lower.len() > trigger.len() + 3 {
                // Extract anything after the trigger word
                if let Some(pos) = lower.find(trigger) {
                    let after = &original[pos + trigger.len()..].trim();
                    // Clean up common connectors
                    let query = after
                        .trim_start_matches([' ', ':', '-', '–', '—'])
                        .trim_start_matches("nach ")
                        .trim_start_matches("for ")
                        .trim_start_matches("about ")
                        .trim();
                    if !query.is_empty() {
                        return Some(query.to_string());
                    }
                }
            }
        }

        None
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

    /// Parse using LLM for complex commands
    #[instrument(skip(self, inference, input), fields(input_len = input.len()))]
    pub async fn parse_with_llm(
        &self,
        inference: &Arc<dyn InferencePort>,
        input: &str,
    ) -> Result<AgentCommand, ApplicationError> {
        // First, try quick parsing
        if let Some(cmd) = self.parse_quick(input) {
            return Ok(cmd);
        }

        // Use LLM for intent detection
        debug!("No quick match, using LLM for intent detection");

        let result = inference
            .generate_with_system(INTENT_SYSTEM_PROMPT, input)
            .await?;

        // Try to parse the LLM response as JSON
        match self.parse_llm_response(&result.content, input) {
            Ok(cmd) => {
                debug!(command = ?cmd, "LLM-parsed command");
                Ok(cmd)
            },
            Err(e) => {
                warn!(error = %e, response = %result.content, "Failed to parse LLM intent response");
                // Fall back to Ask intent
                Ok(AgentCommand::Ask {
                    question: input.to_string(),
                })
            },
        }
    }

    /// Parse the LLM response JSON into an AgentCommand
    fn parse_llm_response(
        &self,
        response: &str,
        original_input: &str,
    ) -> Result<AgentCommand, String> {
        // Extract JSON from response (handle markdown code blocks)
        let json_str = Self::extract_json(response);

        let parsed: ParsedIntent =
            serde_json::from_str(json_str).map_err(|e| format!("JSON parse error: {e}"))?;

        self.intent_to_command(parsed, original_input)
    }

    /// Extract JSON from potentially markdown-wrapped response
    fn extract_json(response: &str) -> &str {
        let response = response.trim();

        // Handle ```json ... ``` blocks
        if let Some(start) = response.find("```json") {
            if let Some(end) = response[start + 7..].find("```") {
                return response[start + 7..start + 7 + end].trim();
            }
        }

        // Handle ``` ... ``` blocks
        if let Some(start) = response.find("```") {
            if let Some(end) = response[start + 3..].find("```") {
                return response[start + 3..start + 3 + end].trim();
            }
        }

        // Handle { ... } directly
        // Ensure start < end to avoid panics with malformed input like "} {"
        if let Some(start) = response.find('{') {
            if let Some(end) = response.rfind('}') {
                if start <= end {
                    return &response[start..=end];
                }
            }
        }

        response
    }

    /// Convert parsed intent to AgentCommand
    #[allow(clippy::unused_self, clippy::too_many_lines)]
    fn intent_to_command(
        &self,
        parsed: ParsedIntent,
        original_input: &str,
    ) -> Result<AgentCommand, String> {
        match parsed.intent.as_str() {
            "morning_briefing" => {
                let date = parsed
                    .date
                    .as_ref()
                    .and_then(|d| NaiveDate::parse_from_str(d, "%Y-%m-%d").ok());
                Ok(AgentCommand::MorningBriefing { date })
            },

            "create_calendar_event" => {
                let date = parsed
                    .date
                    .as_ref()
                    .ok_or("Missing date for calendar event")?;
                let time = parsed
                    .time
                    .as_ref()
                    .ok_or("Missing time for calendar event")?;
                let title = parsed
                    .title
                    .as_ref()
                    .ok_or("Missing title for calendar event")?;

                let date = NaiveDate::parse_from_str(date, "%Y-%m-%d")
                    .map_err(|e| format!("Invalid date format: {e}"))?;
                let time = NaiveTime::parse_from_str(time, "%H:%M")
                    .or_else(|_| NaiveTime::parse_from_str(time, "%H:%M:%S"))
                    .map_err(|e| format!("Invalid time format: {e}"))?;

                Ok(AgentCommand::CreateCalendarEvent {
                    date,
                    time,
                    title: title.clone(),
                    duration_minutes: Some(60),
                    attendees: None,
                    location: None,
                })
            },

            "update_calendar_event" => {
                let event_id = parsed
                    .event_id
                    .as_ref()
                    .ok_or("Missing event_id for calendar event update")?
                    .clone();

                // Parse optional date
                let date = parsed
                    .date
                    .as_ref()
                    .map(|d| {
                        NaiveDate::parse_from_str(d, "%Y-%m-%d")
                            .map_err(|e| format!("Invalid date format: {e}"))
                    })
                    .transpose()?;

                // Parse optional time
                let time = parsed
                    .time
                    .as_ref()
                    .map(|t| {
                        NaiveTime::parse_from_str(t, "%H:%M")
                            .or_else(|_| NaiveTime::parse_from_str(t, "%H:%M:%S"))
                            .map_err(|e| format!("Invalid time format: {e}"))
                    })
                    .transpose()?;

                Ok(AgentCommand::UpdateCalendarEvent {
                    event_id,
                    date,
                    time,
                    title: parsed.title.clone(),
                    duration_minutes: parsed.duration_minutes,
                    attendees: None,
                    location: parsed.location.clone(),
                })
            },

            "list_tasks" => {
                // Parse optional status filter
                let status = parsed
                    .status
                    .as_ref()
                    .map(|s| s.parse::<domain::TaskStatus>())
                    .transpose()
                    .map_err(|_| "Invalid task status")?;

                // Parse optional priority filter
                let priority = parsed
                    .priority
                    .as_ref()
                    .map(|p| match p.to_lowercase().as_str() {
                        "high" => Ok(domain::Priority::High),
                        "medium" | "med" => Ok(domain::Priority::Medium),
                        "low" => Ok(domain::Priority::Low),
                        _ => Err("Invalid priority"),
                    })
                    .transpose()?;

                Ok(AgentCommand::ListTasks {
                    status,
                    priority,
                    list: parsed.list.clone(),
                })
            },

            "create_task" => {
                let title = parsed
                    .title
                    .as_ref()
                    .ok_or("Missing title for task")?
                    .clone();

                // Parse optional due date
                let due_date = parsed
                    .date
                    .as_ref()
                    .map(|d| {
                        NaiveDate::parse_from_str(d, "%Y-%m-%d")
                            .map_err(|e| format!("Invalid date format: {e}"))
                    })
                    .transpose()?;

                // Parse optional priority
                let priority = parsed
                    .priority
                    .as_ref()
                    .map(|p| match p.to_lowercase().as_str() {
                        "high" => Ok(domain::Priority::High),
                        "medium" | "med" => Ok(domain::Priority::Medium),
                        "low" => Ok(domain::Priority::Low),
                        _ => Err("Invalid priority"),
                    })
                    .transpose()?;

                Ok(AgentCommand::CreateTask {
                    title,
                    due_date,
                    priority,
                    description: parsed.description.clone(),
                    list: parsed.list.clone(),
                })
            },

            "list_task_lists" => Ok(AgentCommand::ListTaskLists),

            "create_task_list" => {
                let name = parsed
                    .name
                    .as_ref()
                    .or(parsed.title.as_ref())
                    .ok_or("Missing name for task list")?
                    .clone();

                Ok(AgentCommand::CreateTaskList { name })
            },

            "complete_task" => {
                let task_id = parsed
                    .task_id
                    .as_ref()
                    .ok_or("Missing task_id for complete_task")?
                    .clone();

                Ok(AgentCommand::CompleteTask { task_id })
            },

            "update_task" => {
                let task_id = parsed
                    .task_id
                    .as_ref()
                    .ok_or("Missing task_id for update_task")?
                    .clone();

                // Parse optional due date - Some(None) clears, None keeps existing
                let due_date = parsed.date.as_ref().map(|d| {
                    if d.is_empty() || d == "none" || d == "null" {
                        None
                    } else {
                        NaiveDate::parse_from_str(d, "%Y-%m-%d").ok()
                    }
                });

                // Parse optional priority
                let priority = parsed
                    .priority
                    .as_ref()
                    .map(|p| match p.to_lowercase().as_str() {
                        "high" => domain::Priority::High,
                        "medium" | "med" => domain::Priority::Medium,
                        _ => domain::Priority::Low,
                    });

                // Parse optional description - Some(None) clears, None keeps existing
                let description = parsed.description.as_ref().map(|d| {
                    if d.is_empty() || d == "none" || d == "null" {
                        None
                    } else {
                        Some(d.clone())
                    }
                });

                Ok(AgentCommand::UpdateTask {
                    task_id,
                    title: parsed.title.clone(),
                    due_date,
                    priority,
                    description,
                })
            },

            "delete_task" => {
                let task_id = parsed
                    .task_id
                    .as_ref()
                    .ok_or("Missing task_id for delete_task")?
                    .clone();

                Ok(AgentCommand::DeleteTask { task_id })
            },

            "summarize_inbox" => Ok(AgentCommand::SummarizeInbox {
                count: parsed.count,
                only_important: None,
            }),

            "draft_email" => {
                let to_str = parsed.to.as_ref().ok_or("Missing recipient for email")?;
                let to = domain::EmailAddress::new(to_str)
                    .map_err(|e| format!("Invalid email address: {e}"))?;
                let body = parsed
                    .body
                    .as_ref()
                    .ok_or("Missing body for email")?
                    .clone();

                Ok(AgentCommand::DraftEmail {
                    to,
                    subject: parsed.subject,
                    body,
                })
            },

            "send_email" => {
                let draft_id = parsed
                    .draft_id
                    .as_ref()
                    .ok_or("Missing draft_id for send_email")?
                    .clone();
                Ok(AgentCommand::SendEmail { draft_id })
            },

            "web_search" => {
                let query = parsed
                    .query
                    .as_ref()
                    .ok_or("Missing query for web_search")?
                    .clone();
                Ok(AgentCommand::WebSearch {
                    query,
                    max_results: parsed.max_results,
                })
            },

            "create_reminder" => {
                let title = parsed
                    .title
                    .as_ref()
                    .ok_or("Missing title for reminder")?
                    .clone();
                let remind_at = parsed
                    .remind_at
                    .as_ref()
                    .ok_or("Missing remind_at time for reminder")?
                    .clone();
                Ok(AgentCommand::CreateReminder {
                    title,
                    description: parsed.description.clone(),
                    remind_at,
                })
            },

            "list_reminders" => Ok(AgentCommand::ListReminders {
                include_done: parsed.include_done,
            }),

            "snooze_reminder" => {
                let reminder_id = parsed
                    .reminder_id
                    .as_ref()
                    .ok_or("Missing reminder_id for snooze")?
                    .clone();
                Ok(AgentCommand::SnoozeReminder {
                    reminder_id,
                    duration_minutes: parsed.duration_minutes,
                })
            },

            "acknowledge_reminder" => {
                let reminder_id = parsed
                    .reminder_id
                    .as_ref()
                    .ok_or("Missing reminder_id for acknowledge")?
                    .clone();
                Ok(AgentCommand::AcknowledgeReminder { reminder_id })
            },

            "delete_reminder" => {
                let reminder_id = parsed
                    .reminder_id
                    .as_ref()
                    .ok_or("Missing reminder_id for delete")?
                    .clone();
                Ok(AgentCommand::DeleteReminder { reminder_id })
            },

            "search_transit" => {
                let to_address = parsed
                    .to_address
                    .as_ref()
                    .ok_or("Missing destination for transit search")?
                    .clone();
                Ok(AgentCommand::SearchTransit {
                    from: parsed.from.clone().unwrap_or_default(),
                    to: to_address,
                    departure: parsed.departure.clone(),
                })
            },

            _ => {
                // "ask" or any unknown intent falls back to Ask command
                let question = parsed
                    .question
                    .unwrap_or_else(|| original_input.to_string());
                Ok(AgentCommand::Ask { question })
            },
        }
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
        let cmd = parser.parse_quick("sag Hallo Welt").unwrap();

        let AgentCommand::Echo { message } = cmd else {
            unreachable!("Expected Echo command")
        };
        assert_eq!(message, "Hallo Welt");
    }

    #[test]
    fn parses_echo_with_sage() {
        let parser = CommandParser::new();
        let cmd = parser.parse_quick("sage Guten Tag").unwrap();

        let AgentCommand::Echo { message } = cmd else {
            unreachable!("Expected Echo command")
        };
        assert_eq!(message, "Guten Tag");
    }

    #[test]
    fn parses_help_command() {
        let parser = CommandParser::new();

        let cmd = parser.parse_quick("help").unwrap();
        assert!(matches!(cmd, AgentCommand::Help { command: None }));

        let cmd = parser.parse_quick("help email").unwrap();
        let AgentCommand::Help {
            command: Some(topic),
        } = cmd
        else {
            unreachable!("Expected Help command with topic")
        };
        assert_eq!(topic, "email");
    }

    #[test]
    fn parses_help_with_help_keyword() {
        let parser = CommandParser::new();
        let cmd = parser.parse_quick("help").unwrap();
        assert!(matches!(cmd, AgentCommand::Help { command: None }));
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

        assert!(matches!(cmd, AgentCommand::MorningBriefing { date: None }));
    }

    #[test]
    fn parses_briefing_with_good_morning() {
        let parser = CommandParser::new();
        let cmd = parser.parse_quick("good morning").unwrap();

        assert!(matches!(cmd, AgentCommand::MorningBriefing { date: None }));
    }

    #[test]
    fn unknown_input_returns_none() {
        let parser = CommandParser::new();
        let cmd = parser.parse_quick("some random text");

        assert!(cmd.is_none());
    }

    #[test]
    fn parse_quick_is_case_insensitive() {
        let parser = CommandParser::new();

        let cmd = parser.parse_quick("ECHO Test").unwrap();
        assert!(matches!(cmd, AgentCommand::Echo { .. }));

        let cmd = parser.parse_quick("HELP").unwrap();
        assert!(matches!(cmd, AgentCommand::Help { .. }));

        let cmd = parser.parse_quick("STATUS").unwrap();
        assert!(matches!(
            cmd,
            AgentCommand::System(domain::SystemCommand::Status)
        ));
    }

    #[test]
    fn parse_quick_preserves_original_case_in_message() {
        let parser = CommandParser::new();
        let cmd = parser.parse_quick("echo HeLLo WoRLd").unwrap();

        let AgentCommand::Echo { message } = cmd else {
            unreachable!("Expected Echo command")
        };
        assert_eq!(message, "HeLLo WoRLd");
    }

    #[test]
    fn command_parser_debug_output() {
        let parser = CommandParser::new();
        let debug_str = format!("{parser:?}");
        assert!(debug_str.contains("CommandParser"));
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
        let cmd = parser.parse_quick("MODELS").unwrap();
        assert!(matches!(
            cmd,
            AgentCommand::System(domain::SystemCommand::ListModels)
        ));
    }

    #[test]
    fn parses_inbox_command() {
        let parser = CommandParser::new();
        let cmd = parser.parse_quick("inbox").unwrap();
        assert!(matches!(
            cmd,
            AgentCommand::SummarizeInbox {
                count: None,
                only_important: None
            }
        ));
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
        let cmd = parser.parse_quick("inbox important only").unwrap();
        let AgentCommand::SummarizeInbox { only_important, .. } = cmd else {
            unreachable!("Expected SummarizeInbox")
        };
        assert_eq!(only_important, Some(true));
    }

    #[test]
    fn parses_whats_on() {
        let parser = CommandParser::new();
        // The pattern checks for "what's on" in the input
        let cmd = parser.parse_quick("what's on").unwrap();
        assert!(matches!(cmd, AgentCommand::MorningBriefing { date: None }));
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
        let debug_str = format!("{parser:?}");
        assert!(debug_str.contains("CommandParser"));
    }

    #[test]
    fn parser_has_quick_patterns() {
        let parser = CommandParser::new();
        // The debug output shows the pattern count
        let debug_str = format!("{parser:?}");
        assert!(debug_str.contains("quick_patterns_count"));
    }

    #[test]
    fn help_with_topic_email() {
        let parser = CommandParser::new();
        let cmd = parser.parse_quick("help email").unwrap();
        let AgentCommand::Help {
            command: Some(topic),
        } = cmd
        else {
            unreachable!("Expected Help with topic")
        };
        assert_eq!(topic, "email");
    }

    #[test]
    fn help_with_topic_calendar() {
        let parser = CommandParser::new();
        let cmd = parser.parse_quick("help calendar").unwrap();
        let AgentCommand::Help {
            command: Some(topic),
        } = cmd
        else {
            unreachable!("Expected Help with topic")
        };
        assert_eq!(topic, "calendar");
    }

    #[test]
    fn parses_good_morning() {
        let parser = CommandParser::new();
        let cmd = parser.parse_quick("good morning").unwrap();
        assert!(matches!(cmd, AgentCommand::MorningBriefing { date: None }));
    }

    #[test]
    fn parses_what_is_on_today() {
        let parser = CommandParser::new();
        // The pattern checks for "what is on" - needs to be exact match in lower case
        let cmd = parser.parse_quick("What is on today?").unwrap();
        assert!(matches!(cmd, AgentCommand::MorningBriefing { date: None }));
    }

    #[test]
    fn parses_emails_inbox() {
        let parser = CommandParser::new();
        // The pattern checks for "inbox" or "mails zusammen" or "email zusammen"
        let cmd = parser.parse_quick("emails inbox zeigen").unwrap();
        assert!(matches!(cmd, AgentCommand::SummarizeInbox { .. }));
    }

    #[test]
    fn parses_list_reminders_german() {
        let parser = CommandParser::new();
        let cmd = parser.parse_quick("meine erinnerungen").unwrap();
        let AgentCommand::ListReminders { include_done } = cmd else {
            unreachable!("Expected ListReminders")
        };
        assert_eq!(include_done, Some(false));
    }

    #[test]
    fn parses_list_reminders_english() {
        let parser = CommandParser::new();
        let cmd = parser.parse_quick("show my reminders").unwrap();
        let AgentCommand::ListReminders { include_done } = cmd else {
            unreachable!("Expected ListReminders")
        };
        assert_eq!(include_done, Some(false));
    }

    #[test]
    fn parses_list_reminders_with_all() {
        let parser = CommandParser::new();
        let cmd = parser.parse_quick("zeige alle erinnerungen").unwrap();
        let AgentCommand::ListReminders { include_done } = cmd else {
            unreachable!("Expected ListReminders")
        };
        assert_eq!(include_done, Some(true));
    }

    #[test]
    fn parses_transit_german_wie_komme_ich() {
        let parser = CommandParser::new();
        let cmd = parser.parse_quick("Wie komme ich nach Berlin Hauptbahnhof?").unwrap();
        let AgentCommand::SearchTransit { from, to, departure } = cmd else {
            unreachable!("Expected SearchTransit")
        };
        assert!(from.is_empty()); // Default from home
        assert_eq!(to, "Berlin Hauptbahnhof");
        assert!(departure.is_none());
    }

    #[test]
    fn parses_transit_german_verbindung_nach() {
        let parser = CommandParser::new();
        let cmd = parser.parse_quick("verbindung nach München").unwrap();
        let AgentCommand::SearchTransit { from, to, .. } = cmd else {
            unreachable!("Expected SearchTransit")
        };
        assert_eq!(to, "München");
    }

    #[test]
    fn parses_transit_english_how_do_i_get_to() {
        let parser = CommandParser::new();
        let cmd = parser.parse_quick("how do i get to Central Station?").unwrap();
        let AgentCommand::SearchTransit { to, .. } = cmd else {
            unreachable!("Expected SearchTransit")
        };
        assert_eq!(to, "Central Station");
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

#[cfg(test)]
mod async_tests {
    use std::sync::Arc;

    use mockall::mock;

    use super::*;
    use crate::ports::InferenceResult;

    mock! {
        pub InferenceEngine {}

        #[async_trait::async_trait]
        impl InferencePort for InferenceEngine {
            async fn generate(&self, message: &str) -> Result<InferenceResult, ApplicationError>;
            async fn generate_with_context(&self, conversation: &domain::Conversation) -> Result<InferenceResult, ApplicationError>;
            async fn generate_with_system(&self, system_prompt: &str, message: &str) -> Result<InferenceResult, ApplicationError>;
            async fn generate_stream(&self, message: &str) -> Result<crate::ports::InferenceStream, ApplicationError>;
            async fn generate_stream_with_system(&self, system_prompt: &str, message: &str) -> Result<crate::ports::InferenceStream, ApplicationError>;
            async fn is_healthy(&self) -> bool;
            fn current_model(&self) -> String;
            async fn list_available_models(&self) -> Result<Vec<String>, ApplicationError>;
            async fn switch_model(&self, model_name: &str) -> Result<(), ApplicationError>;
        }
    }

    #[tokio::test]
    async fn parse_with_llm_quick_pattern() {
        let parser = CommandParser::new();
        let mock = MockInferenceEngine::new();
        let inference: Arc<dyn InferencePort> = Arc::new(mock);

        let result = parser.parse_with_llm(&inference, "help").await.unwrap();
        assert!(matches!(result, AgentCommand::Help { command: None }));
    }

    #[tokio::test]
    async fn parse_with_llm_echo() {
        let parser = CommandParser::new();
        let mock = MockInferenceEngine::new();
        let inference: Arc<dyn InferencePort> = Arc::new(mock);

        let result = parser
            .parse_with_llm(&inference, "echo hello")
            .await
            .unwrap();
        let AgentCommand::Echo { message } = result else {
            unreachable!("Expected Echo command");
        };
        assert!(message.contains("hello"));
    }

    #[tokio::test]
    async fn parse_with_llm_status() {
        let parser = CommandParser::new();
        let mock = MockInferenceEngine::new();
        let inference: Arc<dyn InferencePort> = Arc::new(mock);

        let result = parser.parse_with_llm(&inference, "status").await.unwrap();
        assert!(matches!(
            result,
            AgentCommand::System(domain::SystemCommand::Status)
        ));
    }

    #[tokio::test]
    async fn parse_with_llm_unknown_becomes_ask() {
        let parser = CommandParser::new();
        let mut mock = MockInferenceEngine::new();

        // Set up expectation for generate_with_system
        mock.expect_generate_with_system().returning(|_, msg| {
            Ok(InferenceResult {
                content: format!(r#"{{"intent":"ask","question":"{msg}"}}"#),
                model: "test".to_string(),
                tokens_used: Some(10),
                latency_ms: 50,
            })
        });

        let inference: Arc<dyn InferencePort> = Arc::new(mock);

        let result = parser
            .parse_with_llm(&inference, "was ist der Sinn des Lebens?")
            .await
            .unwrap();
        let AgentCommand::Ask { question } = result else {
            unreachable!("Expected Ask command");
        };
        assert!(question.contains("Sinn"));
    }

    #[tokio::test]
    async fn parse_with_llm_briefing() {
        let parser = CommandParser::new();
        let mock = MockInferenceEngine::new();
        let inference: Arc<dyn InferencePort> = Arc::new(mock);

        let result = parser.parse_with_llm(&inference, "briefing").await.unwrap();
        assert!(matches!(
            result,
            AgentCommand::MorningBriefing { date: None }
        ));
    }

    #[tokio::test]
    async fn parse_with_llm_version() {
        let parser = CommandParser::new();
        let mock = MockInferenceEngine::new();
        let inference: Arc<dyn InferencePort> = Arc::new(mock);

        let result = parser.parse_with_llm(&inference, "version").await.unwrap();
        assert!(matches!(
            result,
            AgentCommand::System(domain::SystemCommand::Version)
        ));
    }

    #[tokio::test]
    async fn parse_with_llm_models() {
        let parser = CommandParser::new();
        let mock = MockInferenceEngine::new();
        let inference: Arc<dyn InferencePort> = Arc::new(mock);

        let result = parser.parse_with_llm(&inference, "models").await.unwrap();
        assert!(matches!(
            result,
            AgentCommand::System(domain::SystemCommand::ListModels)
        ));
    }

    #[tokio::test]
    async fn parse_with_llm_inbox() {
        let parser = CommandParser::new();
        let mock = MockInferenceEngine::new();
        let inference: Arc<dyn InferencePort> = Arc::new(mock);

        let result = parser.parse_with_llm(&inference, "inbox").await.unwrap();
        assert!(matches!(result, AgentCommand::SummarizeInbox { .. }));
    }

    // Tests for LLM response parsing

    #[test]
    fn extract_json_plain() {
        let json = r#"{"intent":"ask","question":"test"}"#;
        assert_eq!(CommandParser::extract_json(json), json);
    }

    #[test]
    fn extract_json_with_code_block() {
        let response = r#"```json
{"intent":"ask","question":"test"}
```"#;
        assert_eq!(
            CommandParser::extract_json(response),
            r#"{"intent":"ask","question":"test"}"#
        );
    }

    #[test]
    fn extract_json_with_plain_code_block() {
        let response = r#"```
{"intent":"morning_briefing"}
```"#;
        assert_eq!(
            CommandParser::extract_json(response),
            r#"{"intent":"morning_briefing"}"#
        );
    }

    #[test]
    fn extract_json_with_surrounding_text() {
        let response = r#"Here is the result: {"intent":"ask","question":"hello"} as requested."#;
        assert_eq!(
            CommandParser::extract_json(response),
            r#"{"intent":"ask","question":"hello"}"#
        );
    }

    #[test]
    fn parse_llm_response_morning_briefing() {
        let parser = CommandParser::new();
        let response = r#"{"intent":"morning_briefing"}"#;
        let cmd = parser.parse_llm_response(response, "").unwrap();
        assert!(matches!(cmd, AgentCommand::MorningBriefing { date: None }));
    }

    #[test]
    fn parse_llm_response_morning_briefing_with_date() {
        let parser = CommandParser::new();
        let response = r#"{"intent":"morning_briefing","date":"2025-02-15"}"#;
        let cmd = parser.parse_llm_response(response, "").unwrap();
        let AgentCommand::MorningBriefing { date } = cmd else {
            unreachable!("Expected MorningBriefing")
        };
        assert!(date.is_some());
        assert_eq!(date.unwrap().to_string(), "2025-02-15");
    }

    #[test]
    fn parse_llm_response_summarize_inbox() {
        let parser = CommandParser::new();
        let response = r#"{"intent":"summarize_inbox","count":5}"#;
        let cmd = parser.parse_llm_response(response, "").unwrap();
        let AgentCommand::SummarizeInbox { count, .. } = cmd else {
            unreachable!("Expected SummarizeInbox")
        };
        assert_eq!(count, Some(5));
    }

    #[test]
    fn parse_llm_response_ask() {
        let parser = CommandParser::new();
        let response = r#"{"intent":"ask","question":"What is the weather?"}"#;
        let cmd = parser.parse_llm_response(response, "").unwrap();
        let AgentCommand::Ask { question } = cmd else {
            unreachable!("Expected Ask")
        };
        assert_eq!(question, "What is the weather?");
    }

    #[test]
    fn parse_llm_response_ask_fallback() {
        let parser = CommandParser::new();
        let response = r#"{"intent":"unknown_intent"}"#;
        let cmd = parser
            .parse_llm_response(response, "original input")
            .unwrap();
        let AgentCommand::Ask { question } = cmd else {
            unreachable!("Expected Ask")
        };
        assert_eq!(question, "original input");
    }

    #[test]
    fn parse_llm_response_create_calendar_event() {
        let parser = CommandParser::new();
        let response = r#"{"intent":"create_calendar_event","date":"2025-02-20","time":"14:00","title":"Team Meeting"}"#;
        let cmd = parser.parse_llm_response(response, "").unwrap();
        let AgentCommand::CreateCalendarEvent {
            date, time, title, ..
        } = cmd
        else {
            unreachable!("Expected CreateCalendarEvent")
        };
        assert_eq!(date.to_string(), "2025-02-20");
        assert_eq!(time.to_string(), "14:00:00");
        assert_eq!(title, "Team Meeting");
    }

    #[test]
    fn parse_llm_response_create_calendar_event_missing_date() {
        let parser = CommandParser::new();
        let response = r#"{"intent":"create_calendar_event","time":"14:00","title":"Meeting"}"#;
        let result = parser.parse_llm_response(response, "");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Missing date"));
    }

    #[test]
    fn parse_llm_response_create_calendar_event_missing_time() {
        let parser = CommandParser::new();
        let response =
            r#"{"intent":"create_calendar_event","date":"2025-02-20","title":"Meeting"}"#;
        let result = parser.parse_llm_response(response, "");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Missing time"));
    }

    #[test]
    fn parse_llm_response_create_calendar_event_missing_title() {
        let parser = CommandParser::new();
        let response = r#"{"intent":"create_calendar_event","date":"2025-02-20","time":"14:00"}"#;
        let result = parser.parse_llm_response(response, "");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Missing title"));
    }

    #[test]
    fn parse_llm_response_draft_email() {
        let parser = CommandParser::new();
        let response = r#"{"intent":"draft_email","to":"test@example.com","subject":"Hello","body":"Test message"}"#;
        let cmd = parser.parse_llm_response(response, "").unwrap();
        let AgentCommand::DraftEmail { to, subject, body } = cmd else {
            unreachable!("Expected DraftEmail")
        };
        assert_eq!(to.to_string(), "test@example.com");
        assert_eq!(subject, Some("Hello".to_string()));
        assert_eq!(body, "Test message");
    }

    #[test]
    fn parse_llm_response_draft_email_missing_to() {
        let parser = CommandParser::new();
        let response = r#"{"intent":"draft_email","body":"Test message"}"#;
        let result = parser.parse_llm_response(response, "");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Missing recipient"));
    }

    #[test]
    fn parse_llm_response_draft_email_invalid_email() {
        let parser = CommandParser::new();
        let response = r#"{"intent":"draft_email","to":"invalid-email","body":"Test"}"#;
        let result = parser.parse_llm_response(response, "");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Invalid email"));
    }

    #[test]
    fn parse_llm_response_send_email() {
        let parser = CommandParser::new();
        let response = r#"{"intent":"send_email","draft_id":"draft-123"}"#;
        let cmd = parser.parse_llm_response(response, "").unwrap();
        let AgentCommand::SendEmail { draft_id } = cmd else {
            unreachable!("Expected SendEmail")
        };
        assert_eq!(draft_id, "draft-123");
    }

    #[test]
    fn parse_llm_response_send_email_missing_draft_id() {
        let parser = CommandParser::new();
        let response = r#"{"intent":"send_email"}"#;
        let result = parser.parse_llm_response(response, "");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Missing draft_id"));
    }

    #[test]
    fn parse_llm_response_web_search() {
        let parser = CommandParser::new();
        let response = r#"{"intent":"web_search","query":"Rust async patterns"}"#;
        let cmd = parser.parse_llm_response(response, "").unwrap();
        let AgentCommand::WebSearch { query, max_results } = cmd else {
            unreachable!("Expected WebSearch")
        };
        assert_eq!(query, "Rust async patterns");
        assert!(max_results.is_none());
    }

    #[test]
    fn parse_llm_response_web_search_with_max_results() {
        let parser = CommandParser::new();
        let response = r#"{"intent":"web_search","query":"climate change","max_results":10}"#;
        let cmd = parser.parse_llm_response(response, "").unwrap();
        let AgentCommand::WebSearch { query, max_results } = cmd else {
            unreachable!("Expected WebSearch")
        };
        assert_eq!(query, "climate change");
        assert_eq!(max_results, Some(10));
    }

    #[test]
    fn parse_llm_response_web_search_missing_query() {
        let parser = CommandParser::new();
        let response = r#"{"intent":"web_search"}"#;
        let result = parser.parse_llm_response(response, "");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Missing query"));
    }

    // =========================================================================
    // Reminder Intent Tests
    // =========================================================================

    #[test]
    fn parse_llm_response_create_reminder() {
        let parser = CommandParser::new();
        let response = r#"{"intent":"create_reminder","title":"Call mom","remind_at":"2025-02-20T18:00:00"}"#;
        let cmd = parser.parse_llm_response(response, "").unwrap();
        let AgentCommand::CreateReminder {
            title,
            remind_at,
            description,
        } = cmd
        else {
            unreachable!("Expected CreateReminder")
        };
        assert_eq!(title, "Call mom");
        assert_eq!(remind_at, "2025-02-20T18:00:00");
        assert!(description.is_none());
    }

    #[test]
    fn parse_llm_response_create_reminder_with_description() {
        let parser = CommandParser::new();
        let response = r#"{"intent":"create_reminder","title":"Meeting","remind_at":"2025-02-20T14:00","description":"Preparation needed"}"#;
        let cmd = parser.parse_llm_response(response, "").unwrap();
        let AgentCommand::CreateReminder {
            title,
            description,
            ..
        } = cmd
        else {
            unreachable!("Expected CreateReminder")
        };
        assert_eq!(title, "Meeting");
        assert_eq!(description, Some("Preparation needed".to_string()));
    }

    #[test]
    fn parse_llm_response_create_reminder_missing_title() {
        let parser = CommandParser::new();
        let response = r#"{"intent":"create_reminder","remind_at":"2025-02-20T18:00:00"}"#;
        let result = parser.parse_llm_response(response, "");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Missing title"));
    }

    #[test]
    fn parse_llm_response_create_reminder_missing_remind_at() {
        let parser = CommandParser::new();
        let response = r#"{"intent":"create_reminder","title":"Test reminder"}"#;
        let result = parser.parse_llm_response(response, "");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Missing remind_at"));
    }

    #[test]
    fn parse_llm_response_list_reminders() {
        let parser = CommandParser::new();
        let response = r#"{"intent":"list_reminders"}"#;
        let cmd = parser.parse_llm_response(response, "").unwrap();
        let AgentCommand::ListReminders { include_done } = cmd else {
            unreachable!("Expected ListReminders")
        };
        assert!(include_done.is_none());
    }

    #[test]
    fn parse_llm_response_list_reminders_with_done() {
        let parser = CommandParser::new();
        let response = r#"{"intent":"list_reminders","include_done":true}"#;
        let cmd = parser.parse_llm_response(response, "").unwrap();
        let AgentCommand::ListReminders { include_done } = cmd else {
            unreachable!("Expected ListReminders")
        };
        assert_eq!(include_done, Some(true));
    }

    #[test]
    fn parse_llm_response_snooze_reminder() {
        let parser = CommandParser::new();
        let response = r#"{"intent":"snooze_reminder","reminder_id":"rem-123"}"#;
        let cmd = parser.parse_llm_response(response, "").unwrap();
        let AgentCommand::SnoozeReminder {
            reminder_id,
            duration_minutes,
        } = cmd
        else {
            unreachable!("Expected SnoozeReminder")
        };
        assert_eq!(reminder_id, "rem-123");
        assert!(duration_minutes.is_none());
    }

    #[test]
    fn parse_llm_response_snooze_reminder_with_duration() {
        let parser = CommandParser::new();
        let response = r#"{"intent":"snooze_reminder","reminder_id":"rem-123","duration_minutes":30}"#;
        let cmd = parser.parse_llm_response(response, "").unwrap();
        let AgentCommand::SnoozeReminder {
            reminder_id,
            duration_minutes,
        } = cmd
        else {
            unreachable!("Expected SnoozeReminder")
        };
        assert_eq!(reminder_id, "rem-123");
        assert_eq!(duration_minutes, Some(30));
    }

    #[test]
    fn parse_llm_response_snooze_reminder_missing_id() {
        let parser = CommandParser::new();
        let response = r#"{"intent":"snooze_reminder"}"#;
        let result = parser.parse_llm_response(response, "");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Missing reminder_id"));
    }

    #[test]
    fn parse_llm_response_acknowledge_reminder() {
        let parser = CommandParser::new();
        let response = r#"{"intent":"acknowledge_reminder","reminder_id":"rem-456"}"#;
        let cmd = parser.parse_llm_response(response, "").unwrap();
        let AgentCommand::AcknowledgeReminder { reminder_id } = cmd else {
            unreachable!("Expected AcknowledgeReminder")
        };
        assert_eq!(reminder_id, "rem-456");
    }

    #[test]
    fn parse_llm_response_acknowledge_reminder_missing_id() {
        let parser = CommandParser::new();
        let response = r#"{"intent":"acknowledge_reminder"}"#;
        let result = parser.parse_llm_response(response, "");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Missing reminder_id"));
    }

    #[test]
    fn parse_llm_response_delete_reminder() {
        let parser = CommandParser::new();
        let response = r#"{"intent":"delete_reminder","reminder_id":"rem-789"}"#;
        let cmd = parser.parse_llm_response(response, "").unwrap();
        let AgentCommand::DeleteReminder { reminder_id } = cmd else {
            unreachable!("Expected DeleteReminder")
        };
        assert_eq!(reminder_id, "rem-789");
    }

    #[test]
    fn parse_llm_response_delete_reminder_missing_id() {
        let parser = CommandParser::new();
        let response = r#"{"intent":"delete_reminder"}"#;
        let result = parser.parse_llm_response(response, "");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Missing reminder_id"));
    }

    // =========================================================================
    // Transit Intent Tests
    // =========================================================================

    #[test]
    fn parse_llm_response_search_transit() {
        let parser = CommandParser::new();
        let response = r#"{"intent":"search_transit","to_address":"Berlin Hauptbahnhof"}"#;
        let cmd = parser.parse_llm_response(response, "").unwrap();
        let AgentCommand::SearchTransit {
            from,
            to,
            departure,
        } = cmd
        else {
            unreachable!("Expected SearchTransit")
        };
        assert!(from.is_empty());
        assert_eq!(to, "Berlin Hauptbahnhof");
        assert!(departure.is_none());
    }

    #[test]
    fn parse_llm_response_search_transit_with_from() {
        let parser = CommandParser::new();
        let response = r#"{"intent":"search_transit","from":"Alexanderplatz","to_address":"Potsdamer Platz"}"#;
        let cmd = parser.parse_llm_response(response, "").unwrap();
        let AgentCommand::SearchTransit { from, to, .. } = cmd else {
            unreachable!("Expected SearchTransit")
        };
        assert_eq!(from, "Alexanderplatz");
        assert_eq!(to, "Potsdamer Platz");
    }

    #[test]
    fn parse_llm_response_search_transit_with_departure() {
        let parser = CommandParser::new();
        let response =
            r#"{"intent":"search_transit","to_address":"Munich","departure":"2025-02-20T09:00"}"#;
        let cmd = parser.parse_llm_response(response, "").unwrap();
        let AgentCommand::SearchTransit { departure, .. } = cmd else {
            unreachable!("Expected SearchTransit")
        };
        assert_eq!(departure, Some("2025-02-20T09:00".to_string()));
    }

    #[test]
    fn parse_llm_response_search_transit_missing_destination() {
        let parser = CommandParser::new();
        let response = r#"{"intent":"search_transit","from":"Berlin"}"#;
        let result = parser.parse_llm_response(response, "");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Missing destination"));
    }

    #[test]
    fn parse_llm_response_invalid_json() {
        let parser = CommandParser::new();
        let response = "not json at all";
        let result = parser.parse_llm_response(response, "");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("JSON parse error"));
    }

    #[test]
    fn parse_llm_response_invalid_date_format() {
        let parser = CommandParser::new();
        let response = r#"{"intent":"create_calendar_event","date":"20-02-2025","time":"14:00","title":"Meeting"}"#;
        let result = parser.parse_llm_response(response, "");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Invalid date format"));
    }

    #[test]
    fn parse_llm_response_invalid_time_format() {
        let parser = CommandParser::new();
        let response = r#"{"intent":"create_calendar_event","date":"2025-02-20","time":"2pm","title":"Meeting"}"#;
        let result = parser.parse_llm_response(response, "");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Invalid time format"));
    }

    #[test]
    fn parse_llm_response_time_with_seconds() {
        let parser = CommandParser::new();
        let response = r#"{"intent":"create_calendar_event","date":"2025-02-20","time":"14:30:00","title":"Meeting"}"#;
        let cmd = parser.parse_llm_response(response, "").unwrap();
        let AgentCommand::CreateCalendarEvent { time, .. } = cmd else {
            unreachable!("Expected CreateCalendarEvent")
        };
        assert_eq!(time.to_string(), "14:30:00");
    }

    #[test]
    fn intent_system_prompt_is_valid() {
        // Check prompt has required content
        assert!(INTENT_SYSTEM_PROMPT.len() > 100);
        assert!(INTENT_SYSTEM_PROMPT.contains("intent"));
        assert!(INTENT_SYSTEM_PROMPT.contains("JSON"));
        assert!(INTENT_SYSTEM_PROMPT.contains("web_search"));
    }

    // =========================================================================
    // Task Management Tests
    // =========================================================================

    #[test]
    fn parse_llm_response_list_tasks() {
        let parser = CommandParser::new();
        let response = r#"{"intent":"list_tasks"}"#;
        let cmd = parser.parse_llm_response(response, "").unwrap();
        let AgentCommand::ListTasks {
            status, priority, ..
        } = cmd
        else {
            unreachable!("Expected ListTasks")
        };
        assert!(status.is_none());
        assert!(priority.is_none());
    }

    #[test]
    fn parse_llm_response_list_tasks_with_status() {
        let parser = CommandParser::new();
        let response = r#"{"intent":"list_tasks","status":"in_progress"}"#;
        let cmd = parser.parse_llm_response(response, "").unwrap();
        let AgentCommand::ListTasks { status, .. } = cmd else {
            unreachable!("Expected ListTasks")
        };
        assert!(status.is_some());
        assert!(matches!(status.unwrap(), domain::TaskStatus::InProgress));
    }

    #[test]
    fn parse_llm_response_list_tasks_with_priority() {
        let parser = CommandParser::new();
        let response = r#"{"intent":"list_tasks","priority":"high"}"#;
        let cmd = parser.parse_llm_response(response, "").unwrap();
        let AgentCommand::ListTasks { priority, .. } = cmd else {
            unreachable!("Expected ListTasks")
        };
        assert!(priority.is_some());
        assert!(matches!(priority.unwrap(), domain::Priority::High));
    }

    #[test]
    fn parse_llm_response_list_tasks_medium_priority() {
        let parser = CommandParser::new();
        let response = r#"{"intent":"list_tasks","priority":"medium"}"#;
        let cmd = parser.parse_llm_response(response, "").unwrap();
        let AgentCommand::ListTasks { priority, .. } = cmd else {
            unreachable!("Expected ListTasks")
        };
        assert!(matches!(priority.unwrap(), domain::Priority::Medium));
    }

    #[test]
    fn parse_llm_response_list_tasks_med_priority() {
        let parser = CommandParser::new();
        let response = r#"{"intent":"list_tasks","priority":"med"}"#;
        let cmd = parser.parse_llm_response(response, "").unwrap();
        let AgentCommand::ListTasks { priority, .. } = cmd else {
            unreachable!("Expected ListTasks")
        };
        assert!(matches!(priority.unwrap(), domain::Priority::Medium));
    }

    #[test]
    fn parse_llm_response_list_tasks_invalid_priority() {
        let parser = CommandParser::new();
        let response = r#"{"intent":"list_tasks","priority":"urgent"}"#;
        let result = parser.parse_llm_response(response, "");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Invalid priority"));
    }

    #[test]
    fn parse_llm_response_list_tasks_invalid_status() {
        let parser = CommandParser::new();
        let response = r#"{"intent":"list_tasks","status":"unknown_status"}"#;
        let result = parser.parse_llm_response(response, "");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Invalid task status"));
    }

    #[test]
    fn parse_llm_response_create_task() {
        let parser = CommandParser::new();
        let response = r#"{"intent":"create_task","title":"Buy groceries"}"#;
        let cmd = parser.parse_llm_response(response, "").unwrap();
        let AgentCommand::CreateTask {
            title,
            due_date,
            priority,
            description,
            ..
        } = cmd
        else {
            unreachable!("Expected CreateTask")
        };
        assert_eq!(title, "Buy groceries");
        assert!(due_date.is_none());
        assert!(priority.is_none());
        assert!(description.is_none());
    }

    #[test]
    fn parse_llm_response_create_task_with_all_fields() {
        let parser = CommandParser::new();
        let response = r#"{"intent":"create_task","title":"Call mom","date":"2025-02-15","priority":"high","description":"Discuss birthday plans"}"#;
        let cmd = parser.parse_llm_response(response, "").unwrap();
        let AgentCommand::CreateTask {
            title,
            due_date,
            priority,
            description,
            ..
        } = cmd
        else {
            unreachable!("Expected CreateTask")
        };
        assert_eq!(title, "Call mom");
        assert_eq!(due_date.unwrap().to_string(), "2025-02-15");
        assert!(matches!(priority.unwrap(), domain::Priority::High));
        assert_eq!(description.unwrap(), "Discuss birthday plans");
    }

    #[test]
    fn parse_llm_response_create_task_missing_title() {
        let parser = CommandParser::new();
        let response = r#"{"intent":"create_task","priority":"low"}"#;
        let result = parser.parse_llm_response(response, "");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Missing title"));
    }

    #[test]
    fn parse_llm_response_create_task_invalid_date() {
        let parser = CommandParser::new();
        let response = r#"{"intent":"create_task","title":"Test","date":"invalid-date"}"#;
        let result = parser.parse_llm_response(response, "");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Invalid date format"));
    }

    #[test]
    fn parse_llm_response_create_task_invalid_priority() {
        let parser = CommandParser::new();
        let response = r#"{"intent":"create_task","title":"Test","priority":"super_high"}"#;
        let result = parser.parse_llm_response(response, "");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Invalid priority"));
    }

    #[test]
    fn parse_llm_response_complete_task() {
        let parser = CommandParser::new();
        let response = r#"{"intent":"complete_task","task_id":"task-123"}"#;
        let cmd = parser.parse_llm_response(response, "").unwrap();
        let AgentCommand::CompleteTask { task_id } = cmd else {
            unreachable!("Expected CompleteTask")
        };
        assert_eq!(task_id, "task-123");
    }

    #[test]
    fn parse_llm_response_complete_task_missing_id() {
        let parser = CommandParser::new();
        let response = r#"{"intent":"complete_task"}"#;
        let result = parser.parse_llm_response(response, "");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Missing task_id"));
    }

    #[test]
    fn parse_llm_response_update_task() {
        let parser = CommandParser::new();
        let response = r#"{"intent":"update_task","task_id":"task-456","title":"Updated title"}"#;
        let cmd = parser.parse_llm_response(response, "").unwrap();
        let AgentCommand::UpdateTask { task_id, title, .. } = cmd else {
            unreachable!("Expected UpdateTask")
        };
        assert_eq!(task_id, "task-456");
        assert_eq!(title.unwrap(), "Updated title");
    }

    #[test]
    fn parse_llm_response_update_task_all_fields() {
        let parser = CommandParser::new();
        let response = r#"{"intent":"update_task","task_id":"task-789","title":"New title","date":"2025-03-01","priority":"medium","description":"New description"}"#;
        let cmd = parser.parse_llm_response(response, "").unwrap();
        let AgentCommand::UpdateTask {
            task_id,
            title,
            due_date,
            priority,
            description,
        } = cmd
        else {
            unreachable!("Expected UpdateTask")
        };
        assert_eq!(task_id, "task-789");
        assert_eq!(title.unwrap(), "New title");
        assert!(due_date.is_some());
        assert!(matches!(priority.unwrap(), domain::Priority::Medium));
        assert!(description.is_some());
    }

    #[test]
    fn parse_llm_response_update_task_clear_date() {
        let parser = CommandParser::new();
        let response = r#"{"intent":"update_task","task_id":"task-abc","date":"none"}"#;
        let cmd = parser.parse_llm_response(response, "").unwrap();
        let AgentCommand::UpdateTask { due_date, .. } = cmd else {
            unreachable!("Expected UpdateTask")
        };
        // Some(None) means clear the date
        assert!(due_date.is_some());
        assert!(due_date.unwrap().is_none());
    }

    #[test]
    fn parse_llm_response_update_task_clear_description() {
        let parser = CommandParser::new();
        let response = r#"{"intent":"update_task","task_id":"task-def","description":"null"}"#;
        let cmd = parser.parse_llm_response(response, "").unwrap();
        let AgentCommand::UpdateTask { description, .. } = cmd else {
            unreachable!("Expected UpdateTask")
        };
        // Some(None) means clear the description
        assert!(description.is_some());
        assert!(description.unwrap().is_none());
    }

    #[test]
    fn parse_llm_response_update_task_missing_id() {
        let parser = CommandParser::new();
        let response = r#"{"intent":"update_task","title":"Test"}"#;
        let result = parser.parse_llm_response(response, "");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Missing task_id"));
    }

    #[test]
    fn parse_llm_response_delete_task() {
        let parser = CommandParser::new();
        let response = r#"{"intent":"delete_task","task_id":"task-to-delete"}"#;
        let cmd = parser.parse_llm_response(response, "").unwrap();
        let AgentCommand::DeleteTask { task_id } = cmd else {
            unreachable!("Expected DeleteTask")
        };
        assert_eq!(task_id, "task-to-delete");
    }

    #[test]
    fn parse_llm_response_delete_task_missing_id() {
        let parser = CommandParser::new();
        let response = r#"{"intent":"delete_task"}"#;
        let result = parser.parse_llm_response(response, "");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Missing task_id"));
    }

    // =========================================================================
    // Update Calendar Event Tests
    // =========================================================================

    #[test]
    fn parse_llm_response_update_calendar_event() {
        let parser = CommandParser::new();
        let response = r#"{"intent":"update_calendar_event","event_id":"evt-123","time":"15:00"}"#;
        let cmd = parser.parse_llm_response(response, "").unwrap();
        let AgentCommand::UpdateCalendarEvent { event_id, time, .. } = cmd else {
            unreachable!("Expected UpdateCalendarEvent")
        };
        assert_eq!(event_id, "evt-123");
        assert_eq!(time.unwrap().to_string(), "15:00:00");
    }

    #[test]
    fn parse_llm_response_update_calendar_event_all_fields() {
        let parser = CommandParser::new();
        let response = r#"{"intent":"update_calendar_event","event_id":"evt-456","date":"2025-03-15","time":"10:30","title":"Team Standup","location":"Conference Room A","duration_minutes":30}"#;
        let cmd = parser.parse_llm_response(response, "").unwrap();
        let AgentCommand::UpdateCalendarEvent {
            event_id,
            date,
            time,
            title,
            location,
            duration_minutes,
            ..
        } = cmd
        else {
            unreachable!("Expected UpdateCalendarEvent")
        };
        assert_eq!(event_id, "evt-456");
        assert_eq!(date.unwrap().to_string(), "2025-03-15");
        assert_eq!(time.unwrap().to_string(), "10:30:00");
        assert_eq!(title.unwrap(), "Team Standup");
        assert_eq!(location.unwrap(), "Conference Room A");
        assert_eq!(duration_minutes.unwrap(), 30);
    }

    #[test]
    fn parse_llm_response_update_calendar_event_missing_event_id() {
        let parser = CommandParser::new();
        let response = r#"{"intent":"update_calendar_event","time":"15:00"}"#;
        let result = parser.parse_llm_response(response, "");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Missing event_id"));
    }

    #[test]
    fn parse_llm_response_update_calendar_event_invalid_date() {
        let parser = CommandParser::new();
        let response =
            r#"{"intent":"update_calendar_event","event_id":"evt-123","date":"invalid"}"#;
        let result = parser.parse_llm_response(response, "");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Invalid date format"));
    }

    #[test]
    fn parse_llm_response_update_calendar_event_invalid_time() {
        let parser = CommandParser::new();
        let response = r#"{"intent":"update_calendar_event","event_id":"evt-123","time":"noon"}"#;
        let result = parser.parse_llm_response(response, "");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Invalid time format"));
    }

    #[test]
    fn parses_web_search_german_suche_im_internet() {
        let parser = CommandParser::new();
        let cmd = parser
            .parse_quick("suche im internet nach Rust Programmierung")
            .unwrap();

        let AgentCommand::WebSearch { query, max_results } = cmd else {
            unreachable!("Expected WebSearch command")
        };
        assert_eq!(query, "Rust Programmierung");
        assert!(max_results.is_none());
    }

    #[test]
    fn parses_web_search_german_recherchiere() {
        let parser = CommandParser::new();
        let cmd = parser
            .parse_quick("recherchiere quantum computing")
            .unwrap();

        let AgentCommand::WebSearch { query, max_results } = cmd else {
            unreachable!("Expected WebSearch command")
        };
        assert_eq!(query, "quantum computing");
        assert!(max_results.is_none());
    }

    #[test]
    fn parses_web_search_german_google() {
        let parser = CommandParser::new();
        let cmd = parser
            .parse_quick("google nach aktuelle Nachrichten")
            .unwrap();

        let AgentCommand::WebSearch { query, max_results } = cmd else {
            unreachable!("Expected WebSearch command")
        };
        assert_eq!(query, "aktuelle Nachrichten");
        assert!(max_results.is_none());
    }

    #[test]
    fn parses_web_search_german_finde_heraus() {
        let parser = CommandParser::new();
        let cmd = parser
            .parse_quick("finde heraus was die beste Programmiersprache ist")
            .unwrap();

        let AgentCommand::WebSearch { query, max_results } = cmd else {
            unreachable!("Expected WebSearch command")
        };
        assert_eq!(query, "was die beste Programmiersprache ist");
        assert!(max_results.is_none());
    }

    #[test]
    fn parses_web_search_english_search_the_web() {
        let parser = CommandParser::new();
        let cmd = parser
            .parse_quick("search the web for AI trends 2025")
            .unwrap();

        let AgentCommand::WebSearch { query, max_results } = cmd else {
            unreachable!("Expected WebSearch command")
        };
        assert_eq!(query, "AI trends 2025");
        assert!(max_results.is_none());
    }

    #[test]
    fn parses_web_search_english_look_up() {
        let parser = CommandParser::new();
        let cmd = parser.parse_quick("look up how to bake a cake").unwrap();

        let AgentCommand::WebSearch { query, max_results } = cmd else {
            unreachable!("Expected WebSearch command")
        };
        assert_eq!(query, "how to bake a cake");
        assert!(max_results.is_none());
    }

    #[test]
    fn parses_web_search_german_was_sagt_das_internet() {
        let parser = CommandParser::new();
        let cmd = parser
            .parse_quick("was sagt das internet zu klimawandel")
            .unwrap();

        let AgentCommand::WebSearch { query, max_results } = cmd else {
            unreachable!("Expected WebSearch command")
        };
        assert_eq!(query, "klimawandel");
        assert!(max_results.is_none());
    }

    #[test]
    fn web_search_only_keyword_returns_none() {
        let parser = CommandParser::new();
        // Just the keyword without a query should return None
        let cmd = parser.parse_quick("google");
        assert!(cmd.is_none());
    }

    // =========================================================================
    // Property-Based Tests (proptest)
    // =========================================================================

    mod proptest_tests {
        use super::*;
        use proptest::prelude::*;

        // Strategy for generating valid date strings
        fn valid_date_strategy() -> impl Strategy<Value = String> {
            (2020u32..2030, 1u32..13, 1u32..29)
                .prop_map(|(year, month, day)| format!("{year:04}-{month:02}-{day:02}"))
        }

        // Strategy for generating valid time strings
        fn valid_time_strategy() -> impl Strategy<Value = String> {
            (0u32..24, 0u32..60).prop_map(|(hour, minute)| format!("{hour:02}:{minute:02}"))
        }

        // Strategy for generating valid email addresses
        fn valid_email_strategy() -> impl Strategy<Value = String> {
            ("[a-z]{3,10}", "[a-z]{2,8}", "[a-z]{2,4}")
                .prop_map(|(local, domain, tld)| format!("{local}@{domain}.{tld}"))
        }

        proptest! {
            #![proptest_config(ProptestConfig::with_cases(100))]

            // Test: extract_json should never panic on arbitrary input
            #[test]
            fn extract_json_never_panics(input in ".*") {
                let _ = CommandParser::extract_json(&input);
            }

            // Test: parse_llm_response should handle malformed JSON gracefully
            #[test]
            fn parse_llm_response_handles_garbage(garbage in "[^{}]*") {
                let parser = CommandParser::new();
                let result = parser.parse_llm_response(&garbage, "fallback");
                // Should either succeed (with fallback) or return an error, never panic
                assert!(result.is_ok() || result.is_err());
            }

            // Test: Valid morning_briefing JSON should always parse
            #[test]
            fn valid_morning_briefing_parses(date in valid_date_strategy()) {
                let parser = CommandParser::new();
                let json = format!(r#"{{"intent":"morning_briefing","date":"{date}"}}"#);
                let result = parser.parse_llm_response(&json, "");
                prop_assert!(result.is_ok());
            }

            // Test: Valid web_search JSON should always parse
            #[test]
            fn valid_web_search_parses(query in "[a-zA-Z0-9 ]{1,50}") {
                let parser = CommandParser::new();
                let escaped_query = query.replace('\\', "\\\\").replace('"', "\\\"");
                let json = format!(r#"{{"intent":"web_search","query":"{escaped_query}"}}"#);
                let result = parser.parse_llm_response(&json, "");
                prop_assert!(result.is_ok());
            }

            // Test: Valid calendar event JSON should always parse
            #[test]
            fn valid_calendar_event_parses(
                date in valid_date_strategy(),
                time in valid_time_strategy(),
                title in "[a-zA-Z0-9 ]{1,30}"
            ) {
                let parser = CommandParser::new();
                let escaped_title = title.replace('\\', "\\\\").replace('"', "\\\"");
                let json = format!(
                    r#"{{"intent":"create_calendar_event","date":"{date}","time":"{time}","title":"{escaped_title}"}}"#
                );
                let result = parser.parse_llm_response(&json, "");
                prop_assert!(result.is_ok());
            }

            // Test: Valid draft_email JSON should always parse
            #[test]
            fn valid_draft_email_parses(
                email in valid_email_strategy(),
                body in "[a-zA-Z0-9 ]{1,100}"
            ) {
                let parser = CommandParser::new();
                let escaped_body = body.replace('\\', "\\\\").replace('"', "\\\"");
                let json = format!(
                    r#"{{"intent":"draft_email","to":"{email}","body":"{escaped_body}"}}"#
                );
                let result = parser.parse_llm_response(&json, "");
                prop_assert!(result.is_ok());
            }

            // Test: Invalid date formats should be rejected
            #[test]
            fn invalid_date_format_rejected(
                day in 1u32..32,
                month in 1u32..13,
                year in 2020u32..2030
            ) {
                let parser = CommandParser::new();
                // DD-MM-YYYY format (wrong order)
                let json = format!(
                    r#"{{"intent":"create_calendar_event","date":"{day:02}-{month:02}-{year}","time":"14:00","title":"Test"}}"#
                );
                let result = parser.parse_llm_response(&json, "");
                prop_assert!(result.is_err());
            }

            // Test: Unknown intents should fallback to Ask
            #[test]
            fn unknown_intent_falls_back_to_ask(intent in "[a-z_]{5,20}") {
                // Skip known intents
                prop_assume!(
                    intent != "morning_briefing"
                        && intent != "create_calendar_event"
                        && intent != "summarize_inbox"
                        && intent != "draft_email"
                        && intent != "send_email"
                        && intent != "web_search"
                        && intent != "ask"
                );

                let parser = CommandParser::new();
                let json = format!(r#"{{"intent":"{intent}"}}"#);
                let result = parser.parse_llm_response(&json, "original input");
                prop_assert!(result.is_ok());

                if let Ok(AgentCommand::Ask { question }) = result {
                    prop_assert_eq!(question, "original input");
                } else {
                    prop_assert!(false, "Expected Ask command for unknown intent");
                }
            }

            // Test: parse_quick should never panic on arbitrary input
            #[test]
            fn parse_quick_never_panics(input in ".*") {
                let parser = CommandParser::new();
                let _ = parser.parse_quick(&input);
            }

            // Test: JSON with extra fields should still parse (forward compatibility)
            #[test]
            fn extra_fields_ignored(
                extra_key in "[a-z]{3,10}",
                extra_value in "[a-z0-9]{1,20}"
            ) {
                let parser = CommandParser::new();
                let json = format!(
                    r#"{{"intent":"morning_briefing","{extra_key}":"{extra_value}"}}"#
                );
                let result = parser.parse_llm_response(&json, "");
                prop_assert!(result.is_ok());
            }
        }
    }
}
