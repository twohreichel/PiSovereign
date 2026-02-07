//! PiSovereign HTTP Server
//!
//! Main entry point for the HTTP API server.

use std::{sync::Arc, time::Duration};

use application::{
    AgentService, ApprovalService, ChatService, HealthService,
    ports::{
        CalendarPort, ConversationStore, DatabaseHealthPort, EmailPort, InferencePort, WeatherPort,
    },
};
use infrastructure::{
    AppConfig, HailoInferenceAdapter, SecurityValidator,
    adapters::{
        CalDavCalendarAdapter, DegradedInferenceAdapter, DegradedModeConfig, ProtonEmailAdapter,
        WeatherAdapter,
    },
    persistence::{
        SqliteApprovalQueue, SqliteAuditLog, SqliteConversationStore, SqliteDatabaseHealth,
        create_pool,
    },
    telemetry::{TelemetryConfig, init_telemetry},
};
use presentation_http::{
    ApiKeyAuthLayer, RateLimiterConfig, RateLimiterLayer, ReloadableConfig, RequestIdLayer,
    handlers::metrics::MetricsCollector, routes, spawn_cleanup_task, spawn_config_reload_handler,
    state::AppState,
};
use tokio::{net::TcpListener, signal};
use tower_http::{
    cors::{Any, CorsLayer},
    trace::TraceLayer,
};
use tracing::{error, info, warn};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

/// System prompt for the AI assistant
const SYSTEM_PROMPT: &str = "You are PiSovereign, a helpful AI assistant running on a \
    Raspberry Pi 5 with Hailo-10H. You are friendly, precise, and help with everyday \
    tasks like email, calendar, and information lookup.";

/// Initialize the tracing subscriber based on configuration
///
/// In production mode, defaults to JSON format for structured logging
/// suitable for log aggregation (Loki, Elasticsearch, etc.).
fn init_tracing(log_format: &str, environment: Option<infrastructure::config::Environment>) {
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| "pisovereign_server=debug,tower_http=debug".into());

    // Determine effective log format:
    // - Use explicit config value if not "text" (the default)
    // - In production, default to JSON unless explicitly set to "text"
    let use_json = if log_format == "json" {
        true
    } else if log_format != "text" {
        // Unknown format, treat as JSON for safety
        true
    } else {
        // log_format == "text" - check if production
        matches!(
            environment,
            Some(infrastructure::config::Environment::Production)
        )
    };

    if use_json {
        // JSON format for production/structured logging
        tracing_subscriber::registry()
            .with(filter)
            .with(
                tracing_subscriber::fmt::layer()
                    .json()
                    .with_target(true)
                    .with_file(true)
                    .with_line_number(true)
                    .with_thread_ids(true)
                    .with_span_list(true),
            )
            .init();
    } else {
        // Human-readable text format for development
        tracing_subscriber::registry()
            .with(filter)
            .with(tracing_subscriber::fmt::layer())
            .init();
    }
}

