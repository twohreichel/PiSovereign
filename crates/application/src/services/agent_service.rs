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
}

impl fmt::Debug for AgentService {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AgentService")
            .field("parser", &self.parser)
            .finish_non_exhaustive()
    }
}

impl AgentService {
    /// Create a new agent service
    pub fn new(inference: Arc<dyn InferencePort>) -> Self {
        Self {
            inference,
            parser: CommandParser::new(),
        }
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

            AgentCommand::MorningBriefing { date } => {
                let date_str = date
                    .map(|d| d.to_string())
                    .unwrap_or_else(|| "heute".to_string());

                // TODO: Implement actual briefing with calendar/email integration
                Ok(ExecutionResult {
                    success: true,
                    response: format!(
                        "☀️ Guten Morgen! Hier ist dein Briefing für {date_str}:\n\n\
                         📅 Termine: (noch nicht implementiert)\n\
                         📧 E-Mails: (noch nicht implementiert)\n\
                         ✅ Aufgaben: (noch nicht implementiert)"
                    ),
                })
            },

            AgentCommand::SummarizeInbox {
                count,
                only_important,
            } => {
                // TODO: Implement with Proton Mail integration
                Ok(ExecutionResult {
                    success: true,
                    response: format!(
                        "📧 Inbox-Zusammenfassung (letzte {} E-Mails{}): \n\n\
                         (Proton Mail Integration noch nicht implementiert)",
                        count.unwrap_or(10),
                        if *only_important == Some(true) {
                            ", nur wichtige"
                        } else {
                            ""
                        }
                    ),
                })
            },

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
                // TODO: Implement model switching
                Ok(ExecutionResult {
                    success: false,
                    response: format!(
                        "⚙️ Modellwechsel zu '{model_name}' noch nicht implementiert."
                    ),
                })
            },

            SystemCommand::ReloadConfig => {
                // TODO: Implement config reload
                Ok(ExecutionResult {
                    success: false,
                    response: "⚙️ Konfiguration neu laden noch nicht implementiert.".to_string(),
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
            fn current_model(&self) -> &'static str;
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
        assert!(result.response.contains("2025-01-15"));
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
        mock.expect_current_model().returning(|| "qwen2.5-1.5b");

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
        mock.expect_current_model().returning(|| "test-model");

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
        mock.expect_current_model().returning(|| "qwen2.5-1.5b");

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
        let mock = MockInferenceEngine::new();
        let service = AgentService::new(Arc::new(mock));

        let result = service
            .execute_command(&AgentCommand::System(SystemCommand::SwitchModel {
                model_name: "llama".to_string(),
            }))
            .await
            .unwrap();

        assert!(!result.success);
        assert!(result.response.contains("llama"));
    }

    #[tokio::test]
    async fn execute_system_reload_config() {
        let mock = MockInferenceEngine::new();
        let service = AgentService::new(Arc::new(mock));

        let result = service
            .execute_command(&AgentCommand::System(SystemCommand::ReloadConfig))
            .await
            .unwrap();

        assert!(!result.success);
        assert!(result.response.contains("nicht implementiert"));
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
