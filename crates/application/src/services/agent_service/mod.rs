//! Agent service - Command execution and orchestration
//!
//! This module is split into focused sub-modules:
//! - [`system`]: System commands (status, version, models, config reload)
//! - [`briefing`]: Morning briefing with calendar, email, task, weather integration
//! - [`email`]: Inbox summarization and email draft creation
//! - [`reminders`]: Reminder CRUD and snooze operations
//! - [`tasks`]: Task and task list queries
//! - [`web_search`]: Web search with LLM summarization
//! - [`transit`]: Public transit connection search

mod briefing;
mod contacts;
mod email;
mod reminders;
mod system;
mod tasks;
mod transit;
mod web_search;

use std::{fmt, sync::Arc, time::Instant};

use domain::{AgentCommand, GeoLocation, UserId};
use tracing::{debug, info, instrument, warn};

use crate::{
    command_parser::CommandParser,
    error::ApplicationError,
    ports::{
        ContactPort, DraftStorePort, InferencePort, ReminderPort, TaskPort, TransitPort,
        UserProfileStore, WeatherPort, WebSearchPort,
    },
};

/// Result of executing an agent command
#[derive(Debug, Clone)]
pub struct CommandResult {
    /// The command that was executed
    pub command: AgentCommand,
    /// Whether the command succeeded
    pub success: bool,
    /// Response message to send back to the user
    pub response: String,
    /// Execution time in milliseconds
    pub execution_time_ms: u64,
    /// Whether approval was required and granted
    pub approval_status: Option<ApprovalStatus>,
}

/// Status of approval for commands that require it
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ApprovalStatus {
    /// No approval needed for this command
    NotRequired,
    /// Approval is pending
    Pending,
    /// Approval was granted
    Granted,
    /// Approval was denied
    Denied,
}

/// Service for handling agent commands
pub struct AgentService {
    pub(super) inference: Arc<dyn InferencePort>,
    pub(super) parser: CommandParser,
    /// Optional calendar service for briefing integration
    pub(super) calendar_service: Option<Arc<super::CalendarService>>,
    /// Optional email service for briefing integration
    pub(super) email_service: Option<Arc<super::EmailService>>,
    /// Optional draft store for email draft persistence
    pub(super) draft_store: Option<Arc<dyn DraftStorePort>>,
    /// Optional user profile store for personalization
    pub(super) user_profile_store: Option<Arc<dyn UserProfileStore>>,
    /// Optional task service for todo/task integration
    pub(super) task_service: Option<Arc<dyn TaskPort>>,
    /// Optional weather service for weather integration
    pub(super) weather_service: Option<Arc<dyn WeatherPort>>,
    /// Optional web search service for internet search
    pub(super) websearch_service: Option<Arc<dyn WebSearchPort>>,
    /// Optional reminder service for reminder management
    pub(super) reminder_service: Option<Arc<dyn ReminderPort>>,
    /// Optional transit service for ÖPNV connections
    pub(super) transit_service: Option<Arc<dyn TransitPort>>,
    /// Optional contact service for contact management (CardDAV)
    pub(super) contact_service: Option<Arc<dyn ContactPort>>,
    /// Default location for weather when user profile has no location
    pub(super) default_weather_location: Option<GeoLocation>,
    /// Home location for transit searches (used when "from" is not specified)
    pub(super) home_location: Option<GeoLocation>,
}

impl fmt::Debug for AgentService {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AgentService")
            .field("parser", &self.parser)
            .field("has_calendar", &self.calendar_service.is_some())
            .field("has_email", &self.email_service.is_some())
            .field("has_draft_store", &self.draft_store.is_some())
            .field("has_user_profile", &self.user_profile_store.is_some())
            .field("has_task", &self.task_service.is_some())
            .field("has_weather", &self.weather_service.is_some())
            .field("has_websearch", &self.websearch_service.is_some())
            .field("has_reminder", &self.reminder_service.is_some())
            .field("has_transit", &self.transit_service.is_some())
            .field("has_contacts", &self.contact_service.is_some())
            .finish_non_exhaustive()
    }
}

