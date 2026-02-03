//! PiSovereign HTTP Server
//!
//! Main entry point for the HTTP API server.

use std::sync::Arc;

use axum::Router;
use tokio::net::TcpListener;
use tower_http::cors::{Any, CorsLayer};
use tower_http::trace::TraceLayer;
use tracing::{info, Level};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use application::{AgentService, ChatService};
use infrastructure::{AppConfig, HailoInferenceAdapter};

mod error;
mod handlers;
mod routes;
mod state;

use state::AppState;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize tracing
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "pisovereign_server=debug,tower_http=debug".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    info!("🤖 PiSovereign v{} starting...", env!("CARGO_PKG_VERSION"));

    // Load configuration
    let config = AppConfig::load().unwrap_or_else(|e| {
        tracing::warn!("Failed to load config, using defaults: {}", e);
        AppConfig::default()
    });

    info!(
        host = %config.server.host,
        port = %config.server.port,
        model = %config.inference.default_model,
        "Configuration loaded"
    );

    // Initialize inference adapter
    let inference_adapter = HailoInferenceAdapter::new(config.inference.clone())
        .map_err(|e| anyhow::anyhow!("Failed to initialize inference: {}", e))?;

    let inference: Arc<dyn application::ports::InferencePort> = Arc::new(inference_adapter);

    // Initialize services
    let chat_service = ChatService::with_system_prompt(
        Arc::clone(&inference),
        "Du bist PiSovereign, ein hilfreicher, auf Deutsch antwortender KI-Assistent, \
         der auf einem Raspberry Pi 5 mit Hailo-10H läuft. Du bist freundlich, präzise \
         und hilfst bei alltäglichen Aufgaben wie E-Mail, Kalender und Informationssuche.",
    );

    let agent_service = AgentService::new(Arc::clone(&inference));

    // Create app state
    let state = AppState {
        chat_service: Arc::new(chat_service),
        agent_service: Arc::new(agent_service),
        config: Arc::new(config.clone()),
    };

    // Build router
    let app = routes::create_router(state);

    // Add middleware
    let app = app
        .layer(TraceLayer::new_for_http())
        .layer(
            CorsLayer::new()
                .allow_origin(Any)
                .allow_methods(Any)
                .allow_headers(Any),
        );

    // Start server
    let addr = format!("{}:{}", config.server.host, config.server.port);
    let listener = TcpListener::bind(&addr).await?;

    info!("🚀 Server listening on http://{}", addr);
    info!("📚 API docs: http://{}/health", addr);

    axum::serve(listener, app).await?;

    Ok(())
}
