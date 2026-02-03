//! Route definitions

use axum::{
    Router,
    routing::{get, post},
};

use crate::{handlers, state::AppState};

/// Create the main router with all routes
pub fn create_router(state: AppState) -> Router {
    Router::new()
        // Health and status endpoints
        .route("/health", get(handlers::health::health_check))
        .route("/ready", get(handlers::health::readiness_check))
        // Chat API (v1)
        .route("/v1/chat", post(handlers::chat::chat))
        .route("/v1/chat/stream", post(handlers::chat::chat_stream))
        // Command API (v1)
        .route("/v1/commands", post(handlers::commands::execute_command))
        .route("/v1/commands/parse", post(handlers::commands::parse_command))
        // System API
        .route("/v1/system/status", get(handlers::system::status))
        .route("/v1/system/models", get(handlers::system::list_models))
        // Attach state
        .with_state(state)
}