impl AgentService {
    /// Create a new agent service
    pub fn new(inference: Arc<dyn InferencePort>) -> Self {
        Self {
            inference,
            parser: CommandParser::new(),
            calendar_service: None,
            email_service: None,
            draft_store: None,
            user_profile_store: None,
            task_service: None,
            weather_service: None,
            websearch_service: None,
            reminder_service: None,
            transit_service: None,
            contact_service: None,
            default_weather_location: None,
            home_location: None,
        }
    }

    /// Add calendar service for briefing integration
    #[must_use]
    pub fn with_calendar_service(mut self, service: Arc<super::CalendarService>) -> Self {
        self.calendar_service = Some(service);
        self
    }

    /// Add email service for briefing integration
    #[must_use]
    pub fn with_email_service(mut self, service: Arc<super::EmailService>) -> Self {
        self.email_service = Some(service);
        self
    }

    /// Add draft store for email draft persistence
    #[must_use]
    pub fn with_draft_store(mut self, store: Arc<dyn DraftStorePort>) -> Self {
        self.draft_store = Some(store);
        self
    }

    /// Add user profile store for personalization
    #[must_use]
    pub fn with_user_profile_store(mut self, store: Arc<dyn UserProfileStore>) -> Self {
        self.user_profile_store = Some(store);
        self
    }

    /// Add task service for todo/task integration
    #[must_use]
    pub fn with_task_service(mut self, service: Arc<dyn TaskPort>) -> Self {
        self.task_service = Some(service);
        self
    }

    /// Add weather service for weather integration
    #[must_use]
    pub fn with_weather_service(mut self, service: Arc<dyn WeatherPort>) -> Self {
        self.weather_service = Some(service);
        self
    }

    /// Add web search service for internet search capabilities
    #[must_use]
    pub fn with_websearch_service(mut self, service: Arc<dyn WebSearchPort>) -> Self {
        self.websearch_service = Some(service);
        self
    }

    /// Add reminder service for reminder management
    #[must_use]
    pub fn with_reminder_service(mut self, service: Arc<dyn ReminderPort>) -> Self {
        self.reminder_service = Some(service);
        self
    }

    /// Add transit service for ÖPNV connection searches
    #[must_use]
    pub fn with_transit_service(mut self, service: Arc<dyn TransitPort>) -> Self {
        self.transit_service = Some(service);
        self
    }

    /// Add contact service for contact management (CardDAV)
    #[must_use]
    pub fn with_contact_service(mut self, service: Arc<dyn ContactPort>) -> Self {
        self.contact_service = Some(service);
        self
    }

    /// Set default weather location (fallback when user profile has no location)
    #[must_use]
    pub const fn with_default_weather_location(mut self, location: GeoLocation) -> Self {
        self.default_weather_location = Some(location);
        self
    }

    /// Set home location for transit searches (used when "from" is not specified)
    #[must_use]
    pub const fn with_home_location(mut self, location: GeoLocation) -> Self {
        self.home_location = Some(location);
        self
    }

    /// Parse and execute a command from natural language input
    #[instrument(skip(self, input), fields(input_len = input.len()))]
    pub async fn handle_input(&self, input: &str) -> Result<CommandResult, ApplicationError> {
        self.handle_input_with_user(input, None).await
    }

    /// Parse and execute a command with explicit user context
    ///
    /// This method should be used when the caller has access to the authenticated
    /// user's identity (e.g., from `RequestContext` in HTTP handlers).
    #[instrument(skip(self, input, user_id), fields(input_len = input.len()))]
    pub async fn handle_input_with_user(
        &self,
        input: &str,
        user_id: Option<UserId>,
    ) -> Result<CommandResult, ApplicationError> {
        let start = Instant::now();

        // First, try to parse the command using the LLM
        let command = self.parser.parse_with_llm(&self.inference, input).await?;

        info!(command = ?command, "Parsed command from input");

        // Check if approval is required
        if command.requires_approval() {
            debug!(command = ?command, "Command requires approval");
            #[allow(clippy::cast_possible_truncation)]
            return Ok(CommandResult {
                command: command.clone(),
                success: false,
                response: format!(
                    "⚠️ Diese Aktion erfordert Bestätigung: {}\n\nBitte bestätige mit 'OK' oder breche ab mit 'Abbrechen'.",
                    command.description()
                ),
                execution_time_ms: start.elapsed().as_millis() as u64,
                approval_status: Some(ApprovalStatus::Pending),
            });
        }

        // Execute the command with user context
        let result = self.execute_command_with_user(&command, user_id).await?;

        #[allow(clippy::cast_possible_truncation)]
        Ok(CommandResult {
            command,
            success: result.success,
            response: result.response,
            execution_time_ms: start.elapsed().as_millis() as u64,
            approval_status: Some(ApprovalStatus::NotRequired),
        })
    }

