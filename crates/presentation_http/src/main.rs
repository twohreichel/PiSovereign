//! PiSovereign HTTP Server
//!
//! Main entry point for the HTTP API server.

use std::sync::Arc;

use application::{AgentService, ChatService};
use infrastructure::{AppConfig, HailoInferenceAdapter};
use presentation_http::{
    ApiKeyAuthLayer, RateLimiterConfig, RateLimiterLayer, routes, state::AppState,
};
use tokio::net::TcpListener;
use tower_http::{
    cors::{Any, CorsLayer},
    trace::TraceLayer,
};
use tracing::info;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

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
        .map_err(|e| anyhow::anyhow!("Failed to initialize inference: {e}"))?;

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

    // Configure CORS layer
    let cors_layer = if config.server.allowed_origins.is_empty() {
        // Development mode: allow all origins
        CorsLayer::new()
            .allow_origin(Any)
            .allow_methods(Any)
            .allow_headers(Any)
    } else {
        // Production mode: restrict to configured origins
        use axum::http::{HeaderValue, Method};
        let origins: Vec<HeaderValue> = config
            .server
            .allowed_origins
            .iter()
            .filter_map(|o| o.parse().ok())
            .collect();
        CorsLayer::new()
            .allow_origin(origins)
            .allow_methods([Method::GET, Method::POST, Method::PUT, Method::DELETE])
            .allow_headers(Any)
    };

    // Configure rate limiter
    let rate_limiter = RateLimiterLayer::new(&RateLimiterConfig {
        enabled: config.security.rate_limit_enabled,
        requests_per_minute: config.security.rate_limit_rpm,
    });

    // Configure API key auth
    let auth_layer = ApiKeyAuthLayer::new(config.security.api_key.clone());

    // Add middleware (order matters: first added = outermost)
    let app = app
        .layer(TraceLayer::new_for_http())
        .layer(cors_layer)
        .layer(rate_limiter)
        .layer(auth_layer);

    // Start server
    let addr = format!("{}:{}", config.server.host, config.server.port);
    let listener = TcpListener::bind(&addr).await?;

    info!("🚀 Server listening on http://{}", addr);
    info!("📚 API docs: http://{}/health", addr);

    axum::serve(listener, app).await?;

    Ok(())
}
