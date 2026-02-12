//! PiSovereign HTTP Server
//!
//! Main entry point for the HTTP API server.

use std::{sync::Arc, time::Duration};

use application::{
    AgentService, ApprovalService, ChatService, HealthService,
    ports::{
        CalendarPort, ConversationStore, DatabaseHealthPort, EmailPort, InferencePort,
        MessengerPort, ReminderPort, SuspiciousActivityPort, TransitPort, WeatherPort,
    },
    services::PromptSanitizer,
};
use infrastructure::{
    AppConfig, MessengerSelection, OllamaInferenceAdapter, SecurityValidator,
    adapters::{
        CalDavCalendarAdapter, DegradedInferenceAdapter, DegradedModeConfig,
        InMemorySuspiciousActivityTracker, ProtonEmailAdapter, SignalMessengerAdapter,
        TransitAdapter, WeatherAdapter, WhatsAppMessengerAdapter,
    },
    persistence::{
        AsyncConversationStore, AsyncDatabase, AsyncDatabaseConfig, SqliteApprovalQueue,
        SqliteAuditLog, SqliteDatabaseHealth, SqliteReminderStore,
    },
    telemetry::{TelemetryConfig, init_telemetry},
};
use integration_signal::{SignalClient, SignalClientConfig};
use integration_whatsapp::WhatsAppClientConfig;
use presentation_http::{
    ApiKeyAuthLayer, RateLimiterConfig, RateLimiterLayer, ReloadableConfig, RequestIdLayer,
    SecurityHeadersLayer, handlers::metrics::MetricsCollector, routes, spawn_cleanup_task,
    spawn_config_reload_handler, spawn_conversation_cleanup_task, state::AppState,
};
use secrecy::ExposeSecret;
use std::net::SocketAddr;
use tokio::{net::TcpListener, signal};
use tower_http::{
    cors::{Any, CorsLayer},
    limit::RequestBodyLimitLayer,
    trace::TraceLayer,
};
use tracing::{debug, error, info, warn};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

