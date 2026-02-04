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
        // Metrics endpoints
        .route("/metrics", get(handlers::metrics::get_metrics))
        .route("/metrics/prometheus", get(handlers::metrics::get_metrics_prometheus))
        // Chat API (v1)
        .route("/v1/chat", post(handlers::chat::chat))
        .route("/v1/chat/stream", post(handlers::chat::chat_stream))
        // Command API (v1)
        .route("/v1/commands", post(handlers::commands::execute_command))
        .route("/v1/commands/parse", post(handlers::commands::parse_command))
        // Approval API (v1)
        .route("/v1/approvals", get(handlers::approvals::list_approvals))
        .route("/v1/approvals/{id}", get(handlers::approvals::get_approval))
        .route(
            "/v1/approvals/{id}/approve",
            post(handlers::approvals::approve_request),
        )
        .route(
            "/v1/approvals/{id}/deny",
            post(handlers::approvals::deny_request),
        )
        .route(
            "/v1/approvals/{id}/cancel",
            post(handlers::approvals::cancel_request),
        )
        // System API
        .route("/v1/system/status", get(handlers::system::status))
        .route("/v1/system/models", get(handlers::system::list_models))
        // WhatsApp webhook
        .route(
            "/webhook/whatsapp",
            get(handlers::whatsapp::verify_webhook).post(handlers::whatsapp::handle_webhook),
        )
        // Attach state
        .with_state(state)
}