    /// Execute a specific command (after parsing/approval)
    #[instrument(skip(self, command))]
    pub async fn execute_command(
        &self,
        command: &AgentCommand,
    ) -> Result<ExecutionResult, ApplicationError> {
        self.execute_command_with_user(command, None).await
    }

    /// Execute a specific command with explicit user context
    ///
    /// This method should be used when the caller has access to the authenticated
    /// user's identity. Commands that need user context (like fetching tasks)
    /// will use the provided user ID instead of the default.
    #[instrument(skip(self, command, user_id))]
    pub async fn execute_command_with_user(
        &self,
        command: &AgentCommand,
        user_id: Option<UserId>,
    ) -> Result<ExecutionResult, ApplicationError> {
        match command {
            AgentCommand::Echo { message } => Ok(ExecutionResult {
                success: true,
                response: format!("🔊 {message}"),
            }),

            AgentCommand::Help { command: cmd } => {
                let help_text = self.generate_help(cmd.as_deref());
                Ok(ExecutionResult {
                    success: true,
                    response: help_text,
                })
            },

            AgentCommand::System(sys_cmd) => self.handle_system_command(sys_cmd).await,

            AgentCommand::Ask { question } => {
                let response = self.inference.generate(question).await?;
                Ok(ExecutionResult {
                    success: true,
                    response: response.content,
                })
            },

            AgentCommand::MorningBriefing { date } => {
                self.handle_morning_briefing(*date, user_id).await
            },

            AgentCommand::SummarizeInbox {
                count,
                only_important,
            } => self.handle_summarize_inbox(*count, *only_important).await,

            AgentCommand::Unknown { original_input } => {
                warn!(input = %original_input, "Unknown command received");
                Ok(ExecutionResult {
                    success: false,
                    response: format!(
                        "❓ I could not understand the command: '{original_input}'\n\n\
                         Type 'help' for a list of available commands."
                    ),
                })
            },

            // Draft email - create and store draft
            AgentCommand::DraftEmail { to, subject, body } => {
                self.handle_draft_email(to, subject.as_deref(), body).await
            },

            // List tasks - read-only, doesn't require approval
            AgentCommand::ListTasks {
                status,
                priority,
                list,
            } => {
                self.handle_list_tasks(status.as_ref(), priority.as_ref(), list.as_deref())
                    .await
            },

            // List task lists - read-only, doesn't require approval
            AgentCommand::ListTaskLists => self.handle_list_task_lists().await,

            // Commands that require approval - should not reach here without approval
            AgentCommand::CreateCalendarEvent { .. }
            | AgentCommand::UpdateCalendarEvent { .. }
            | AgentCommand::CreateTask { .. }
            | AgentCommand::CompleteTask { .. }
            | AgentCommand::UpdateTask { .. }
            | AgentCommand::DeleteTask { .. }
            | AgentCommand::CreateTaskList { .. }
            | AgentCommand::SendEmail { .. }
            | AgentCommand::CreateContact { .. }
            | AgentCommand::UpdateContact { .. }
            | AgentCommand::DeleteContact { .. } => {
                Err(ApplicationError::ApprovalRequired(command.description()))
            },

            // Web search - requires websearch service to be configured
            AgentCommand::WebSearch { query, max_results } => {
                self.handle_web_search(query, *max_results).await
            },

            // Reminder commands
            AgentCommand::CreateReminder {
                title,
                remind_at,
                description,
            } => {
                self.handle_create_reminder(title, remind_at, description.as_deref())
                    .await
            },
            AgentCommand::ListReminders { include_done } => {
                self.handle_list_reminders(*include_done).await
            },
            AgentCommand::SnoozeReminder {
                reminder_id,
                duration_minutes,
            } => {
                self.handle_snooze_reminder(reminder_id, *duration_minutes)
                    .await
            },
            AgentCommand::AcknowledgeReminder { reminder_id } => {
                self.handle_acknowledge_reminder(reminder_id).await
            },
            AgentCommand::DeleteReminder { reminder_id } => {
                self.handle_delete_reminder(reminder_id).await
            },

            // Transit search
            AgentCommand::SearchTransit {
                from,
                to,
                departure,
            } => {
                self.handle_search_transit(from, to, departure.as_deref())
                    .await
            },

            // Contact management - read-only operations
            AgentCommand::ListContacts { query } => {
                self.handle_list_contacts(query.as_deref()).await
            },
            AgentCommand::GetContact { contact_id } => self.handle_get_contact(contact_id).await,
            AgentCommand::SearchContacts { query } => self.handle_search_contacts(query).await,
        }
    }
}

