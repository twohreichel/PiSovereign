//! Agent service - Command execution and orchestration

use std::{fmt, sync::Arc, time::Instant};

use domain::{AgentCommand, SystemCommand};
use tracing::{debug, info, instrument, warn};

use crate::{command_parser::CommandParser, error::ApplicationError, ports::InferencePort};

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
    inference: Arc<dyn InferencePort>,
    parser: CommandParser,
    /// Optional calendar service for briefing integration
    calendar_service: Option<Arc<super::CalendarService>>,
    /// Optional email service for briefing integration
    email_service: Option<Arc<super::EmailService>>,
}

impl fmt::Debug for AgentService {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AgentService")
            .field("parser", &self.parser)
            .field("has_calendar", &self.calendar_service.is_some())
            .field("has_email", &self.email_service.is_some())
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

    /// Parse and execute a command from natural language input
    #[instrument(skip(self, input), fields(input_len = input.len()))]
    pub async fn handle_input(&self, input: &str) -> Result<CommandResult, ApplicationError> {
        let start = Instant::now();

        // First, try to parse the command using the LLM
        let command = self.parser.parse_with_llm(&self.inference, input).await?;

        info!(command = ?command, "Parsed command from input");

        // Check if approval is required
        if command.requires_approval() {
            debug!(command = ?command, "Command requires approval");
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

        // Execute the command
        let result = self.execute_command(&command).await?;

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

            AgentCommand::MorningBriefing { date } => self.handle_morning_briefing(*date).await,

            AgentCommand::SummarizeInbox {
                count,
                only_important,
            } => self.handle_summarize_inbox(*count, *only_important).await,

            AgentCommand::Unknown { original_input } => {
                warn!(input = %original_input, "Unknown command received");
                Ok(ExecutionResult {
                    success: false,
                    response: format!(
                        "❓ Ich konnte den Befehl nicht verstehen: '{original_input}'\n\n\
                         Schreibe 'hilfe' für eine Liste der verfügbaren Befehle."
                    ),
                })
            },

            // Commands that require approval - should not reach here without approval
            AgentCommand::CreateCalendarEvent { .. }
            | AgentCommand::DraftEmail { .. }
            | AgentCommand::SendEmail { .. } => {
                Err(ApplicationError::ApprovalRequired(command.description()))
            },
        }
    }

    /// Handle system commands
    async fn handle_system_command(
        &self,
        cmd: &SystemCommand,
    ) -> Result<ExecutionResult, ApplicationError> {
        match cmd {
            SystemCommand::Status => {
                let healthy = self.inference.is_healthy().await;
                let status = if healthy {
                    "🟢 Online"
                } else {
                    "🔴 Offline"
                };
                Ok(ExecutionResult {
                    success: true,
                    response: format!(
                        "📊 System Status:\n\n\
                         Hailo-Inferenz: {}\n\
                         Modell: {}\n\
                         Version: {}",
                        status,
                        self.inference.current_model(),
                        env!("CARGO_PKG_VERSION")
                    ),
                })
            },

            SystemCommand::Version => Ok(ExecutionResult {
                success: true,
                response: format!(
                    "🤖 PiSovereign v{}\n\
                     Rust Edition 2024\n\
                     Hailo-10H AI HAT+ 2",
                    env!("CARGO_PKG_VERSION")
                ),
            }),

            SystemCommand::ListModels => {
                // TODO: Query available models from Hailo
                Ok(ExecutionResult {
                    success: true,
                    response: format!(
                        "📦 Verfügbare Modelle:\n\n\
                         • qwen2.5-1.5b-instruct (aktiv)\n\
                         • llama3.2-1b-instruct\n\
                         • qwen2-1.5b-function-calling\n\n\
                         Aktuell: {}",
                        self.inference.current_model()
                    ),
                })
            },

            SystemCommand::SwitchModel { model_name } => {
                // Switch to the requested model
                match self.inference.switch_model(&model_name).await {
                    Ok(()) => {
                        info!(model = %model_name, "Model switched successfully");
                        Ok(ExecutionResult {
                            success: true,
                            response: format!(
                                "✅ Modell erfolgreich auf '{model_name}' gewechselt."
                            ),
                        })
                    },
                    Err(e) => {
                        warn!(model = %model_name, error = %e, "Model switch failed");
                        Ok(ExecutionResult {
                            success: false,
                            response: format!("❌ Modellwechsel fehlgeschlagen: {e}"),
                        })
                    },
                }
            },

            SystemCommand::ReloadConfig => {
                // Config reload is handled at the HTTP layer via SIGHUP
                // Here we just acknowledge the request
                Ok(ExecutionResult {
                    success: true,
                    response: "🔄 Konfiguration wird neu geladen. Sende SIGHUP an den Server oder nutze die API.".to_string(),
                })
            },
        }
    }

