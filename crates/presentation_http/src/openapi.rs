//! OpenAPI documentation module
//!
//! Provides OpenAPI 3.0 documentation for the PiSovereign HTTP API.
//! Includes Swagger UI and ReDoc for interactive API exploration.

// Allow clippy warnings from macro-generated code in utoipa derive
#![allow(clippy::needless_for_each)]

use axum::{Router, response::Html, routing::get};
use utoipa::OpenApi;
use utoipa_redoc::{Redoc, Servable as RedocServable};
use utoipa_swagger_ui::SwaggerUi;

use crate::{handlers, state::AppState};

/// OpenAPI documentation for PiSovereign
#[derive(OpenApi)]
#[openapi(
    info(
        title = "PiSovereign API",
        version = "0.1.0",
        description = "Personal AI assistant API running on Raspberry Pi 5 with Hailo-10H accelerator",
        license(name = "MIT", url = "https://opensource.org/licenses/MIT"),
        contact(
            name = "PiSovereign",
            url = "https://github.com/andreasreichel/PiSovereign"
        )
    ),
    servers(
        (url = "/", description = "Local server")
    ),
    tags(
        (name = "health", description = "Health check and readiness endpoints"),
        (name = "chat", description = "Conversational AI chat endpoints"),
        (name = "commands", description = "Natural language command execution"),
        (name = "approvals", description = "Approval workflow management"),
        (name = "system", description = "System status and model information"),
        (name = "metrics", description = "Application metrics and observability")
    ),
    paths(
        // Health endpoints
        handlers::health::health_check,
        handlers::health::readiness_check,
        // Chat endpoints
        handlers::chat::chat,
        handlers::chat::chat_stream,
        // Command endpoints
        handlers::commands::execute_command,
        handlers::commands::parse_command,
        // Approval endpoints
        handlers::approvals::list_approvals,
        handlers::approvals::get_approval,
        handlers::approvals::approve_request,
        handlers::approvals::deny_request,
        handlers::approvals::cancel_request,
        // System endpoints
        handlers::system::status,
        handlers::system::list_models,
        // Metrics endpoints
        handlers::metrics::get_metrics,
        handlers::metrics::get_metrics_prometheus,
    ),
    components(
        schemas(
            // Health schemas
            handlers::health::HealthResponse,
            handlers::health::ReadinessResponse,
            handlers::health::ServiceStatus,
            // Chat schemas
            handlers::chat::ChatRequest,
            handlers::chat::ChatResponse,
            handlers::chat::StreamChatRequest,
            // Command schemas
            handlers::commands::ExecuteCommandRequest,
            handlers::commands::ExecuteCommandResponse,
            handlers::commands::ParseCommandRequest,
            handlers::commands::ParseCommandResponse,
            // Approval schemas
            handlers::approvals::ApprovalResponse,
            handlers::approvals::ListApprovalsQuery,
            handlers::approvals::DenyRequest,
            // System schemas
            handlers::system::StatusResponse,
            handlers::system::ModelsResponse,
            handlers::system::ModelInfo,
            // Metrics schemas
            handlers::metrics::MetricsResponse,
            handlers::metrics::AppMetrics,
            handlers::metrics::RequestMetrics,
            handlers::metrics::InferenceMetrics,
            handlers::metrics::SystemMetrics,
            // Error schemas
            crate::error::ErrorResponse,
            // Domain schemas (inline re-definitions for OpenAPI)
            AgentCommandSchema,
            SystemCommandSchema,
            ApprovalStatusSchema,
        )
    ),
    security(
        ("api_key" = []),
        ("admin_key" = [])
    ),
    modifiers(&SecurityAddon)
)]
#[derive(Debug)]
pub struct ApiDoc;

/// Security scheme modifier for OpenAPI
struct SecurityAddon;

impl utoipa::Modify for SecurityAddon {
    fn modify(&self, openapi: &mut utoipa::openapi::OpenApi) {
        if let Some(components) = openapi.components.as_mut() {
            use utoipa::openapi::security::{ApiKey, ApiKeyValue, SecurityScheme};

            components.add_security_scheme(
                "api_key",
                SecurityScheme::ApiKey(ApiKey::Header(ApiKeyValue::new("Authorization"))),
            );
            components.add_security_scheme(
                "admin_key",
                SecurityScheme::ApiKey(ApiKey::Header(ApiKeyValue::new("X-Admin-Key"))),
            );
        }
    }
}

