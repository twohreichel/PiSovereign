//! Command parser - Parse natural language into typed commands

use std::{fmt, sync::Arc};

use domain::AgentCommand;
use tracing::{debug, instrument};

use crate::{error::ApplicationError, ports::InferencePort};

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
                keywords: vec!["hilfe", "help", "?"],
                builder: |input| {
                    let lower = input.to_lowercase().trim().to_string();
                    if lower == "hilfe" || lower == "help" || lower == "?" {
                        return Some(AgentCommand::Help { command: None });
                    }
                    for prefix in ["hilfe ", "help "] {
                        if let Some(topic) = lower.strip_prefix(prefix) {
                            return Some(AgentCommand::Help {
                                command: Some(topic.trim().to_string()),
                            });
                        }
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
                keywords: vec!["modelle", "models"],
                builder: |input| {
                    let lower = input.to_lowercase().trim().to_string();
                    if lower == "modelle" || lower == "models" {
                        return Some(AgentCommand::System(domain::SystemCommand::ListModels));
                    }
                    None
                },
            },
            // Morning briefing
            QuickPattern {
                keywords: vec!["briefing", "morgen", "guten morgen", "was steht an"],
                builder: |input| {
                    let lower = input.to_lowercase();
                    if lower.contains("briefing")
                        || lower == "guten morgen"
                        || lower.contains("was steht an")
                        || lower.contains("was steht heute an")
                    {
                        // TODO: Parse date from input
                        return Some(AgentCommand::MorningBriefing { date: None });
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
                        || lower.contains("mails zusammen")
                        || lower.contains("email zusammen")
                    {
                        let only_important = lower.contains("wichtig");
                        return Some(AgentCommand::SummarizeInbox {
                            count: None,
                            only_important: if only_important { Some(true) } else { None },
                        });
                    }
                    None
                },
            },
        ]
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
    #[instrument(skip(self, _inference, input), fields(input_len = input.len()))]
    pub async fn parse_with_llm(
        &self,
        _inference: &Arc<dyn InferencePort>,
        input: &str,
    ) -> Result<AgentCommand, ApplicationError> {
        // First, try quick parsing
        if let Some(cmd) = self.parse_quick(input) {
            return Ok(cmd);
        }

        // For now, treat unrecognized input as a question
        // TODO: Implement proper LLM-based intent detection
        debug!("No quick match, treating as question");

        Ok(AgentCommand::Ask {
            question: input.to_string(),
        })
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
    fn parses_help_command() {
        let parser = CommandParser::new();

        let cmd = parser.parse_quick("hilfe").unwrap();
        assert!(matches!(cmd, AgentCommand::Help { command: None }));

        let cmd = parser.parse_quick("hilfe email").unwrap();
        let AgentCommand::Help {
            command: Some(topic),
        } = cmd
        else {
            unreachable!("Expected Help command with topic")
        };
        assert_eq!(topic, "email");
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
    fn unknown_input_returns_none() {
        let parser = CommandParser::new();
        let cmd = parser.parse_quick("some random text");

        assert!(cmd.is_none());
    }
}