    /// Generate help text
    #[allow(clippy::unused_self)]
    fn generate_help(&self, command: Option<&str>) -> String {
        match command {
            Some("briefing" | "morgen") => "☀️ **Morning Briefing**\n\n\
                 Zeigt eine Übersicht über Termine, E-Mails und Aufgaben.\n\n\
                 Beispiele:\n\
                 • 'briefing'\n\
                 • 'briefing für morgen'\n\
                 • 'was steht heute an?'"
                .to_string(),
            Some("email" | "mail") => "📧 **E-Mail Befehle**\n\n\
                 • 'inbox zusammenfassen' - Zusammenfassung der E-Mails\n\
                 • 'mail an X schreiben' - E-Mail-Entwurf erstellen\n\
                 • 'wichtige mails' - Nur wichtige E-Mails anzeigen"
                .to_string(),
            Some("kalender" | "termin") => "📅 **Kalender Befehle**\n\n\
                 • 'termin am X um Y' - Neuen Termin erstellen\n\
                 • 'termine heute' - Heutige Termine anzeigen\n\
                 • 'nächster termin' - Nächsten Termin anzeigen"
                .to_string(),
            Some("status" | "system") => "🔧 **System Befehle**\n\n\
                 • 'status' - Systemstatus anzeigen\n\
                 • 'version' - Versionsinformation\n\
                 • 'modelle' - Verfügbare KI-Modelle"
                .to_string(),
            _ => "🤖 **PiSovereign Hilfe**\n\n\
                 Verfügbare Befehle:\n\n\
                 • 'hilfe [thema]' - Diese Hilfe\n\
                 • 'briefing' - Tagesübersicht\n\
                 • 'inbox' - E-Mail-Zusammenfassung\n\
                 • 'termin ...' - Kalenderfunktionen\n\
                 • 'status' - Systemstatus\n\
                 • 'echo [text]' - Text zurückgeben\n\n\
                 Du kannst auch einfach Fragen stellen!"
                .to_string(),
        }
    }

