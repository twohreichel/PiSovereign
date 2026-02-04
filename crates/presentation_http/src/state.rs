//! Application state shared across handlers

use std::sync::Arc;

use application::{AgentService, ChatService};

use crate::config_reload::ReloadableConfig;

/// Shared application state
#[derive(Clone, Debug)]
pub struct AppState {
    /// Chat service for conversation handling
    pub chat_service: Arc<ChatService>,
    /// Agent service for command execution
    pub agent_service: Arc<AgentService>,
    /// Reloadable application configuration
    pub config: ReloadableConfig,
}
