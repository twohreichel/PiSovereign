//! PiSovereign HTTP Server
//!
//! Main entry point for the HTTP API server.

use std::{sync::Arc, time::Duration};

use application::{AgentService, ChatService};
use infrastructure::{AppConfig, HailoInferenceAdapter};
use presentation_http::{
    ApiKeyAuthLayer, RateLimiterConfig, RateLimiterLayer, ReloadableConfig,
    handlers::metrics::MetricsCollector, routes, spawn_config_reload_handler, state::AppState,
};
use tokio::{net::TcpListener, signal};
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
    let initial_config = AppConfig::load().unwrap_or_else(|e| {
        tracing::warn!("Failed to load config, using defaults: {}", e);
        AppConfig::default()
    });

    info!(
        host = %initial_config.server.host,
        port = %initial_config.server.port,
        model = %initial_config.inference.default_model,
        "Configuration loaded"
    );

    // Create reloadable config and spawn SIGHUP handler
    let reloadable_config =
        spawn_config_reload_handler(ReloadableConfig::new(initial_config.clone()));

    // Initialize inference adapter
    let inference_adapter = HailoInferenceAdapter::new(initial_config.inference.clone())
        .map_err(|e| anyhow::anyhow!("Failed to initialize inference: {e}"))?;

    let inference: Arc<dyn application::ports::InferencePort> = Arc::new(inference_adapter);

    // Initialize services
    let chat_service = ChatService::with_system_prompt(
        Arc::clone(&inference),
        "You are PiSovereign, a helpful AI assistant running on a Raspberry Pi 5 with Hailo-10H. \
         You are friendly, precise, and help with everyday tasks like email, calendar, and information lookup.",
    );

    let agent_service = AgentService::new(Arc::clone(&inference));

    // Initialize metrics collector
    let metrics = Arc::new(MetricsCollector::new());

    // Create app state with reloadable config
    // Note: ApprovalService is optional and not initialized here for now
    // It would require ApprovalQueuePort and AuditLogPort implementations
    let state = AppState {
        chat_service: Arc::new(chat_service),
        agent_service: Arc::new(agent_service),
        approval_service: None, // TODO: Initialize when persistence is configured
        config: reloadable_config,
        metrics,
    };

    // Build router
    let app = routes::create_router(state);

    // Configure CORS layer
    let cors_layer = if initial_config.server.allowed_origins.is_empty() {
        // Development mode: allow all origins
        CorsLayer::new()
            .allow_origin(Any)
            .allow_methods(Any)
            .allow_headers(Any)
    } else {
        // Production mode: restrict to configured origins
        use axum::http::{HeaderValue, Method};
        let origins: Vec<HeaderValue> = initial_config
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
        enabled: initial_config.security.rate_limit_enabled,
        requests_per_minute: initial_config.security.rate_limit_rpm,
    });

    // Configure API key auth
    let auth_layer = ApiKeyAuthLayer::new(initial_config.security.api_key.clone());

    // Add middleware (order matters: first added = outermost)
    let app = app
        .layer(TraceLayer::new_for_http())
        .layer(cors_layer)
        .layer(rate_limiter)
        .layer(auth_layer);

    // Start server
    let addr = format!(
        "{}:{}",
        initial_config.server.host, initial_config.server.port
    );
    let listener = TcpListener::bind(&addr).await?;

    info!("🚀 Server listening on http://{}", addr);
    info!("📚 API docs: http://{}/health", addr);
    info!("🔄 SIGHUP for config reload is enabled (Unix only)");

    // Graceful shutdown configuration
    let shutdown_timeout =
        Duration::from_secs(initial_config.server.shutdown_timeout_secs.unwrap_or(30));

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal(shutdown_timeout))
        .await?;

    info!("👋 Server shutdown complete");

    Ok(())
}

/// Wait for shutdown signals (SIGINT, SIGTERM) and handle graceful shutdown
#[allow(clippy::expect_used)]
async fn shutdown_signal(timeout: Duration) {
    let ctrl_c = async {
        // Log error but continue waiting - this is a best-effort signal handler
        if let Err(e) = signal::ctrl_c().await {
            tracing::error!("Failed to install Ctrl+C handler: {}", e);
        }
    };

    #[cfg(unix)]
    let terminate = async {
        // unwrap is acceptable here as failure to install signal handler is unrecoverable
        match signal::unix::signal(signal::unix::SignalKind::terminate()) {
            Ok(mut signal) => {
                signal.recv().await;
            },
            Err(e) => {
                tracing::error!("Failed to install SIGTERM handler: {}", e);
                std::future::pending::<()>().await;
            },
        }
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        () = ctrl_c => {
            info!("📥 Received Ctrl+C, initiating graceful shutdown...");
        }
        () = terminate => {
            info!("📥 Received SIGTERM, initiating graceful shutdown...");
        }
    }

    info!("⏳ Waiting up to {:?} for connections to close...", timeout);
    // Note: The actual connection draining is handled by axum's graceful_shutdown
}