    /// Handle morning briefing command
    async fn handle_morning_briefing(
        &self,
        date: Option<chrono::NaiveDate>,
    ) -> Result<ExecutionResult, ApplicationError> {
        use chrono::Local;

        use super::briefing_service::{
            BriefingService, CalendarBrief, EmailBrief, EmailHighlight, TaskBrief,
        };

        let briefing_date = date.unwrap_or_else(|| Local::now().date_naive());
        let date_str = if date.is_none() {
            "heute".to_string()
        } else {
            briefing_date.format("%d.%m.%Y").to_string()
        };

        // Collect calendar data if service available
        let calendar_brief = if let Some(ref calendar_svc) = self.calendar_service {
            match calendar_svc.get_calendar_brief(briefing_date).await {
                Ok(brief) => brief,
                Err(e) => {
                    warn!(error = %e, "Failed to get calendar brief");
                    CalendarBrief::default()
                },
            }
        } else {
            CalendarBrief::default()
        };

        // Collect email data if service available
        let email_brief = if let Some(ref email_svc) = self.email_service {
            match email_svc.get_inbox_summary(5, false).await {
                Ok(summary) => EmailBrief {
                    unread_count: summary.unread_count,
                    important_count: summary.emails.iter().filter(|e| e.is_starred).count() as u32,
                    top_senders: summary
                        .emails
                        .iter()
                        .take(3)
                        .map(|e| e.from.clone())
                        .collect(),
                    highlights: summary
                        .emails
                        .iter()
                        .take(3)
                        .map(|e| EmailHighlight {
                            from: e.from.clone(),
                            subject: e.subject.clone(),
                            preview: e.snippet.clone(),
                        })
                        .collect(),
                },
                Err(e) => {
                    warn!(error = %e, "Failed to get email summary");
                    EmailBrief::default()
                },
            }
        } else {
            EmailBrief::default()
        };

        // Generate briefing using BriefingService
        let briefing_service = BriefingService::new(1); // Central European timezone
        let briefing = briefing_service.generate_briefing(
            calendar_brief,
            email_brief,
            TaskBrief::default(), // TODO: Implement task integration
            None,                 // TODO: Implement weather integration
        );

        // Format briefing response
        let mut response = format!("☀️ Guten Morgen! Hier ist dein Briefing für {date_str}:\n\n");

        // Add calendar section
        response.push_str("📅 **Termine**\n");
        if briefing.calendar.event_count == 0 {
            response.push_str("Heute stehen keine Termine an.\n");
        } else {
            response.push_str(&format!(
                "{} Termin(e) heute:\n",
                briefing.calendar.event_count
            ));
            for event in &briefing.calendar.events {
                if event.all_day {
                    response.push_str(&format!("  • {} (ganztägig)\n", event.title));
                } else {
                    response.push_str(&format!("  • {} um {}\n", event.title, event.start_time));
                }
            }
            if !briefing.calendar.conflicts.is_empty() {
                response.push_str(&format!(
                    "  ⚠️ {} Konflikt(e) erkannt\n",
                    briefing.calendar.conflicts.len()
                ));
            }
        }

        // Add email section
        response.push_str("\n📧 **E-Mails**\n");
        if briefing.email.unread_count == 0 {
            response.push_str("Keine ungelesenen E-Mails.\n");
        } else {
            response.push_str(&format!(
                "{} ungelesene E-Mail(s)",
                briefing.email.unread_count
            ));
            if briefing.email.important_count > 0 {
                response.push_str(&format!(", {} wichtig", briefing.email.important_count));
            }
            response.push_str("\n");
            for highlight in &briefing.email.highlights {
                response.push_str(&format!("  • {}: {}\n", highlight.from, highlight.subject));
            }
        }

        // Add task section if available
        if briefing.tasks.due_today > 0 || briefing.tasks.overdue > 0 {
            response.push_str("\n✅ **Aufgaben**\n");
            if briefing.tasks.due_today > 0 {
                response.push_str(&format!(
                    "{} Aufgabe(n) heute fällig\n",
                    briefing.tasks.due_today
                ));
            }
            if briefing.tasks.overdue > 0 {
                response.push_str(&format!(
                    "⚠️ {} überfällige Aufgabe(n)\n",
                    briefing.tasks.overdue
                ));
            }
        }

        Ok(ExecutionResult {
            success: true,
            response,
        })
    }

    /// Handle inbox summarization command
    async fn handle_summarize_inbox(
        &self,
        count: Option<u32>,
        only_important: Option<bool>,
    ) -> Result<ExecutionResult, ApplicationError> {
        let email_count = count.unwrap_or(10);
        let important_only = only_important.unwrap_or(false);

        // Use email service if available
        if let Some(ref email_svc) = self.email_service {
            match email_svc.summarize_inbox(email_count, important_only).await {
                Ok(summary) => {
                    return Ok(ExecutionResult {
                        success: true,
                        response: summary,
                    });
                },
                Err(e) => {
                    warn!(error = %e, "Failed to summarize inbox, falling back to placeholder");
                },
            }
        }

        // Fallback when service not available
        let filter_msg = if important_only { ", nur wichtige" } else { "" };
        Ok(ExecutionResult {
            success: true,
            response: format!(
                "📧 Inbox-Zusammenfassung (letzte {} E-Mails{}):\n\n\
                 (E-Mail-Integration nicht konfiguriert. Bitte Proton Bridge einrichten.)",
                email_count, filter_msg
            ),
        })
    }
}

/// Result of command execution
#[derive(Debug)]
pub struct ExecutionResult {
    pub success: bool,
    pub response: String,
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

    fn mock_inference_result(content: &str) -> InferenceResult {
        InferenceResult {
            content: content.to_string(),
            model: "test-model".to_string(),
            tokens_used: Some(42),
            latency_ms: 100,
        }
    }

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
    async fn execute_help_general() {
        let mock = MockInferenceEngine::new();
        let service = AgentService::new(Arc::new(mock));

        let result = service
            .execute_command(&AgentCommand::Help { command: None })
            .await
            .unwrap();

        assert!(result.success);
        assert!(result.response.contains("PiSovereign"));
        assert!(result.response.contains("Hilfe"));
    }

