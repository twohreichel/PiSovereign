//! Agent commands - Strongly typed representations of user intents

use chrono::{NaiveDate, NaiveTime};
use serde::{Deserialize, Serialize};

use crate::value_objects::EmailAddress;

/// All possible commands the agent can execute
///
/// Each variant represents a distinct user intent with its required parameters.
/// Commands are parsed from natural language input (WhatsApp, chat) or explicit API calls.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AgentCommand {
    /// Request a morning briefing with calendar, tasks, and important emails
    MorningBriefing {
        /// Date for the briefing (defaults to today)
        date: Option<NaiveDate>,
    },

    /// Create a new calendar event
    CreateCalendarEvent {
        /// Event date
        date: NaiveDate,
        /// Event start time
        time: NaiveTime,
        /// Event title/summary
        title: String,
        /// Duration in minutes (defaults to 60)
        duration_minutes: Option<u32>,
        /// Optional attendees
        attendees: Option<Vec<EmailAddress>>,
        /// Optional location
        location: Option<String>,
    },

    /// Get a summary of the inbox
    SummarizeInbox {
        /// Number of recent emails to summarize (defaults to 10)
        count: Option<u32>,
        /// Filter by importance
        only_important: Option<bool>,
    },

    /// Draft an email
    DraftEmail {
        /// Recipient email address
        to: EmailAddress,
        /// Email subject (can be generated if not provided)
        subject: Option<String>,
        /// Email body content or instructions for content
        body: String,
    },

    /// Send a pre-drafted email (requires approval)
    SendEmail {
        /// Draft ID to send
        draft_id: String,
    },

    /// Query the assistant with a free-form question
    Ask {
        /// The question or prompt
        question: String,
    },

    /// System-level commands
    System(SystemCommand),

    /// Echo back a message (for testing)
    Echo {
        /// Message to echo
        message: String,
    },

    /// Show help information
    Help {
        /// Specific command to get help for
        command: Option<String>,
    },

    /// Unknown or unparseable command
    Unknown {
        /// The original input that couldn't be parsed
        original_input: String,
    },
}

/// System-level commands for administration
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum SystemCommand {
    /// Get system status
    Status,
    /// Get version information
    Version,
    /// Reload configuration
    ReloadConfig,
    /// List available models
    ListModels,
    /// Switch to a different model
    SwitchModel { model_name: String },
}

impl AgentCommand {
    /// Check if this command requires user approval before execution
    pub const fn requires_approval(&self) -> bool {
        matches!(
            self,
            Self::SendEmail { .. }
                | Self::CreateCalendarEvent { .. }
                | Self::System(SystemCommand::ReloadConfig | SystemCommand::SwitchModel { .. })
        )
    }

    /// Get a human-readable description of the command
    pub fn description(&self) -> String {
        match self {
            Self::MorningBriefing { date } => {
                let date_str = date
                    .map(|d| d.to_string())
                    .unwrap_or_else(|| "today".to_string());
                format!("Morning briefing for {date_str}")
            },
            Self::CreateCalendarEvent { title, date, .. } => {
                format!("Create event '{title}' on {date}")
            },
            Self::SummarizeInbox { count, .. } => {
                format!("Summarize inbox (last {} emails)", count.unwrap_or(10))
            },
            Self::DraftEmail { to, subject, .. } => {
                let subj = subject.as_deref().unwrap_or("(no subject)");
                format!("Draft email to {to} - {subj}")
            },
            Self::SendEmail { draft_id } => {
                format!("Send email draft {draft_id}")
            },
            Self::Ask { question } => {
                let preview: String = question.chars().take(50).collect();
                format!("Ask: {preview}...")
            },
            Self::System(cmd) => match cmd {
                SystemCommand::Status => "System status".to_string(),
                SystemCommand::Version => "Version info".to_string(),
                SystemCommand::ReloadConfig => "Reload configuration".to_string(),
                SystemCommand::ListModels => "List available models".to_string(),
                SystemCommand::SwitchModel { model_name } => {
                    format!("Switch to model: {model_name}")
                },
            },
            Self::Echo { message } => format!("Echo: {message}"),
            Self::Help { command } => match command {
                Some(cmd) => format!("Help for: {cmd}"),
                None => "General help".to_string(),
            },
            Self::Unknown { original_input } => {
                format!("Unknown command: {original_input}")
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn send_email_requires_approval() {
        let cmd = AgentCommand::SendEmail {
            draft_id: "123".to_string(),
        };
        assert!(cmd.requires_approval());
    }

    #[test]
    fn ask_does_not_require_approval() {
        let cmd = AgentCommand::Ask {
            question: "What's the weather?".to_string(),
        };
        assert!(!cmd.requires_approval());
    }

    #[test]
    fn command_serializes_to_tagged_json() {
        let cmd = AgentCommand::Echo {
            message: "hello".to_string(),
        };
        let json = serde_json::to_string(&cmd).unwrap();
        assert!(json.contains(r#""type":"echo""#));
    }
}
