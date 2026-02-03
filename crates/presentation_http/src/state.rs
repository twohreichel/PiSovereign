//! Application state shared across handlers

use std::sync::Arc;

use application::{AgentService, ChatService};
use infrastructure::AppConfig;

/// Shared application state
#[derive(Clone)]
pub struct AppState {
    /// Chat service for conversation handling
    pub chat_service: Arc<ChatService>,
    /// Agent service for command execution
    pub agent_service: Arc<AgentService>,
    /// Application configuration
    pub config: Arc<AppConfig>,
}
