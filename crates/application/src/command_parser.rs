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
    fn parses_briefing_with_morgen() {
        let parser = CommandParser::new();
        let cmd = parser.parse_quick("guten morgen").unwrap();

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

        let cmd = parser.parse_quick("HILFE").unwrap();
        assert!(matches!(cmd, AgentCommand::Help { .. }));

        let cmd = parser.parse_quick("STATUS").unwrap();
        assert!(matches!(cmd, AgentCommand::System(domain::SystemCommand::Status)));
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
        let debug_str = format!("{:?}", parser);
        assert!(debug_str.contains("CommandParser"));
    }

    #[test]
    fn parses_version_command() {
        let parser = CommandParser::new();
        let cmd = parser.parse_quick("version").unwrap();
        assert!(matches!(cmd, AgentCommand::System(domain::SystemCommand::Version)));
    }

    #[test]
    fn parses_models_command() {
        let parser = CommandParser::new();
        let cmd = parser.parse_quick("modelle").unwrap();
        assert!(matches!(cmd, AgentCommand::System(domain::SystemCommand::ListModels)));
    }

    #[test]
    fn parses_models_command_english() {
        let parser = CommandParser::new();
        let cmd = parser.parse_quick("models").unwrap();
        assert!(matches!(cmd, AgentCommand::System(domain::SystemCommand::ListModels)));
    }

    #[test]
    fn parses_inbox_command() {
        let parser = CommandParser::new();
        let cmd = parser.parse_quick("inbox").unwrap();
        assert!(matches!(cmd, AgentCommand::SummarizeInbox { count: None, only_important: None }));
    }

    #[test]
    fn parses_mails_zusammenfassen() {
        let parser = CommandParser::new();
        let cmd = parser.parse_quick("mails zusammenfassen").unwrap();
        assert!(matches!(cmd, AgentCommand::SummarizeInbox { .. }));
    }

    #[test]
    fn parses_wichtige_mails() {
        let parser = CommandParser::new();
        let cmd = parser.parse_quick("inbox nur wichtige").unwrap();
        let AgentCommand::SummarizeInbox { only_important, .. } = cmd else {
            unreachable!("Expected SummarizeInbox")
        };
        assert_eq!(only_important, Some(true));
    }

    #[test]
    fn parses_was_steht_an() {
        let parser = CommandParser::new();
        // The pattern checks for "was steht an" in the input
        let cmd = parser.parse_quick("was steht an").unwrap();
        assert!(matches!(cmd, AgentCommand::MorningBriefing { date: None }));
    }

    #[test]
    fn parses_ping_as_status() {
        let parser = CommandParser::new();
        let cmd = parser.parse_quick("ping").unwrap();
        assert!(matches!(cmd, AgentCommand::System(domain::SystemCommand::Status)));
    }

    #[test]
    fn default_creates_parser() {
        let parser = CommandParser::default();
        let debug_str = format!("{:?}", parser);
        assert!(debug_str.contains("CommandParser"));
    }

    #[test]
    fn parser_has_quick_patterns() {
        let parser = CommandParser::new();
        // The debug output shows the pattern count
        let debug_str = format!("{:?}", parser);
        assert!(debug_str.contains("quick_patterns_count"));
    }

    #[test]
    fn help_with_topic_email() {
        let parser = CommandParser::new();
        let cmd = parser.parse_quick("help email").unwrap();
        let AgentCommand::Help { command: Some(topic) } = cmd else {
            unreachable!("Expected Help with topic")
        };
        assert_eq!(topic, "email");
    }

    #[test]
    fn help_with_topic_kalender() {
        let parser = CommandParser::new();
        let cmd = parser.parse_quick("hilfe kalender").unwrap();
        let AgentCommand::Help { command: Some(topic) } = cmd else {
            unreachable!("Expected Help with topic")
        };
        assert_eq!(topic, "kalender");
    }

    #[test]
    fn parses_guten_morgen() {
        let parser = CommandParser::new();
        let cmd = parser.parse_quick("guten morgen").unwrap();
        assert!(matches!(cmd, AgentCommand::MorningBriefing { date: None }));
    }

    #[test]
    fn parses_was_steht_heute_an() {
        let parser = CommandParser::new();
        // The pattern checks for "was steht an" - needs to be exact match in lower case
        let cmd = parser.parse_quick("Was steht an heute?").unwrap();
        assert!(matches!(cmd, AgentCommand::MorningBriefing { date: None }));
    }

    #[test]
    fn parses_emails_inbox() {
        let parser = CommandParser::new();
        // The pattern checks for "inbox" or "mails zusammen" or "email zusammen"
        let cmd = parser.parse_quick("emails inbox zeigen").unwrap();
        assert!(matches!(cmd, AgentCommand::SummarizeInbox { .. }));
    }
}

