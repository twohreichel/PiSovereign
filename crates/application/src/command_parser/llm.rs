//! LLM-powered intent detection and JSON parsing.

use std::sync::Arc;

use domain::AgentCommand;
use tracing::{debug, instrument, warn};

use super::{CommandParser, INTENT_SYSTEM_PROMPT, ParsedIntent};
use crate::{error::ApplicationError, ports::InferencePort};

impl CommandParser {
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

    /// Parse the LLM response JSON into an `AgentCommand`
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
        let response =
            r#"{"intent":"create_reminder","title":"Call mom","remind_at":"2025-02-20T18:00:00"}"#;
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
            title, description, ..
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
        let response =
            r#"{"intent":"snooze_reminder","reminder_id":"rem-123","duration_minutes":30}"#;
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
        let response =
            r#"{"intent":"search_transit","from":"Alexanderplatz","to_address":"Potsdamer Platz"}"#;
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
        use proptest::prelude::*;

        use super::*;

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
