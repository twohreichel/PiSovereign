//! OpenTelemetry initialization and configuration
//!
//! Provides tracing pipeline setup for exporting traces to Tempo or other OTLP endpoints.
//! Features graceful degradation when the collector is unavailable.

use std::time::Duration;

use opentelemetry::trace::TracerProvider;
use opentelemetry_otlp::WithExportConfig;
use opentelemetry_sdk::{
    Resource, runtime,
    trace::{Sampler, TracerProvider as SdkTracerProvider},
};
use serde::{Deserialize, Serialize};
use tracing::{info, warn};
use tracing_opentelemetry::OpenTelemetryLayer;
use tracing_subscriber::{EnvFilter, layer::SubscriberExt, util::SubscriberInitExt};

/// Configuration for telemetry/tracing
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TelemetryConfig {
    /// Whether OpenTelemetry export is enabled
    #[serde(default)]
    pub enabled: bool,

    /// OTLP endpoint URL (e.g., "http://localhost:4317" for gRPC)
    #[serde(default = "default_endpoint")]
    pub endpoint: String,

    /// Service name for traces
    #[serde(default = "default_service_name")]
    pub service_name: String,

    /// Sampling ratio (0.0 - 1.0)
    #[serde(default = "default_sampling_ratio")]
    pub sampling_ratio: f64,

    /// Batch export timeout in seconds
    #[serde(default = "default_export_timeout")]
    pub export_timeout_secs: u64,

    /// Maximum batch size for trace export
    #[serde(default = "default_max_batch_size")]
    pub max_batch_size: usize,

    /// Log level filter (e.g., "info", "debug", "pisovereign=debug,tower_http=info")
    #[serde(default = "default_log_filter")]
    pub log_filter: String,

    /// Whether to fall back to console-only logging if OTLP export fails
    ///
    /// When `true` (default), if the OpenTelemetry collector is unavailable,
    /// the application will continue with console-only logging instead of failing.
    /// Set to `false` to require a working collector in production environments.
    #[serde(default = "default_graceful_fallback")]
    pub graceful_fallback: bool,
}

const fn default_sampling_ratio() -> f64 {
    1.0
}

const fn default_export_timeout() -> u64 {
    30
}

const fn default_max_batch_size() -> usize {
    512
}

fn default_endpoint() -> String {
    "http://localhost:4317".to_string()
}

fn default_service_name() -> String {
    "pisovereign".to_string()
}

fn default_log_filter() -> String {
    "pisovereign=info,tower_http=info".to_string()
}

const fn default_graceful_fallback() -> bool {
    true
}

impl Default for TelemetryConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            endpoint: default_endpoint(),
            service_name: default_service_name(),
            sampling_ratio: default_sampling_ratio(),
            export_timeout_secs: default_export_timeout(),
            max_batch_size: default_max_batch_size(),
            log_filter: default_log_filter(),
            graceful_fallback: default_graceful_fallback(),
        }
    }
}

/// Guard that shuts down the tracer provider when dropped
pub struct TelemetryGuard {
    provider: Option<SdkTracerProvider>,
}

impl std::fmt::Debug for TelemetryGuard {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TelemetryGuard")
            .field("active", &self.provider.is_some())
            .finish_non_exhaustive()
    }
}

impl Drop for TelemetryGuard {
    fn drop(&mut self) {
        if let Some(provider) = self.provider.take() {
            if let Err(e) = provider.shutdown() {
                tracing::error!("Failed to shutdown tracer provider: {:?}", e);
            }
        }
    }
}