/// Result of command execution
#[derive(Debug)]
pub struct ExecutionResult {
    pub success: bool,
    pub response: String,
}

// ---------------------------------------------------------------------------
// Test support: shared mock types for handler sub-module tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod test_support {
    use mockall::mock;

    use crate::{error::ApplicationError, ports::InferenceResult};

    mock! {
        pub InferenceEngine {}

        #[async_trait::async_trait]
        impl crate::ports::InferencePort for InferenceEngine {
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

    pub fn mock_inference_result(content: &str) -> InferenceResult {
        InferenceResult {
            content: content.to_string(),
            model: "test-model".to_string(),
            tokens_used: Some(42),
            latency_ms: 100,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_result_creation() {
        let result = CommandResult {
            command: AgentCommand::Echo {
                message: "test".to_string(),
            },
            success: true,
            response: "OK".to_string(),
            execution_time_ms: 100,
            approval_status: None,
        };
        assert!(result.success);
        assert_eq!(result.execution_time_ms, 100);
    }

    #[test]
    fn command_result_with_approval_status() {
        let result = CommandResult {
            command: AgentCommand::Help { command: None },
            success: true,
            response: "Help text".to_string(),
            execution_time_ms: 50,
            approval_status: Some(ApprovalStatus::NotRequired),
        };
        assert_eq!(result.approval_status, Some(ApprovalStatus::NotRequired));
    }

    #[test]
    fn approval_status_pending() {
        let status = ApprovalStatus::Pending;
        assert_eq!(status, ApprovalStatus::Pending);
        assert_ne!(status, ApprovalStatus::Granted);
    }

    #[test]
    fn approval_status_granted() {
        let status = ApprovalStatus::Granted;
        assert_eq!(status, ApprovalStatus::Granted);
    }

    #[test]
    fn approval_status_denied() {
        let status = ApprovalStatus::Denied;
        assert_eq!(status, ApprovalStatus::Denied);
    }

    #[test]
    fn approval_status_not_required() {
        let status = ApprovalStatus::NotRequired;
        assert_eq!(status, ApprovalStatus::NotRequired);
    }

    #[test]
    fn approval_status_clone() {
        let status = ApprovalStatus::Pending;
        #[allow(clippy::redundant_clone)]
        let cloned = status.clone();
        assert_eq!(status, cloned);
    }

    #[test]
    fn command_result_clone() {
        let result = CommandResult {
            command: AgentCommand::Echo {
                message: "test".to_string(),
            },
            success: true,
            response: "OK".to_string(),
            execution_time_ms: 100,
            approval_status: Some(ApprovalStatus::NotRequired),
        };
        #[allow(clippy::redundant_clone)]
        let cloned = result.clone();
        assert_eq!(result.success, cloned.success);
        assert_eq!(result.response, cloned.response);
    }

    #[test]
    fn command_result_has_debug() {
        let result = CommandResult {
            command: AgentCommand::Echo {
                message: "test".to_string(),
            },
            success: true,
            response: "OK".to_string(),
            execution_time_ms: 100,
            approval_status: None,
        };
        let debug = format!("{result:?}");
        assert!(debug.contains("CommandResult"));
        assert!(debug.contains("success"));
    }

    #[test]
    fn approval_status_has_debug() {
        let status = ApprovalStatus::Pending;
        let debug = format!("{status:?}");
        assert!(debug.contains("Pending"));
    }

    #[test]
    fn execution_result_creation() {
        let result = ExecutionResult {
            success: true,
            response: "Done".to_string(),
        };
        assert!(result.success);
        assert_eq!(result.response, "Done");
    }

    #[test]
    fn execution_result_failure() {
        let result = ExecutionResult {
            success: false,
            response: "Failed".to_string(),
        };
        assert!(!result.success);
    }

    #[test]
    fn execution_result_has_debug() {
        let result = ExecutionResult {
            success: true,
            response: "OK".to_string(),
        };
        let debug = format!("{result:?}");
        assert!(debug.contains("ExecutionResult"));
    }
}

#[cfg(test)]
mod async_tests {
    use std::sync::Arc;

    use domain::AgentCommand;

    use super::{
        AgentService,
        test_support::{MockInferenceEngine, mock_inference_result},
    };
    use crate::error::ApplicationError;

    #[tokio::test]
    async fn agent_service_new() {
        let mock = MockInferenceEngine::new();
        let service = AgentService::new(Arc::new(mock));
        let debug = format!("{service:?}");
        assert!(debug.contains("AgentService"));
    }

    #[tokio::test]
    async fn execute_echo_command() {
        let mock = MockInferenceEngine::new();
        let service = AgentService::new(Arc::new(mock));

        let result = service
            .execute_command(&AgentCommand::Echo {
                message: "Hello".to_string(),
            })
            .await
            .unwrap();

        assert!(result.success);
        assert!(result.response.contains("Hello"));
    }

    #[tokio::test]
    async fn execute_ask_command() {
        let mut mock = MockInferenceEngine::new();
        mock.expect_generate()
            .returning(|_| Ok(mock_inference_result("AI Response")));

        let service = AgentService::new(Arc::new(mock));

        let result = service
            .execute_command(&AgentCommand::Ask {
                question: "What is the weather?".to_string(),
            })
            .await
            .unwrap();

        assert!(result.success);
        assert_eq!(result.response, "AI Response");
    }

    #[tokio::test]
    async fn execute_unknown_command() {
        let mock = MockInferenceEngine::new();
        let service = AgentService::new(Arc::new(mock));

        let result = service
            .execute_command(&AgentCommand::Unknown {
                original_input: "gibberish".to_string(),
            })
            .await
            .unwrap();

        assert!(!result.success);
        assert!(result.response.contains("gibberish"));
    }

    #[tokio::test]
    async fn execute_calendar_event_requires_approval() {
        let mock = MockInferenceEngine::new();
        let service = AgentService::new(Arc::new(mock));

        let result = service
            .execute_command(&AgentCommand::CreateCalendarEvent {
                title: "Meeting".to_string(),
                date: chrono::NaiveDate::from_ymd_opt(2025, 1, 15).unwrap(),
                time: chrono::NaiveTime::from_hms_opt(10, 0, 0).unwrap(),
                duration_minutes: None,
                attendees: None,
                location: None,
            })
            .await;

        assert!(result.is_err());
        if let Err(ApplicationError::ApprovalRequired(desc)) = result {
            assert!(desc.contains("Meeting"));
        }
    }

    #[tokio::test]
    async fn execute_send_email_requires_approval() {
        let mock = MockInferenceEngine::new();
        let service = AgentService::new(Arc::new(mock));

        let result = service
            .execute_command(&AgentCommand::SendEmail {
                draft_id: "draft-123".to_string(),
            })
            .await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn agent_service_debug_output() {
        let mock = MockInferenceEngine::new();
        let service = AgentService::new(Arc::new(mock));
        let debug = format!("{service:?}");
        assert!(debug.contains("AgentService"));
        assert!(debug.contains("parser"));
    }
}
