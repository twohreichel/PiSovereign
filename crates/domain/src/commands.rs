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
    use crate::value_objects::EmailAddress;

    // === requires_approval Tests ===

    #[test]
    fn send_email_requires_approval() {
        let cmd = AgentCommand::SendEmail {
            draft_id: "123".to_string(),
        };
        assert!(cmd.requires_approval());
    }

    #[test]
    fn create_calendar_event_requires_approval() {
        let cmd = AgentCommand::CreateCalendarEvent {
            title: "Meeting".to_string(),
            date: NaiveDate::from_ymd_opt(2026, 2, 3).unwrap(),
            time: NaiveTime::from_hms_opt(10, 0, 0).unwrap(),
            duration_minutes: Some(60),
            attendees: None,
            location: None,
        };
        assert!(cmd.requires_approval());
    }

    #[test]
    fn system_reload_config_requires_approval() {
        let cmd = AgentCommand::System(SystemCommand::ReloadConfig);
        assert!(cmd.requires_approval());
    }

    #[test]
    fn system_switch_model_requires_approval() {
        let cmd = AgentCommand::System(SystemCommand::SwitchModel {
            model_name: "llama3".to_string(),
        });
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
    fn echo_does_not_require_approval() {
        let cmd = AgentCommand::Echo {
            message: "hello".to_string(),
        };
        assert!(!cmd.requires_approval());
    }

    #[test]
    fn help_does_not_require_approval() {
        let cmd = AgentCommand::Help { command: None };
        assert!(!cmd.requires_approval());
    }

    #[test]
    fn system_status_does_not_require_approval() {
        let cmd = AgentCommand::System(SystemCommand::Status);
        assert!(!cmd.requires_approval());
    }

    #[test]
    fn system_version_does_not_require_approval() {
        let cmd = AgentCommand::System(SystemCommand::Version);
        assert!(!cmd.requires_approval());
    }

    #[test]
    fn system_list_models_does_not_require_approval() {
        let cmd = AgentCommand::System(SystemCommand::ListModels);
        assert!(!cmd.requires_approval());
    }

    #[test]
    fn summarize_inbox_does_not_require_approval() {
        let cmd = AgentCommand::SummarizeInbox {
            count: Some(10),
            only_important: None,
        };
        assert!(!cmd.requires_approval());
    }

    #[test]
    fn draft_email_does_not_require_approval() {
        let cmd = AgentCommand::DraftEmail {
            to: EmailAddress::new("test@example.com").unwrap(),
            subject: Some("Test".to_string()),
            body: "Body".to_string(),
        };
        assert!(!cmd.requires_approval());
    }

    #[test]
    fn morning_briefing_does_not_require_approval() {
        let cmd = AgentCommand::MorningBriefing { date: None };
        assert!(!cmd.requires_approval());
    }

    #[test]
    fn unknown_does_not_require_approval() {
        let cmd = AgentCommand::Unknown {
            original_input: "xyz".to_string(),
        };
        assert!(!cmd.requires_approval());
    }

    // === description Tests ===

    #[test]
    fn morning_briefing_description_with_date() {
        let cmd = AgentCommand::MorningBriefing {
            date: Some(NaiveDate::from_ymd_opt(2026, 2, 3).unwrap()),
        };
        assert_eq!(cmd.description(), "Morning briefing for 2026-02-03");
    }

    #[test]
    fn morning_briefing_description_without_date() {
        let cmd = AgentCommand::MorningBriefing { date: None };
        assert_eq!(cmd.description(), "Morning briefing for today");
    }

    #[test]
    fn create_calendar_event_description() {
        let cmd = AgentCommand::CreateCalendarEvent {
            title: "Team Standup".to_string(),
            date: NaiveDate::from_ymd_opt(2026, 2, 3).unwrap(),
            time: NaiveTime::from_hms_opt(9, 0, 0).unwrap(),
            duration_minutes: None,
            attendees: None,
            location: None,
        };
        assert!(cmd.description().contains("Team Standup"));
        assert!(cmd.description().contains("2026-02-03"));
    }

    #[test]
    fn summarize_inbox_description_with_count() {
        let cmd = AgentCommand::SummarizeInbox {
            count: Some(20),
            only_important: None,
        };
        assert_eq!(cmd.description(), "Summarize inbox (last 20 emails)");
    }

    #[test]
    fn summarize_inbox_description_without_count() {
        let cmd = AgentCommand::SummarizeInbox {
            count: None,
            only_important: None,
        };
        assert_eq!(cmd.description(), "Summarize inbox (last 10 emails)");
    }

    #[test]
    fn draft_email_description_with_subject() {
        let cmd = AgentCommand::DraftEmail {
            to: EmailAddress::new("test@example.com").unwrap(),
            subject: Some("Hello".to_string()),
            body: "Test body".to_string(),
        };
        let desc = cmd.description();
        assert!(desc.contains("test@example.com"));
        assert!(desc.contains("Hello"));
    }

    #[test]
    fn draft_email_description_without_subject() {
        let cmd = AgentCommand::DraftEmail {
            to: EmailAddress::new("test@example.com").unwrap(),
            subject: None,
            body: "Test body".to_string(),
        };
        let desc = cmd.description();
        assert!(desc.contains("test@example.com"));
        assert!(desc.contains("(no subject)"));
    }

    #[test]
    fn send_email_description() {
        let cmd = AgentCommand::SendEmail {
            draft_id: "draft-456".to_string(),
        };
        assert_eq!(cmd.description(), "Send email draft draft-456");
    }

    #[test]
    fn ask_description_truncates_long_questions() {
        let cmd = AgentCommand::Ask {
            question:
                "This is a very long question that exceeds fifty characters and should be truncated"
                    .to_string(),
        };
        let desc = cmd.description();
        assert!(desc.starts_with("Ask: "));
        assert!(desc.ends_with("..."));
        assert!(desc.len() < 70);
    }

    #[test]
    fn system_status_description() {
        let cmd = AgentCommand::System(SystemCommand::Status);
        assert_eq!(cmd.description(), "System status");
    }

    #[test]
    fn system_version_description() {
        let cmd = AgentCommand::System(SystemCommand::Version);
        assert_eq!(cmd.description(), "Version info");
    }

    #[test]
    fn system_reload_config_description() {
        let cmd = AgentCommand::System(SystemCommand::ReloadConfig);
        assert_eq!(cmd.description(), "Reload configuration");
    }

    #[test]
    fn system_list_models_description() {
        let cmd = AgentCommand::System(SystemCommand::ListModels);
        assert_eq!(cmd.description(), "List available models");
    }

    #[test]
    fn system_switch_model_description() {
        let cmd = AgentCommand::System(SystemCommand::SwitchModel {
            model_name: "qwen2.5".to_string(),
        });
        assert_eq!(cmd.description(), "Switch to model: qwen2.5");
    }

    #[test]
    fn echo_description() {
        let cmd = AgentCommand::Echo {
            message: "test message".to_string(),
        };
        assert_eq!(cmd.description(), "Echo: test message");
    }

    #[test]
    fn help_description_with_command() {
        let cmd = AgentCommand::Help {
            command: Some("email".to_string()),
        };
        assert_eq!(cmd.description(), "Help for: email");
    }

    #[test]
    fn help_description_without_command() {
        let cmd = AgentCommand::Help { command: None };
        assert_eq!(cmd.description(), "General help");
    }

    #[test]
    fn unknown_description() {
        let cmd = AgentCommand::Unknown {
            original_input: "blah blah".to_string(),
        };
        assert_eq!(cmd.description(), "Unknown command: blah blah");
    }

    // === Serialization Tests ===

    #[test]
    fn command_serializes_to_tagged_json() {
        let cmd = AgentCommand::Echo {
            message: "hello".to_string(),
        };
        let json = serde_json::to_string(&cmd).unwrap();
        assert!(json.contains(r#""type":"echo""#));
    }

    #[test]
    fn command_deserializes_from_tagged_json() {
        let json = r#"{"type":"echo","message":"hello"}"#;
        let cmd: AgentCommand = serde_json::from_str(json).unwrap();
        assert!(matches!(cmd, AgentCommand::Echo { message } if message == "hello"));
    }

    #[test]
    fn system_command_serializes_correctly() {
        let cmd = AgentCommand::System(SystemCommand::Status);
        let json = serde_json::to_string(&cmd).unwrap();
        assert!(json.contains("status"));
    }

    #[test]
    fn ask_command_serializes_correctly() {
        let cmd = AgentCommand::Ask {
            question: "Hello?".to_string(),
        };
        let json = serde_json::to_string(&cmd).unwrap();
        assert!(json.contains("ask"));
        assert!(json.contains("Hello?"));
    }

    #[test]
    fn help_command_serializes_correctly() {
        let cmd = AgentCommand::Help {
            command: Some("test".to_string()),
        };
        let json = serde_json::to_string(&cmd).unwrap();
        assert!(json.contains("help"));
    }
}