#[tokio::main]
#[allow(clippy::too_many_lines)]
async fn main() -> anyhow::Result<()> {
    // Load configuration first to determine log format
    let initial_config = AppConfig::load().unwrap_or_else(|e| {
        // Can't log yet, print to stderr (allowed before tracing init)
        #[allow(clippy::print_stderr)]
        {
            eprintln!("Warning: Failed to load config, using defaults: {e}");
        }
        AppConfig::default()
    });

    // Initialize tracing with configured format
    init_tracing(
        &initial_config.server.log_format,
        initial_config.environment,
    );

    info!("🤖 PiSovereign v{} starting...", env!("CARGO_PKG_VERSION"));

    // Validate security configuration
    let security_warnings = SecurityValidator::validate(&initial_config);
    if !security_warnings.is_empty() {
        SecurityValidator::log_warnings(&security_warnings);

        if SecurityValidator::should_block_startup(&initial_config, &security_warnings) {
            error!(
                "🛑 Startup blocked due to critical security issues in production mode. \
                 Set PISOVEREIGN_ALLOW_INSECURE_CONFIG=true to override (not recommended)."
            );
            std::process::exit(1);
        }
    }

    info!(
        host = %initial_config.server.host,
        port = %initial_config.server.port,
        model = %initial_config.inference.default_model,
        "Configuration loaded"
    );

    // Security check: validate API keys are properly hashed
    // In release builds with plaintext keys, startup is blocked by SecurityValidator above
    // This provides an additional warning for development mode
    let plaintext_count = initial_config.security.count_plaintext_keys();
    if plaintext_count > 0 {
        warn!(
            count = plaintext_count,
            "⚠️ SECURITY WARNING: {} API key(s) are not properly hashed with Argon2. \
             Run 'pisovereign-cli migrate-keys' to convert them to secure hashes.",
            plaintext_count
        );
    }

    // Initialize OpenTelemetry if configured
    let _telemetry_guard = initial_config
        .telemetry
        .as_ref()
        .filter(|c| c.enabled)
        .and_then(|otel_config| {
            let telemetry_config = TelemetryConfig {
                enabled: true,
                endpoint: otel_config.otlp_endpoint.clone(),
                service_name: "pisovereign".to_string(),
                sampling_ratio: otel_config.sample_ratio.unwrap_or(1.0),
                ..TelemetryConfig::default()
            };
            match init_telemetry(&telemetry_config) {
                Ok(guard) => {
                    info!(
                        endpoint = %telemetry_config.endpoint,
                        "📊 OpenTelemetry initialized"
                    );
                    Some(guard)
                },
                Err(e) => {
                    warn!(error = %e, "⚠️ Failed to initialize telemetry, continuing without");
                    None
                },
            }
        });

    // Create reloadable config and spawn SIGHUP handler
    let reloadable_config =
        spawn_config_reload_handler(ReloadableConfig::new(initial_config.clone()));

    // Initialize inference adapter with degraded mode wrapper
    let hailo_adapter = HailoInferenceAdapter::new(initial_config.inference.clone())
        .map_err(|e| anyhow::anyhow!("Failed to initialize inference: {e}"))?;

    // Configure degraded mode from config or use defaults
    let degraded_config =
        initial_config
            .degraded_mode
            .as_ref()
            .map_or_else(DegradedModeConfig::default, |dm| DegradedModeConfig {
                enabled: dm.enabled,
                unavailable_message: dm.unavailable_message.clone(),
                retry_cooldown_secs: dm.retry_cooldown_secs,
                failure_threshold: dm.failure_threshold,
                success_threshold: dm.success_threshold,
            });

    let degraded_adapter = DegradedInferenceAdapter::new(Arc::new(hailo_adapter), degraded_config);
    info!("🛡️ Degraded mode adapter initialized");

    let inference: Arc<dyn InferencePort> = Arc::new(degraded_adapter);

    // Initialize optional weather adapter
    let weather_port: Option<Arc<dyn WeatherPort>> =
        initial_config
            .weather
            .as_ref()
            .and_then(|_| match WeatherAdapter::new() {
                Ok(adapter) => {
                    info!("🌤️ Weather adapter initialized");
                    Some(Arc::new(adapter.with_circuit_breaker()) as Arc<dyn WeatherPort>)
                },
                Err(e) => {
                    warn!(error = %e, "⚠️ Failed to initialize weather adapter");
                    None
                },
            });

    // Initialize optional CalDAV calendar adapter
    let calendar_port: Option<Arc<dyn CalendarPort>> =
        initial_config.caldav.as_ref().and_then(|config| {
            match CalDavCalendarAdapter::new(config.to_caldav_config()) {
                Ok(adapter) => {
                    info!("📅 CalDAV calendar adapter initialized");
                    Some(Arc::new(adapter.with_circuit_breaker()) as Arc<dyn CalendarPort>)
                },
                Err(e) => {
                    warn!(error = %e, "⚠️ Failed to initialize CalDAV adapter");
                    None
                },
            }
        });

    // Initialize optional Proton email adapter
    let email_port: Option<Arc<dyn EmailPort>> = initial_config.proton.as_ref().map(|config| {
        let adapter = ProtonEmailAdapter::new(config.to_proton_config()).with_circuit_breaker();
        info!("📧 Proton email adapter initialized");
        Arc::new(adapter) as Arc<dyn EmailPort>
    });

    // Initialize database connection pool
    let (approval_service, conversation_store, database_health_port) =
        match create_pool(&initial_config.database) {
            Ok(pool) => {
                let pool = Arc::new(pool);
                let approval_queue = Arc::new(SqliteApprovalQueue::new(Arc::clone(&pool)));
                let audit_log = Arc::new(SqliteAuditLog::new(Arc::clone(&pool)));
                let approval_service = ApprovalService::new(approval_queue, audit_log);
                let conversation_store: Arc<dyn ConversationStore> =
                    Arc::new(SqliteConversationStore::new(Arc::clone(&pool)));
                let database_health: Arc<dyn DatabaseHealthPort> =
                    Arc::new(SqliteDatabaseHealth::new(Arc::clone(&pool)));
                info!("✅ Database initialized with conversation and approval stores");
                (
                    Some(Arc::new(approval_service)),
                    Some(conversation_store),
                    Some(database_health),
                )
            },
            Err(e) => {
                warn!(
                    error = %e,
                    "⚠️ Failed to initialize database, persistence features disabled"
                );
                (None, None, None)
            },
        };

    // Initialize services
    let chat_service = conversation_store.as_ref().map_or_else(
        || {
            warn!("⚠️ ChatService running without conversation persistence");
            ChatService::with_system_prompt(Arc::clone(&inference), SYSTEM_PROMPT)
        },
        |store| ChatService::with_all(Arc::clone(&inference), Arc::clone(store), SYSTEM_PROMPT),
    );

    let agent_service = AgentService::new(Arc::clone(&inference));

    // Initialize metrics collector
    let metrics = Arc::new(MetricsCollector::new());

    // Build HealthService with all available ports
    let mut health_service = HealthService::new(Arc::clone(&inference));
    if let Some(ref database) = database_health_port {
        health_service = health_service.with_database(Arc::clone(database));
    }
    if let Some(ref email) = email_port {
        health_service = health_service.with_email(Arc::clone(email));
    }
    if let Some(ref calendar) = calendar_port {
        health_service = health_service.with_calendar(Arc::clone(calendar));
    }
    if let Some(ref weather) = weather_port {
        health_service = health_service.with_weather(Arc::clone(weather));
    }
    info!("❤️ HealthService initialized with all available ports");

    // Create app state with reloadable config
    let state = AppState {
        chat_service: Arc::new(chat_service),
        agent_service: Arc::new(agent_service),
        approval_service,
        health_service: Some(Arc::new(health_service)),
        voice_message_service: None, // VoiceMessageService not yet configured in main
        config: reloadable_config,
        metrics,
    };

    // Build router
    let app = routes::create_router(state);

    // Configure CORS layer
    let cors_layer = if initial_config.server.allowed_origins.is_empty() {
        // Development mode: allow all origins
        warn!(
            "⚠️ CORS configured to allow ANY origin - not recommended for production. \
             Set 'server.allowed_origins' in config.toml to restrict access."
        );
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

    // Spawn rate limiter cleanup task
    let rate_limiter_state = rate_limiter.state();
    let _cleanup_handle = spawn_cleanup_task(
        rate_limiter_state,
        Duration::from_secs(initial_config.security.rate_limit_cleanup_interval_secs),
        Duration::from_secs(initial_config.security.rate_limit_cleanup_max_age_secs),
    );

    // Configure API key auth from hashed API keys
    let auth_layer = if initial_config.security.api_keys.is_empty() {
        ApiKeyAuthLayer::disabled()
    } else {
        ApiKeyAuthLayer::from_api_keys(initial_config.security.api_keys.clone())
    };

    // Add middleware (order matters: first added = outermost)
    // Request ID layer is outermost to ensure all logs have the correlation ID
    let app = app
        .layer(RequestIdLayer::new())
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
