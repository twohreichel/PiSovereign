//! System command handlers (status, version, models, config reload) and help text

use domain::SystemCommand;
use tracing::{info, warn};

use super::{AgentService, ExecutionResult};
use crate::error::ApplicationError;

impl AgentService {
    /// Handle system commands
    pub(super) async fn handle_system_command(
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
                         Hailo Inference: {}\n\
                         Model: {}\n\
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
                let current_model = self.inference.current_model();
                match self.inference.list_available_models().await {
                    Ok(models) => {
                        let model_list: String = models
                            .iter()
                            .map(|m| {
                                if m == &current_model {
                                    format!("• {m} (active)")
                                } else {
                                    format!("• {m}")
                                }
                            })
                            .collect::<Vec<_>>()
                            .join("\n");

                        Ok(ExecutionResult {
                            success: true,
                            response: format!(
                                "📦 Available Models:\n\n{model_list}\n\nCurrent: {current_model}"
                            ),
                        })
                    },
                    Err(e) => {
                        warn!(error = %e, "Failed to list models from inference service");
                        Ok(ExecutionResult {
                            success: false,
                            response: format!(
                                "⚠️ Could not retrieve model list: {e}\n\nCurrent: {current_model}"
                            ),
                        })
                    },
                }
            },

            SystemCommand::SwitchModel { model_name } => {
                match self.inference.switch_model(model_name).await {
                    Ok(()) => {
                        info!(model = %model_name, "Model switched successfully");
                        Ok(ExecutionResult {
                            success: true,
                            response: format!("✅ Model successfully switched to '{model_name}'."),
                        })
                    },
                    Err(e) => {
                        warn!(model = %model_name, error = %e, "Model switch failed");
                        Ok(ExecutionResult {
                            success: false,
                            response: format!("❌ Model switch failed: {e}"),
                        })
                    },
                }
            },

            SystemCommand::ReloadConfig => {
                // Config reload is handled at the HTTP layer via SIGHUP
                Ok(ExecutionResult {
                    success: true,
                    response: "🔄 Configuration is being reloaded. Send SIGHUP to the server or use the API.".to_string(),
                })
            },
        }
    }

    /// Generate help text
    #[allow(clippy::unused_self)]
    pub(super) fn generate_help(&self, command: Option<&str>) -> String {
        match command {
            Some("briefing" | "morning") => "☀️ **Morning Briefing**\n\n\
                 Shows an overview of appointments, emails, and tasks.\n\n\
                 Examples:\n\
                 • 'briefing'\n\
                 • 'briefing for tomorrow'\n\
                 • 'what's on today?'"
                .to_string(),
            Some("email" | "mail") => "📧 **Email Commands**\n\n\
                 • 'summarize inbox' - Summarize emails\n\
                 • 'write mail to X' - Create email draft\n\
                 • 'important mails' - Show only important emails"
                .to_string(),
            Some("calendar" | "appointment") => "📅 **Calendar Commands**\n\n\
                 • 'appointment on X at Y' - Create new appointment\n\
                 • 'appointments today' - Show today's appointments\n\
                 • 'next appointment' - Show next appointment"
                .to_string(),
            Some("status" | "system") => "🔧 **System Commands**\n\n\
                 • 'status' - Show system status\n\
                 • 'version' - Version information\n\
                 • 'models' - Available AI models"
                .to_string(),
            _ => "🤖 **PiSovereign Help**\n\n\
                 Available commands:\n\n\
                 • 'help [topic]' - This help\n\
                 • 'briefing' - Daily overview\n\
                 • 'inbox' - Email summary\n\
                 • 'appointment ...' - Calendar functions\n\
                 • 'status' - System status\n\
                 • 'echo [text]' - Return text\n\n\
                 You can also just ask questions!"
                .to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use domain::{AgentCommand, SystemCommand};

    use super::super::{AgentService, test_support::MockInferenceEngine};
    use crate::error::ApplicationError;

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
        assert!(result.response.contains("Help"));
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
        assert!(result.response.contains("Email"));
    }

    #[tokio::test]
    async fn execute_help_calendar() {
        let mock = MockInferenceEngine::new();
        let service = AgentService::new(Arc::new(mock));

        let result = service
            .execute_command(&AgentCommand::Help {
                command: Some("calendar".to_string()),
            })
            .await
            .unwrap();

        assert!(result.success);
        assert!(result.response.contains("Calendar"));
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
    async fn execute_system_list_models_success() {
        let mut mock = MockInferenceEngine::new();
        mock.expect_current_model()
            .returning(|| "qwen2.5-1.5b".to_string());
        mock.expect_list_available_models().returning(|| {
            Ok(vec![
                "qwen2.5-1.5b".to_string(),
                "llama3.2-1b".to_string(),
                "mistral-7b".to_string(),
            ])
        });

        let service = AgentService::new(Arc::new(mock));

        let result = service
            .execute_command(&AgentCommand::System(SystemCommand::ListModels))
            .await
            .unwrap();

        assert!(result.success);
        assert!(result.response.contains("Available Models"));
        assert!(result.response.contains("qwen2.5-1.5b (active)"));
        assert!(result.response.contains("• llama3.2-1b"));
        assert!(result.response.contains("• mistral-7b"));
        assert!(!result.response.contains("llama3.2-1b (active)"));
    }

    #[tokio::test]
    async fn execute_system_list_models_error_fallback() {
        let mut mock = MockInferenceEngine::new();
        mock.expect_current_model()
            .returning(|| "qwen2.5-1.5b".to_string());
        mock.expect_list_available_models().returning(|| {
            Err(ApplicationError::ExternalService(
                "Circuit breaker open".to_string(),
            ))
        });

        let service = AgentService::new(Arc::new(mock));

        let result = service
            .execute_command(&AgentCommand::System(SystemCommand::ListModels))
            .await
            .unwrap();

        assert!(!result.success);
        assert!(result.response.contains("Could not retrieve model list"));
        assert!(result.response.contains("Current: qwen2.5-1.5b"));
    }

    #[tokio::test]
    async fn execute_system_list_models_empty() {
        let mut mock = MockInferenceEngine::new();
        mock.expect_current_model()
            .returning(|| "unknown".to_string());
        mock.expect_list_available_models().returning(|| Ok(vec![]));

        let service = AgentService::new(Arc::new(mock));

        let result = service
            .execute_command(&AgentCommand::System(SystemCommand::ListModels))
            .await
            .unwrap();

        assert!(result.success);
        assert!(result.response.contains("Available Models"));
        assert!(result.response.contains("Current: unknown"));
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

        assert!(result.success);
        assert!(result.response.contains("Configuration"));
    }

    #[tokio::test]
    async fn generate_help_for_morning() {
        let mock = MockInferenceEngine::new();
        let service = AgentService::new(Arc::new(mock));

        let result = service
            .execute_command(&AgentCommand::Help {
                command: Some("morning".to_string()),
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

        assert!(result.response.contains("Email"));
    }

    #[tokio::test]
    async fn generate_help_for_appointment() {
        let mock = MockInferenceEngine::new();
        let service = AgentService::new(Arc::new(mock));

        let result = service
            .execute_command(&AgentCommand::Help {
                command: Some("appointment".to_string()),
            })
            .await
            .unwrap();

        assert!(result.response.contains("Calendar"));
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
}