/// Initialize telemetry with the given configuration
///
/// Returns a guard that must be kept alive for the duration of the application.
/// When the guard is dropped, the tracer provider is shut down and pending
/// traces are flushed.
///
/// # Example
///
/// ```ignore
/// use infrastructure::telemetry::{TelemetryConfig, init_telemetry};
///
/// #[tokio::main]
/// async fn main() {
///     let config = TelemetryConfig {
///         enabled: true,
///         endpoint: "http://localhost:4317".to_string(),
///         ..Default::default()
///     };
///     
///     let _guard = init_telemetry(&config).expect("Failed to initialize telemetry");
///     
///     // Application code...
/// }
/// ```
pub fn init_telemetry(config: &TelemetryConfig) -> Result<TelemetryGuard, TelemetryError> {
    let env_filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(&config.log_filter));

    let fmt_layer = tracing_subscriber::fmt::layer()
        .with_target(true)
        .with_thread_ids(true)
        .with_file(true)
        .with_line_number(true);

    if !config.enabled {
        // No OTLP export, just console logging
        tracing_subscriber::registry()
            .with(env_filter)
            .with(fmt_layer)
            .try_init()
            .map_err(|e| TelemetryError::Init(e.to_string()))?;

        info!("Telemetry initialized (OTLP disabled, console only)");
        return Ok(TelemetryGuard { provider: None });
    }

    // Try to build OTLP exporter - may fail if collector is unavailable
    let exporter_result = opentelemetry_otlp::SpanExporter::builder()
        .with_tonic()
        .with_endpoint(&config.endpoint)
        .with_timeout(Duration::from_secs(config.export_timeout_secs))
        .build();

    match exporter_result {
        Ok(exporter) => {
            // Build sampler
            let sampler = if (config.sampling_ratio - 1.0).abs() < f64::EPSILON {
                Sampler::AlwaysOn
            } else if config.sampling_ratio <= 0.0 {
                Sampler::AlwaysOff
            } else {
                Sampler::TraceIdRatioBased(config.sampling_ratio)
            };

            // Build resource with service name
            let resource = Resource::new(vec![opentelemetry::KeyValue::new(
                "service.name",
                config.service_name.clone(),
            )]);

            // Build tracer provider using new API
            let provider = SdkTracerProvider::builder()
                .with_batch_exporter(exporter, runtime::Tokio)
                .with_sampler(sampler)
                .with_resource(resource)
                .build();

            // Get tracer
            let tracer = provider.tracer(config.service_name.clone());

            // Create OpenTelemetry layer
            let otel_layer = OpenTelemetryLayer::new(tracer);

            // Initialize subscriber with OTLP export
            tracing_subscriber::registry()
                .with(env_filter)
                .with(fmt_layer)
                .with(otel_layer)
                .try_init()
                .map_err(|e: tracing_subscriber::util::TryInitError| {
                    TelemetryError::Init(e.to_string())
                })?;

            info!(
                endpoint = %config.endpoint,
                service = %config.service_name,
                sampling = %config.sampling_ratio,
                "Telemetry initialized with OTLP export"
            );

            Ok(TelemetryGuard {
                provider: Some(provider),
            })
        },
        Err(e) => {
            if config.graceful_fallback {
                // Fall back to console-only logging
                warn!(
                    endpoint = %config.endpoint,
                    error = %e,
                    "OTLP collector unavailable, falling back to console-only logging"
                );

                tracing_subscriber::registry()
                    .with(env_filter)
                    .with(fmt_layer)
                    .try_init()
                    .map_err(|e| TelemetryError::Init(e.to_string()))?;

                info!("Telemetry initialized (OTLP fallback to console)");
                Ok(TelemetryGuard { provider: None })
            } else {
                // Fail if collector is required
                Err(TelemetryError::Exporter(e.to_string()))
            }
        },
    }
}

/// Error type for telemetry initialization
#[derive(Debug, thiserror::Error)]
pub enum TelemetryError {
    /// Failed to initialize tracing subscriber
    #[error("Failed to initialize tracing: {0}")]
    Init(String),

    /// Failed to create OTLP exporter
    #[error("Failed to create OTLP exporter: {0}")]
    Exporter(String),

    /// Failed to build tracer provider
    #[error("Failed to build tracer provider: {0}")]
    Provider(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_default() {
        let config = TelemetryConfig::default();
        assert!(!config.enabled);
        assert_eq!(config.endpoint, "http://localhost:4317");
        assert_eq!(config.service_name, "pisovereign");
        assert!((config.sampling_ratio - 1.0).abs() < f64::EPSILON);
        assert_eq!(config.export_timeout_secs, 30);
        assert_eq!(config.max_batch_size, 512);
        assert!(config.graceful_fallback);
    }

    #[test]
    fn test_config_serialization() {
        let config = TelemetryConfig {
            enabled: true,
            endpoint: "http://tempo:4317".to_string(),
            service_name: "test-service".to_string(),
            sampling_ratio: 0.5,
            export_timeout_secs: 60,
            max_batch_size: 1024,
            log_filter: "debug".to_string(),
            graceful_fallback: false,
        };

        let json = serde_json::to_string(&config).unwrap();
        let parsed: TelemetryConfig = serde_json::from_str(&json).unwrap();

        assert!(parsed.enabled);
        assert_eq!(parsed.endpoint, "http://tempo:4317");
        assert_eq!(parsed.service_name, "test-service");
        assert!((parsed.sampling_ratio - 0.5).abs() < f64::EPSILON);
        assert_eq!(parsed.export_timeout_secs, 60);
        assert_eq!(parsed.max_batch_size, 1024);
        assert!(!parsed.graceful_fallback);
    }

    #[test]
    fn test_config_graceful_fallback_default() {
        // When graceful_fallback is not specified in JSON, it should default to true
        let json = r#"{"enabled": true, "endpoint": "http://tempo:4317"}"#;
        let parsed: TelemetryConfig = serde_json::from_str(json).unwrap();
        assert!(parsed.graceful_fallback);
    }

    #[test]
    fn test_telemetry_guard_default() {
        // TelemetryGuard with None should not panic on drop
        let guard = TelemetryGuard { provider: None };
        drop(guard);
    }
}