/// Simplified AgentCommand schema for OpenAPI documentation
#[derive(Debug, utoipa::ToSchema)]
#[schema(example = json!({
    "type": "ask",
    "question": "What is the weather like today?"
}))]
#[allow(dead_code)]
pub enum AgentCommandSchema {
    /// Request a morning briefing
    #[schema(rename = "morning_briefing")]
    MorningBriefing {
        /// Date for the briefing
        date: Option<String>,
    },
    /// Create a calendar event
    #[schema(rename = "create_calendar_event")]
    CreateCalendarEvent {
        date: String,
        time: String,
        title: String,
        duration_minutes: Option<u32>,
        location: Option<String>,
    },
    /// Summarize inbox
    #[schema(rename = "summarize_inbox")]
    SummarizeInbox {
        count: Option<u32>,
        only_important: Option<bool>,
    },
    /// Draft an email
    #[schema(rename = "draft_email")]
    DraftEmail {
        to: String,
        subject: Option<String>,
        body: String,
    },
    /// Send a drafted email
    #[schema(rename = "send_email")]
    SendEmail { draft_id: String },
    /// Ask a question
    #[schema(rename = "ask")]
    Ask { question: String },
    /// System command
    #[schema(rename = "system")]
    System(SystemCommandSchema),
    /// Echo a message
    #[schema(rename = "echo")]
    Echo { message: String },
    /// Show help
    #[schema(rename = "help")]
    Help { command: Option<String> },
    /// Unknown command
    #[schema(rename = "unknown")]
    Unknown { original_input: String },
}

/// System command variants for OpenAPI
#[derive(Debug, utoipa::ToSchema)]
#[allow(dead_code)]
pub enum SystemCommandSchema {
    /// Get system status
    Status,
    /// Get version info
    Version,
    /// Reload configuration
    ReloadConfig,
    /// List available models
    ListModels,
    /// Switch to a different model
    SwitchModel { model: String },
}

/// Approval status for OpenAPI
#[derive(Debug, utoipa::ToSchema)]
#[allow(dead_code)]
pub enum ApprovalStatusSchema {
    /// Awaiting approval
    Pending,
    /// Approved for execution
    Approved,
    /// Denied by user
    Denied,
    /// Cancelled by user
    Cancelled,
    /// Expired without action
    Expired,
}

/// Create OpenAPI documentation routes
///
/// Adds the following routes:
/// - `/api-docs/openapi.json` - OpenAPI specification (used by Swagger UI)
/// - `/swagger-ui/*` - Swagger UI interactive documentation
/// - `/redoc` - ReDoc documentation
pub fn create_openapi_routes() -> Router<AppState> {
    let redoc = Redoc::with_url("/api-docs/openapi.json", ApiDoc::openapi());

    Router::new()
        // ReDoc documentation
        .route("/redoc", get(|| async move { Html(redoc.to_html()) }))
        // Swagger UI with assets - SwaggerUi will serve /api-docs/openapi.json internally
        .merge(
            SwaggerUi::new("/swagger-ui")
                .url("/api-docs/openapi.json", ApiDoc::openapi()),
        )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn openapi_spec_is_valid() {
        let doc = ApiDoc::openapi();
        let json = serde_json::to_string_pretty(&doc).expect("Failed to serialize OpenAPI spec");
        assert!(json.contains("PiSovereign API"));
        assert!(json.contains("/health"));
        assert!(json.contains("/v1/chat"));
    }

    #[test]
    fn openapi_has_all_tags() {
        let doc = ApiDoc::openapi();
        let tags: Vec<&str> = doc
            .tags
            .as_ref()
            .map(|t| t.iter().map(|tag| tag.name.as_str()).collect())
            .unwrap_or_default();

        assert!(tags.contains(&"health"));
        assert!(tags.contains(&"chat"));
        assert!(tags.contains(&"commands"));
        assert!(tags.contains(&"approvals"));
        assert!(tags.contains(&"system"));
        assert!(tags.contains(&"metrics"));
    }

    #[test]
    fn openapi_has_security_schemes() {
        let doc = ApiDoc::openapi();
        let components = doc.components.expect("Missing components");
        let schemes = components.security_schemes;

        assert!(schemes.contains_key("api_key"));
        assert!(schemes.contains_key("admin_key"));
    }

    #[test]
    fn agent_command_schema_has_variants() {
        // Just verify the schema compiles and can be used
        let _schema: AgentCommandSchema = AgentCommandSchema::Ask {
            question: "test".to_string(),
        };
    }
}