    #[tokio::test]
    async fn execute_help_briefing() {
        let mock = MockInferenceEngine::new();
        let service = AgentService::new(Arc::new(mock));

        let result = service
            .execute_command(&AgentCommand::Help {
                command: Some("briefing".to_string()),
            })
            .await
            .unwrap();

        assert!(result.success);
        assert!(result.response.contains("Briefing"));
    }

    #[tokio::test]
    async fn execute_help_email() {
        let mock = MockInferenceEngine::new();
        let service = AgentService::new(Arc::new(mock));

        let result = service
            .execute_command(&AgentCommand::Help {
                command: Some("email".to_string()),
            })
            .await
            .unwrap();

        assert!(result.success);
        assert!(result.response.contains("E-Mail"));
    }

    #[tokio::test]
    async fn execute_help_kalender() {
        let mock = MockInferenceEngine::new();
        let service = AgentService::new(Arc::new(mock));

        let result = service
            .execute_command(&AgentCommand::Help {
                command: Some("kalender".to_string()),
            })
            .await
            .unwrap();

        assert!(result.success);
        assert!(result.response.contains("Kalender"));
    }

    #[tokio::test]
    async fn execute_help_status() {
        let mock = MockInferenceEngine::new();
        let service = AgentService::new(Arc::new(mock));

        let result = service
            .execute_command(&AgentCommand::Help {
                command: Some("status".to_string()),
            })
            .await
            .unwrap();

        assert!(result.success);
        assert!(result.response.contains("System"));
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
    async fn execute_morning_briefing() {
        let mock = MockInferenceEngine::new();
        let service = AgentService::new(Arc::new(mock));

        let result = service
            .execute_command(&AgentCommand::MorningBriefing { date: None })
            .await
            .unwrap();

        assert!(result.success);
        assert!(result.response.contains("Guten Morgen"));
    }

    #[tokio::test]
    async fn execute_morning_briefing_with_date() {
        let mock = MockInferenceEngine::new();
        let service = AgentService::new(Arc::new(mock));

        let date = chrono::NaiveDate::from_ymd_opt(2025, 1, 15).unwrap();
        let result = service
            .execute_command(&AgentCommand::MorningBriefing { date: Some(date) })
            .await
            .unwrap();

        assert!(result.success);
        assert!(result.response.contains("15.01.2025"));
    }

    #[tokio::test]
    async fn execute_summarize_inbox() {
        let mock = MockInferenceEngine::new();
        let service = AgentService::new(Arc::new(mock));

        let result = service
            .execute_command(&AgentCommand::SummarizeInbox {
                count: Some(5),
                only_important: Some(true),
            })
            .await
            .unwrap();

        assert!(result.success);
        assert!(result.response.contains("Inbox"));
        assert!(result.response.contains('5'));
    }

    #[tokio::test]
    async fn execute_summarize_inbox_defaults() {
        let mock = MockInferenceEngine::new();
        let service = AgentService::new(Arc::new(mock));

        let result = service
            .execute_command(&AgentCommand::SummarizeInbox {
                count: None,
                only_important: None,
            })
            .await
            .unwrap();

        assert!(result.success);
        assert!(result.response.contains("10"));
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
    async fn execute_system_status() {
        let mut mock = MockInferenceEngine::new();
        mock.expect_is_healthy().returning(|| true);
        mock.expect_current_model()
            .returning(|| "qwen2.5-1.5b".to_string());

        let service = AgentService::new(Arc::new(mock));

        let result = service
            .execute_command(&AgentCommand::System(SystemCommand::Status))
            .await
            .unwrap();

        assert!(result.success);
        assert!(result.response.contains("Online"));
        assert!(result.response.contains("qwen2.5-1.5b"));
    }

    #[tokio::test]
    async fn execute_system_status_unhealthy() {
        let mut mock = MockInferenceEngine::new();
        mock.expect_is_healthy().returning(|| false);
        mock.expect_current_model()
            .returning(|| "test-model".to_string());

        let service = AgentService::new(Arc::new(mock));

        let result = service
            .execute_command(&AgentCommand::System(SystemCommand::Status))
            .await
            .unwrap();

        assert!(result.success);
        assert!(result.response.contains("Offline"));
    }

    #[tokio::test]
    async fn execute_system_version() {
        let mock = MockInferenceEngine::new();
        let service = AgentService::new(Arc::new(mock));

        let result = service
            .execute_command(&AgentCommand::System(SystemCommand::Version))
            .await
            .unwrap();

        assert!(result.success);
        assert!(result.response.contains("PiSovereign"));
    }

    #[tokio::test]
    async fn execute_system_list_models() {
        let mut mock = MockInferenceEngine::new();
        mock.expect_current_model()
            .returning(|| "qwen2.5-1.5b".to_string());

        let service = AgentService::new(Arc::new(mock));

        let result = service
            .execute_command(&AgentCommand::System(SystemCommand::ListModels))
            .await
            .unwrap();

        assert!(result.success);
        assert!(result.response.contains("Modelle"));
    }

    #[tokio::test]
    async fn execute_system_switch_model() {
        let mut mock = MockInferenceEngine::new();
        mock.expect_switch_model()
            .with(mockall::predicate::eq("llama"))
            .returning(|_| Ok(()));

        let service = AgentService::new(Arc::new(mock));

        let result = service
            .execute_command(&AgentCommand::System(SystemCommand::SwitchModel {
                model_name: "llama".to_string(),
            }))
            .await
            .unwrap();

        assert!(result.success);
        assert!(result.response.contains("llama"));
    }

    #[tokio::test]
    async fn execute_system_switch_model_error() {
        let mut mock = MockInferenceEngine::new();
        mock.expect_switch_model().returning(|_| {
            Err(ApplicationError::Configuration(
                "Model not found".to_string(),
            ))
        });

        let service = AgentService::new(Arc::new(mock));

        let result = service
            .execute_command(&AgentCommand::System(SystemCommand::SwitchModel {
                model_name: "invalid".to_string(),
            }))
            .await
            .unwrap();

        assert!(!result.success);
    }

    #[tokio::test]
    async fn execute_system_reload_config() {
        let mock = MockInferenceEngine::new();
        let service = AgentService::new(Arc::new(mock));

        let result = service
            .execute_command(&AgentCommand::System(SystemCommand::ReloadConfig))
            .await
            .unwrap();

        // Config reload succeeds (placeholder implementation)
        assert!(result.success);
        assert!(result.response.contains("Konfiguration"));
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
    async fn execute_draft_email_requires_approval() {
        let mock = MockInferenceEngine::new();
        let service = AgentService::new(Arc::new(mock));

        let result = service
            .execute_command(&AgentCommand::DraftEmail {
                to: domain::EmailAddress::new("test@example.com").unwrap(),
                subject: Some("Test".to_string()),
                body: "Body content".to_string(),
            })
            .await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn generate_help_for_morgen() {
        let mock = MockInferenceEngine::new();
        let service = AgentService::new(Arc::new(mock));

        let result = service
            .execute_command(&AgentCommand::Help {
                command: Some("morgen".to_string()),
            })
            .await
            .unwrap();

        assert!(result.response.contains("Briefing"));
    }

    #[tokio::test]
    async fn generate_help_for_mail() {
        let mock = MockInferenceEngine::new();
        let service = AgentService::new(Arc::new(mock));

        let result = service
            .execute_command(&AgentCommand::Help {
                command: Some("mail".to_string()),
            })
            .await
            .unwrap();

        assert!(result.response.contains("E-Mail"));
    }

    #[tokio::test]
    async fn generate_help_for_termin() {
        let mock = MockInferenceEngine::new();
        let service = AgentService::new(Arc::new(mock));

        let result = service
            .execute_command(&AgentCommand::Help {
                command: Some("termin".to_string()),
            })
            .await
            .unwrap();

        assert!(result.response.contains("Kalender"));
    }

    #[tokio::test]
    async fn generate_help_for_system() {
        let mock = MockInferenceEngine::new();
        let service = AgentService::new(Arc::new(mock));

        let result = service
            .execute_command(&AgentCommand::Help {
                command: Some("system".to_string()),
            })
            .await
            .unwrap();

        assert!(result.response.contains("System"));
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
