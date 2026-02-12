//! Resilience configurations: Telemetry, Retry, Degraded Mode, Health checks.

use serde::{Deserialize, Serialize};

use super::default_true;

// ==============================
// Telemetry Configuration
// ==============================

/// Telemetry configuration for OpenTelemetry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TelemetryAppConfig {
    /// Enable telemetry
    #[serde(default)]
    pub enabled: bool,

    /// OTLP endpoint URL
    #[serde(default = "default_otlp_endpoint")]
    pub otlp_endpoint: String,

    /// Sample ratio (0.0 to 1.0)
    #[serde(default)]
    pub sample_ratio: Option<f64>,
}

fn default_otlp_endpoint() -> String {
    "http://localhost:4317".to_string()
}

impl Default for TelemetryAppConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            otlp_endpoint: default_otlp_endpoint(),
            sample_ratio: Some(1.0),
        }
    }
}

// ==============================
// Retry Configuration
// ==============================

/// Retry configuration for external service calls
///
/// Configures exponential backoff retry behavior for all external services.
/// Individual services can override these defaults.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetryAppConfig {
    /// Initial delay before first retry in milliseconds (default: 100ms)
    #[serde(default = "default_retry_initial_delay")]
    pub initial_delay_ms: u64,

    /// Maximum delay between retries in milliseconds (default: 10000ms = 10s)
    #[serde(default = "default_retry_max_delay")]
    pub max_delay_ms: u64,

    /// Multiplier for exponential backoff (default: 2.0)
    #[serde(default = "default_retry_multiplier")]
    pub multiplier: f64,

    /// Maximum number of retry attempts (default: 3)
    #[serde(default = "default_retry_max_retries")]
    pub max_retries: u32,
}

const fn default_retry_initial_delay() -> u64 {
    100
}

const fn default_retry_max_delay() -> u64 {
    10_000
}

const fn default_retry_multiplier() -> f64 {
    2.0
}

const fn default_retry_max_retries() -> u32 {
    3
}

impl Default for RetryAppConfig {
    fn default() -> Self {
        Self {
            initial_delay_ms: default_retry_initial_delay(),
            max_delay_ms: default_retry_max_delay(),
            multiplier: default_retry_multiplier(),
            max_retries: default_retry_max_retries(),
        }
    }
}

impl RetryAppConfig {
    /// Convert to `retry::RetryConfig` for use with retry operations
    #[must_use]
    pub const fn to_retry_config(&self) -> crate::retry::RetryConfig {
        crate::retry::RetryConfig::new(
            self.initial_delay_ms,
            self.max_delay_ms,
            self.multiplier,
            self.max_retries,
        )
    }
}

// ==============================
// Degraded Mode Configuration
// ==============================

/// Degraded mode configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DegradedModeAppConfig {
    /// Enable degraded mode fallback
    #[serde(default = "default_true")]
    pub enabled: bool,

    /// Message to return when service is unavailable
    #[serde(default = "default_unavailable_message")]
    pub unavailable_message: String,

    /// Cooldown before retrying primary backend (seconds)
    #[serde(default = "default_retry_cooldown")]
    pub retry_cooldown_secs: u64,

    /// Number of failures before entering degraded mode
    #[serde(default = "default_failure_threshold")]
    pub failure_threshold: u32,

    /// Number of successes to exit degraded mode
    #[serde(default = "default_success_threshold")]
    pub success_threshold: u32,
}

fn default_unavailable_message() -> String {
    "I'm currently experiencing technical difficulties. Please try again in a moment.".to_string()
}

const fn default_retry_cooldown() -> u64 {
    30
}

const fn default_failure_threshold() -> u32 {
    3
}

const fn default_success_threshold() -> u32 {
    2
}

impl Default for DegradedModeAppConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            unavailable_message: default_unavailable_message(),
            retry_cooldown_secs: default_retry_cooldown(),
            failure_threshold: default_failure_threshold(),
            success_threshold: default_success_threshold(),
        }
    }
}

// ==============================
// Health Check Configuration
// ==============================

/// Health check configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthAppConfig {
    /// Global timeout for all health checks in seconds
    #[serde(default = "default_health_global_timeout")]
    pub global_timeout_secs: u64,

    /// Inference engine health check timeout in seconds (overrides global)
    pub inference_timeout_secs: Option<u64>,

    /// Email service health check timeout in seconds (overrides global)
    pub email_timeout_secs: Option<u64>,

    /// Calendar service health check timeout in seconds (overrides global)
    pub calendar_timeout_secs: Option<u64>,

    /// Weather service health check timeout in seconds (overrides global)
    pub weather_timeout_secs: Option<u64>,
}

const fn default_health_global_timeout() -> u64 {
    5
}

impl Default for HealthAppConfig {
    fn default() -> Self {
        Self {
            global_timeout_secs: default_health_global_timeout(),
            inference_timeout_secs: None,
            email_timeout_secs: None,
            calendar_timeout_secs: None,
            weather_timeout_secs: None,
        }
    }
}

impl HealthAppConfig {
    /// Convert to `application::HealthConfig`
    #[must_use]
    pub fn to_health_config(&self) -> application::HealthConfig {
        use std::collections::HashMap;

        let mut service_timeouts = HashMap::new();

        if let Some(t) = self.inference_timeout_secs {
            service_timeouts.insert("inference".to_string(), t);
        }
        if let Some(t) = self.email_timeout_secs {
            service_timeouts.insert("email".to_string(), t);
        }
        if let Some(t) = self.calendar_timeout_secs {
            service_timeouts.insert("calendar".to_string(), t);
        }
        if let Some(t) = self.weather_timeout_secs {
            service_timeouts.insert("weather".to_string(), t);
        }

        application::HealthConfig {
            global_timeout_secs: self.global_timeout_secs,
            service_timeouts,
        }
    }
}