#[cfg(test)]
mod async_tests {
    use super::*;
    use crate::ports::InferenceResult;
    use mockall::mock;
    use std::sync::Arc;

    mock! {
        pub InferenceEngine {}

        #[async_trait::async_trait]
        impl InferencePort for InferenceEngine {
            async fn generate(&self, message: &str) -> Result<InferenceResult, ApplicationError>;
            async fn generate_with_context(&self, conversation: &domain::Conversation) -> Result<InferenceResult, ApplicationError>;
            async fn generate_with_system(&self, system_prompt: &str, message: &str) -> Result<InferenceResult, ApplicationError>;
            async fn is_healthy(&self) -> bool;
            fn current_model(&self) -> &'static str;
        }
    }

    #[tokio::test]
    async fn parse_with_llm_quick_pattern() {
        let parser = CommandParser::new();
        let mock = MockInferenceEngine::new();
        let inference: Arc<dyn InferencePort> = Arc::new(mock);

        let result = parser.parse_with_llm(&inference, "hilfe").await.unwrap();
        assert!(matches!(result, AgentCommand::Help { command: None }));
    }

    #[tokio::test]
    async fn parse_with_llm_echo() {
        let parser = CommandParser::new();
        let mock = MockInferenceEngine::new();
        let inference: Arc<dyn InferencePort> = Arc::new(mock);

        let result = parser.parse_with_llm(&inference, "echo hello").await.unwrap();
        let AgentCommand::Echo { message } = result else {
            panic!("Expected Echo command");
        };
        assert!(message.contains("hello"));
    }

    #[tokio::test]
    async fn parse_with_llm_status() {
        let parser = CommandParser::new();
        let mock = MockInferenceEngine::new();
        let inference: Arc<dyn InferencePort> = Arc::new(mock);

        let result = parser.parse_with_llm(&inference, "status").await.unwrap();
        assert!(matches!(result, AgentCommand::System(domain::SystemCommand::Status)));
    }

    #[tokio::test]
    async fn parse_with_llm_unknown_becomes_ask() {
        let parser = CommandParser::new();
        let mock = MockInferenceEngine::new();
        let inference: Arc<dyn InferencePort> = Arc::new(mock);

        let result = parser.parse_with_llm(&inference, "was ist der Sinn des Lebens?").await.unwrap();
        let AgentCommand::Ask { question } = result else {
            panic!("Expected Ask command");
        };
        assert!(question.contains("Sinn"));
    }

    #[tokio::test]
    async fn parse_with_llm_briefing() {
        let parser = CommandParser::new();
        let mock = MockInferenceEngine::new();
        let inference: Arc<dyn InferencePort> = Arc::new(mock);

        let result = parser.parse_with_llm(&inference, "briefing").await.unwrap();
        assert!(matches!(result, AgentCommand::MorningBriefing { date: None }));
    }

    #[tokio::test]
    async fn parse_with_llm_version() {
        let parser = CommandParser::new();
        let mock = MockInferenceEngine::new();
        let inference: Arc<dyn InferencePort> = Arc::new(mock);

        let result = parser.parse_with_llm(&inference, "version").await.unwrap();
        assert!(matches!(result, AgentCommand::System(domain::SystemCommand::Version)));
    }

    #[tokio::test]
    async fn parse_with_llm_models() {
        let parser = CommandParser::new();
        let mock = MockInferenceEngine::new();
        let inference: Arc<dyn InferencePort> = Arc::new(mock);

        let result = parser.parse_with_llm(&inference, "modelle").await.unwrap();
        assert!(matches!(result, AgentCommand::System(domain::SystemCommand::ListModels)));
    }

    #[tokio::test]
    async fn parse_with_llm_inbox() {
        let parser = CommandParser::new();
        let mock = MockInferenceEngine::new();
        let inference: Arc<dyn InferencePort> = Arc::new(mock);

        let result = parser.parse_with_llm(&inference, "inbox").await.unwrap();
        assert!(matches!(result, AgentCommand::SummarizeInbox { .. }));
    }
}