/// System prompt for the AI assistant
const SYSTEM_PROMPT: &str = "You are PiSovereign, a helpful AI assistant. On Raspberry Pi \
    you run on the Hailo-10H NPU, on Mac you use Metal GPU acceleration. You are friendly, \
    precise, and help with everyday tasks like email, calendar, and information lookup.";

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

    // Configure error response detail exposure based on environment
    // In production, we hide implementation details to prevent information leakage
    let is_production = matches!(
        initial_config.environment,
        Some(infrastructure::config::Environment::Production)
    );
    presentation_http::error::set_expose_internal_errors(!is_production);
    if is_production {
        info!("🔒 Production mode: error details will be sanitized");
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
    let ollama_adapter = OllamaInferenceAdapter::new(initial_config.inference.clone())
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

    let degraded_adapter = DegradedInferenceAdapter::new(Arc::new(ollama_adapter), degraded_config);
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

    // Initialize optional transit adapter
    let transit_port: Option<Arc<dyn TransitPort>> =
        initial_config.transit.as_ref().and_then(|config| {
            let transit_config = config.to_transit_config();
            let geocoding_config = integration_transit::NominatimConfig::default();

            match (
                integration_transit::HafasTransitClient::new(&transit_config),
                integration_transit::NominatimGeocodingClient::new(&geocoding_config),
            ) {
                (Ok(transit_client), Ok(geocoding_client)) => {
                    let adapter = TransitAdapter::new(transit_client, geocoding_client)
                        .with_circuit_breaker();
                    info!("🚇 Transit adapter initialized");
                    Some(Arc::new(adapter) as Arc<dyn TransitPort>)
                },
                (Err(e), _) => {
                    warn!(error = %e, "⚠️ Failed to initialize transit client");
                    None
                },
                (_, Err(e)) => {
                    warn!(error = %e, "⚠️ Failed to initialize geocoding client");
                    None
                },
            }
        });

    // Get home location from transit config for route calculations
    let home_location = initial_config
        .transit
        .as_ref()
        .and_then(|t| t.home_location.as_ref())
        .and_then(infrastructure::config::GeoLocationConfig::to_geo_location);

    // Initialize async database
    let (approval_service, conversation_store, database_health_port, reminder_port) = {
        let db_config = AsyncDatabaseConfig::file(&initial_config.database.path);
        match AsyncDatabase::new(&db_config).await {
            Ok(db) => match db.migrate().await {
                Ok(()) => {
                    let pool = db.pool().clone();
                    let approval_queue = Arc::new(SqliteApprovalQueue::new(pool.clone()));
                    let audit_log = Arc::new(SqliteAuditLog::new(pool.clone()));
                    let approval_service = ApprovalService::new(approval_queue, audit_log);
                    let conversation_store: Arc<dyn ConversationStore> =
                        Arc::new(AsyncConversationStore::new(pool.clone()));
                    let database_health: Arc<dyn DatabaseHealthPort> =
                        Arc::new(SqliteDatabaseHealth::new(pool.clone()));
                    let reminder_store: Arc<dyn ReminderPort> =
                        Arc::new(SqliteReminderStore::new(pool));
                    info!(
                        "✅ Database initialized with conversation, approval, and reminder stores"
                    );
                    (
                        Some(Arc::new(approval_service)),
                        Some(conversation_store),
                        Some(database_health),
                        Some(reminder_store),
                    )
                },
                Err(e) => {
                    warn!(
                        error = %e,
                        "⚠️ Failed to run database migrations, persistence features disabled"
                    );
                    (None, None, None, None)
                },
            },
            Err(e) => {
                warn!(
                    error = %e,
                    "⚠️ Failed to initialize database, persistence features disabled"
                );
                (None, None, None, None)
            },
        }
    };

    // Initialize services
    let chat_service = conversation_store.as_ref().map_or_else(
        || {
            warn!("⚠️ ChatService running without conversation persistence");
            ChatService::with_system_prompt(Arc::clone(&inference), SYSTEM_PROMPT)
        },
        |store| ChatService::with_all(Arc::clone(&inference), Arc::clone(store), SYSTEM_PROMPT),
    );

    // Build agent service with optional reminder and transit support
    let mut agent_service = AgentService::new(Arc::clone(&inference));
    if let Some(ref reminder) = reminder_port {
        agent_service = agent_service.with_reminder_service(Arc::clone(reminder));
        info!("📋 AgentService configured with reminder support");
    }
    if let Some(ref transit) = transit_port {
        agent_service = agent_service.with_transit_service(Arc::clone(transit));
        info!("🚇 AgentService configured with transit support");
    }
    if let Some(location) = home_location {
        agent_service = agent_service.with_home_location(location);
        info!("🏠 AgentService configured with home location");
    }

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

    // Initialize messenger adapter based on configuration
    let (messenger_adapter, signal_client): (
        Option<Arc<dyn MessengerPort>>,
        Option<Arc<SignalClient>>,
    ) = match initial_config.messenger {
        MessengerSelection::WhatsApp => {
            // Initialize WhatsApp messenger adapter
            if let (Some(access_token), Some(phone_number_id)) = (
                initial_config.whatsapp.access_token.as_ref(),
                initial_config.whatsapp.phone_number_id.as_ref(),
            ) {
                let client_config = WhatsAppClientConfig {
                    access_token: access_token.expose_secret().to_string(),
                    phone_number_id: phone_number_id.clone(),
                    app_secret: initial_config
                        .whatsapp
                        .app_secret
                        .as_ref()
                        .map(|s| s.expose_secret().to_string())
                        .unwrap_or_default(),
                    verify_token: initial_config
                        .whatsapp
                        .verify_token
                        .clone()
                        .unwrap_or_default(),
                    signature_required: initial_config.whatsapp.signature_required,
                    api_version: initial_config.whatsapp.api_version.clone(),
                };

                match WhatsAppMessengerAdapter::with_whitelist(
                    client_config,
                    initial_config.whatsapp.whitelist.clone(),
                ) {
                    Ok(adapter) => {
                        info!("📱 WhatsApp messenger adapter initialized");
                        (Some(Arc::new(adapter) as Arc<dyn MessengerPort>), None)
                    },
                    Err(e) => {
                        warn!(error = %e, "⚠️ Failed to initialize WhatsApp adapter");
                        (None, None)
                    },
                }
            } else {
                warn!(
                    "⚠️ WhatsApp selected but not fully configured (missing access_token or phone_number_id)"
                );
                (None, None)
            }
        },
        MessengerSelection::Signal => {
            // Initialize Signal messenger adapter
            if initial_config.signal.phone_number.is_empty() {
                warn!("⚠️ Signal selected but not configured (missing phone_number)");
                (None, None)
            } else {
                let client_config = SignalClientConfig {
                    phone_number: initial_config.signal.phone_number.clone(),
                    socket_path: initial_config.signal.socket_path.clone(),
                    data_path: initial_config.signal.data_path.clone(),
                    timeout_ms: initial_config.signal.timeout_ms,
                };

                let signal_client = Arc::new(SignalClient::with_whitelist(
                    client_config.clone(),
                    initial_config.signal.whitelist.clone(),
                ));

                let adapter = SignalMessengerAdapter::with_whitelist(
                    client_config,
                    initial_config.signal.whitelist.clone(),
                );
                info!("📱 Signal messenger adapter initialized");
                (
                    Some(Arc::new(adapter) as Arc<dyn MessengerPort>),
                    Some(signal_client),
                )
            }
        },
        MessengerSelection::None => {
            info!("📵 No messenger integration configured");
            (None, None)
        },
    };

    // Initialize prompt security services if enabled
    let (prompt_sanitizer, suspicious_activity_tracker): (
        Option<Arc<PromptSanitizer>>,
        Option<Arc<dyn SuspiciousActivityPort>>,
    ) = if initial_config.prompt_security.enabled {
        let sanitizer = PromptSanitizer::with_config(
            initial_config.prompt_security.to_prompt_security_config(),
        );
        let tracker = InMemorySuspiciousActivityTracker::new(
            initial_config
                .prompt_security
                .to_suspicious_activity_config(),
        );
        info!(
            sensitivity = %initial_config.prompt_security.sensitivity,
            "🛡️ Prompt security enabled"
        );
        (
            Some(Arc::new(sanitizer)),
            Some(Arc::new(tracker) as Arc<dyn SuspiciousActivityPort>),
        )
    } else {
        info!("⚠️ Prompt security disabled");
        (None, None)
    };

    // Spawn conversation cleanup task if retention is configured
    let _conversation_cleanup_handle = if let Some(ref store) = conversation_store {
        // Get retention days from the active messenger's persistence config
        let retention_days = match initial_config.messenger {
            MessengerSelection::WhatsApp => initial_config.whatsapp.persistence.retention_days,
            MessengerSelection::Signal => initial_config.signal.persistence.retention_days,
            MessengerSelection::None => None,
        };

        retention_days.map_or_else(
            || {
                debug!("Conversation retention not configured, cleanup task disabled");
                None
            },
            |days| {
                info!(
                    retention_days = days,
                    "🗑️ Conversation retention cleanup enabled"
                );
                Some(spawn_conversation_cleanup_task(
                    Arc::clone(store),
                    days,
                    None, // Use default 1-hour interval
                ))
            },
        )
    } else {
        None
    };

    // Create app state with reloadable config
    let state = AppState {
        chat_service: Arc::new(chat_service),
        agent_service: Arc::new(agent_service),
        approval_service,
        health_service: Some(Arc::new(health_service)),
        voice_message_service: None, // VoiceMessageService not yet configured in main
        config: reloadable_config,
        metrics,
        messenger_adapter,
        signal_client,
        prompt_sanitizer,
        suspicious_activity_tracker,
        conversation_store,
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

    // Configure rate limiter with trusted proxy support
    let rate_limiter = RateLimiterLayer::new(&RateLimiterConfig {
        enabled: initial_config.security.rate_limit_enabled,
        requests_per_minute: initial_config.security.rate_limit_rpm,
        trusted_proxies: initial_config.security.trusted_proxies.clone(),
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
    // Body limit is applied early to reject oversized requests fast
    // Security headers is innermost to ensure they're always added
    let app = app
        .layer(RequestIdLayer::new())
        .layer(TraceLayer::new_for_http())
        .layer(cors_layer)
        .layer(RequestBodyLimitLayer::new(
            initial_config.server.max_body_size_json_bytes,
        ))
        .layer(rate_limiter)
        .layer(auth_layer)
        .layer(SecurityHeadersLayer::new());

    info!(
        max_body_size_bytes = initial_config.server.max_body_size_json_bytes,
        "📦 Request body size limit enabled"
    );
    info!("🔒 Security headers middleware enabled");

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

    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
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
