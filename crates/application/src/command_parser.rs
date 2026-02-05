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
- "summarize_inbox": Email summary (e.g., "What's new?", "Mails")
- "draft_email": Draft email (requires: to, body; optional: subject)
- "send_email": Send email (requires: draft_id)
- "ask": General question (if nothing else matches)

Reply ONLY with valid JSON:
{
  "intent": "<intent_name>",
  "date": "YYYY-MM-DD" (optional, only for appointments),
  "time": "HH:MM" (optional, only for appointments),
  "title": "..." (optional, for appointments),
  "to": "email@example.com" (optional, for emails),
  "subject": "..." (optional, for emails),
  "body": "..." (optional, for emails),
  "question": "..." (only for ask intent),
  "count": 10 (optional, for inbox),
  "draft_id": "..." (optional, for send_email)
}

Examples:
- "Briefing for tomorrow" → {"intent":"morning_briefing","date":"2025-02-02"}
- "Appointment tomorrow 14:00 Team Meeting" → {"intent":"create_calendar_event","date":"2025-02-02","time":"14:00","title":"Team Meeting"}
- "Summarize my mails" → {"intent":"summarize_inbox"}
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
                keywords: vec!["briefing", "morning", "good morning", "what's on", "what is on"],
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
        if let Some(start) = response.find('{') {
            if let Some(end) = response.rfind('}') {
                return &response[start..=end];
            }
        }

        response
    }

    /// Convert parsed intent to AgentCommand
    #[allow(clippy::unused_self)]
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
        let debug_str = format!("{:?}", parser);
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
        assert!(matches!(cmd, AgentCommand::SummarizeInbox {
            count: None,
            only_important: None
        }));
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
                content: format!(r#"{{"intent":"ask","question":"{}"}}"#, msg),
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
        assert!(matches!(result, AgentCommand::MorningBriefing {
            date: None
        }));
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
    }
}
