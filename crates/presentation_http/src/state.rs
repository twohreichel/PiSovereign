//! Application state shared across handlers

use std::sync::Arc;

use application::ports::{MessengerPort, SuspiciousActivityPort};
use application::services::PromptSanitizer;
use application::{AgentService, ApprovalService, ChatService, HealthService, VoiceMessageService};
use integration_signal::SignalClient;

use crate::{config_reload::ReloadableConfig, handlers::metrics::MetricsCollector};

/// Shared application state
#[derive(Clone)]
pub struct AppState {
    /// Chat service for conversation handling
    pub chat_service: Arc<ChatService>,
    /// Agent service for command execution
    pub agent_service: Arc<AgentService>,
    /// Approval service for managing approval workflows
    pub approval_service: Option<Arc<ApprovalService>>,
    /// Health aggregation service
    pub health_service: Option<Arc<HealthService>>,
    /// Voice message service for STT/TTS processing
    pub voice_message_service: Option<Arc<VoiceMessageService>>,
    /// Reloadable application configuration
    pub config: ReloadableConfig,
    /// Metrics collector
    pub metrics: Arc<MetricsCollector>,
    /// Unified messenger adapter (WhatsApp or Signal)
    pub messenger_adapter: Option<Arc<dyn MessengerPort>>,
    /// Signal client for direct access (polling messages)
    pub signal_client: Option<Arc<SignalClient>>,
    /// Prompt sanitizer for security analysis
    pub prompt_sanitizer: Option<Arc<PromptSanitizer>>,
    /// Suspicious activity tracker for IP-based blocking
    pub suspicious_activity_tracker: Option<Arc<dyn SuspiciousActivityPort>>,
}

impl std::fmt::Debug for AppState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AppState")
            .field("chat_service", &self.chat_service)
            .field("agent_service", &self.agent_service)
            .field("approval_service", &self.approval_service.is_some())
            .field("health_service", &self.health_service.is_some())
            .field(
                "voice_message_service",
                &self.voice_message_service.is_some(),
            )
            .field("config", &self.config)
            .field("metrics", &"<MetricsCollector>")
            .field("messenger_adapter", &self.messenger_adapter.is_some())
            .field("signal_client", &self.signal_client.is_some())
            .field("prompt_sanitizer", &self.prompt_sanitizer.is_some())
            .field(
                "suspicious_activity_tracker",
                &self.suspicious_activity_tracker.is_some(),
            )
            .finish()
    }
}
