//! Agent service - Command execution and orchestration

use std::sync::Arc;
use std::time::Instant;
use tracing::{debug, info, instrument, warn};

use domain::{AgentCommand, SystemCommand};

use crate::command_parser::CommandParser;
use crate::error::ApplicationError;
use crate::ports::InferencePort;

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
#[derive(Debug, Clone, PartialEq)]
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
    #[instrument(skip(self), fields(command_type = ?std::mem::discriminant(&command)))]
    pub async fn execute_command(&self, command: &AgentCommand) -> Result<ExecutionResult, ApplicationError> {
        match command {
            AgentCommand::Echo { message } => Ok(ExecutionResult {
                success: true,
                response: format!("🔊 {}", message),
            }),

            AgentCommand::Help { command: cmd } => {
                let help_text = self.generate_help(cmd.as_deref());
                Ok(ExecutionResult {
                    success: true,
                    response: help_text,
                })
            }

            AgentCommand::System(sys_cmd) => self.handle_system_command(sys_cmd).await,

            AgentCommand::Ask { question } => {
                let response = self.inference.generate(question).await?;
                Ok(ExecutionResult {
                    success: true,
                    response: response.content,
                })
            }

            AgentCommand::MorningBriefing { date } => {
                let date_str = date
                    .map(|d| d.to_string())
                    .unwrap_or_else(|| "heute".to_string());
                
                // TODO: Implement actual briefing with calendar/email integration
                Ok(ExecutionResult {
                    success: true,
                    response: format!(
                        "☀️ Guten Morgen! Hier ist dein Briefing für {}:\n\n\
                         📅 Termine: (noch nicht implementiert)\n\
                         📧 E-Mails: (noch nicht implementiert)\n\
                         ✅ Aufgaben: (noch nicht implementiert)",
                        date_str
                    ),
                })
            }

            AgentCommand::SummarizeInbox { count, only_important } => {
                // TODO: Implement with Proton Mail integration
                Ok(ExecutionResult {
                    success: true,
                    response: format!(
                        "📧 Inbox-Zusammenfassung (letzte {} E-Mails{}): \n\n\
                         (Proton Mail Integration noch nicht implementiert)",
                        count.unwrap_or(10),
                        if *only_important == Some(true) { ", nur wichtige" } else { "" }
                    ),
                })
            }

            AgentCommand::Unknown { original_input } => {
                warn!(input = %original_input, "Unknown command received");
                Ok(ExecutionResult {
                    success: false,
                    response: format!(
                        "❓ Ich konnte den Befehl nicht verstehen: '{}'\n\n\
                         Schreibe 'hilfe' für eine Liste der verfügbaren Befehle.",
                        original_input
                    ),
                })
            }

            // Commands that require approval - should not reach here without approval
            AgentCommand::CreateCalendarEvent { .. }
            | AgentCommand::DraftEmail { .. }
            | AgentCommand::SendEmail { .. } => {
                Err(ApplicationError::ApprovalRequired(command.description()))
            }
        }
    }

    /// Handle system commands
    async fn handle_system_command(&self, cmd: &SystemCommand) -> Result<ExecutionResult, ApplicationError> {
        match cmd {
            SystemCommand::Status => {
                let healthy = self.inference.is_healthy().await;
                let status = if healthy { "🟢 Online" } else { "🔴 Offline" };
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
            }

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
            }

            SystemCommand::SwitchModel { model_name } => {
                // TODO: Implement model switching
                Ok(ExecutionResult {
                    success: false,
                    response: format!(
                        "⚙️ Modellwechsel zu '{}' noch nicht implementiert.",
                        model_name
                    ),
                })
            }

            SystemCommand::ReloadConfig => {
                // TODO: Implement config reload
                Ok(ExecutionResult {
                    success: false,
                    response: "⚙️ Konfiguration neu laden noch nicht implementiert.".to_string(),
                })
            }
        }
    }

    /// Generate help text
    fn generate_help(&self, command: Option<&str>) -> String {
        match command {
            Some("briefing") | Some("morgen") => {
                "☀️ **Morning Briefing**\n\n\
                 Zeigt eine Übersicht über Termine, E-Mails und Aufgaben.\n\n\
                 Beispiele:\n\
                 • 'briefing'\n\
                 • 'briefing für morgen'\n\
                 • 'was steht heute an?'"
                    .to_string()
            }
            Some("email") | Some("mail") => {
                "📧 **E-Mail Befehle**\n\n\
                 • 'inbox zusammenfassen' - Zusammenfassung der E-Mails\n\
                 • 'mail an X schreiben' - E-Mail-Entwurf erstellen\n\
                 • 'wichtige mails' - Nur wichtige E-Mails anzeigen"
                    .to_string()
            }
            Some("kalender") | Some("termin") => {
                "📅 **Kalender Befehle**\n\n\
                 • 'termin am X um Y' - Neuen Termin erstellen\n\
                 • 'termine heute' - Heutige Termine anzeigen\n\
                 • 'nächster termin' - Nächsten Termin anzeigen"
                    .to_string()
            }
            Some("status") | Some("system") => {
                "🔧 **System Befehle**\n\n\
                 • 'status' - Systemstatus anzeigen\n\
                 • 'version' - Versionsinformation\n\
                 • 'modelle' - Verfügbare KI-Modelle"
                    .to_string()
            }
            _ => {
                "🤖 **PiSovereign Hilfe**\n\n\
                 Verfügbare Befehle:\n\n\
                 • 'hilfe [thema]' - Diese Hilfe\n\
                 • 'briefing' - Tagesübersicht\n\
                 • 'inbox' - E-Mail-Zusammenfassung\n\
                 • 'termin ...' - Kalenderfunktionen\n\
                 • 'status' - Systemstatus\n\
                 • 'echo [text]' - Text zurückgeben\n\n\
                 Du kannst auch einfach Fragen stellen!"
                    .to_string()
            }
        }
    }
}

/// Result of command execution
#[derive(Debug)]
pub struct ExecutionResult {
    pub success: bool,
    pub response: String,
}
