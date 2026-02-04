//! Application state shared across handlers

use std::sync::Arc;

use application::{AgentService, ApprovalService, ChatService};

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
    /// Reloadable application configuration
    pub config: ReloadableConfig,
    /// Metrics collector
    pub metrics: Arc<MetricsCollector>,
}

impl std::fmt::Debug for AppState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AppState")
            .field("chat_service", &self.chat_service)
            .field("agent_service", &self.agent_service)
            .field("approval_service", &self.approval_service.is_some())
            .field("config", &self.config)
            .field("metrics", &"<MetricsCollector>")
            .finish()
    }
}
