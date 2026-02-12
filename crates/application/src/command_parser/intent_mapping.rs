//! Mapping of parsed intents to typed `AgentCommand` values.

use chrono::{NaiveDate, NaiveTime};
use domain::AgentCommand;

use super::{CommandParser, ParsedIntent};

impl CommandParser {
    /// Convert parsed intent to `AgentCommand`
    #[allow(clippy::unused_self, clippy::too_many_lines)]
    pub(super) fn intent_to_command(
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
