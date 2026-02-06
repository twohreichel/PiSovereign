//! Health aggregation service
//!
//! Provides comprehensive health checks for all external services,
//! with configurable timeouts and individual service status reporting.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tokio::time::timeout;
use tracing::{debug, instrument, warn};

use crate::ports::{CalendarPort, EmailPort, InferencePort, WeatherPort};

/// Default global timeout for health checks in seconds
const DEFAULT_HEALTH_CHECK_TIMEOUT_SECS: u64 = 5;

/// Configuration for health check behavior
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthConfig {
    /// Global timeout for all health checks in seconds (default: 5)
    #[serde(default = "default_global_timeout")]
    pub global_timeout_secs: u64,

    /// Service-specific timeout overrides in seconds
    #[serde(default)]
    pub service_timeouts: HashMap<String, u64>,
}

const fn default_global_timeout() -> u64 {
    DEFAULT_HEALTH_CHECK_TIMEOUT_SECS
}

impl Default for HealthConfig {
    fn default() -> Self {
        Self {
            global_timeout_secs: default_global_timeout(),
            service_timeouts: HashMap::new(),
        }
    }
}

impl HealthConfig {
    /// Get the timeout for a specific service
    #[must_use]
    pub fn timeout_for_service(&self, service: &str) -> Duration {
        let secs = self
            .service_timeouts
            .get(service)
            .copied()
            .unwrap_or(self.global_timeout_secs);
        Duration::from_secs(secs)
    }
}

/// Status of an individual service
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceHealth {
    /// Whether the service is healthy
    pub healthy: bool,
    /// Optional additional information (e.g., model name, version)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub info: Option<String>,
    /// Response time in milliseconds (if check was performed)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response_time_ms: Option<u64>,
    /// Error message if unhealthy
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl ServiceHealth {
    /// Create a healthy status
    #[must_use]
    pub const fn healthy() -> Self {
        Self {
            healthy: true,
            info: None,
            response_time_ms: None,
            error: None,
        }
    }

    /// Create a healthy status with additional info
    #[must_use]
    pub fn healthy_with_info(info: impl Into<String>) -> Self {
        Self {
            healthy: true,
            info: Some(info.into()),
            response_time_ms: None,
            error: None,
        }
    }

    /// Create an unhealthy status
    #[must_use]
    pub fn unhealthy(error: impl Into<String>) -> Self {
        Self {
            healthy: false,
            info: None,
            response_time_ms: None,
            error: Some(error.into()),
        }
    }

    /// Create an unhealthy status due to timeout
    #[must_use]
    pub fn timeout() -> Self {
        Self {
            healthy: false,
            info: None,
            response_time_ms: None,
            error: Some("Health check timed out".to_string()),
        }
    }

    /// Create a status for an unconfigured/disabled service
    #[must_use]
    pub fn unconfigured() -> Self {
        Self {
            healthy: false,
            info: Some("Service not configured".to_string()),
            response_time_ms: None,
            error: None,
        }
    }

    /// Add response time to the status
    #[must_use]
    pub const fn with_response_time(mut self, ms: u64) -> Self {
        self.response_time_ms = Some(ms);
        self
    }
}

/// Comprehensive health report for all services
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthReport {
    /// Overall system health (true if all critical services are healthy)
    pub healthy: bool,
    /// Individual service statuses
    pub services: HashMap<String, ServiceHealth>,
    /// Timestamp of the health check
    pub checked_at: chrono::DateTime<chrono::Utc>,
}

impl HealthReport {
    /// Create a new health report
    #[must_use]
    pub fn new(services: HashMap<String, ServiceHealth>) -> Self {
        // System is healthy if all services are healthy
        let healthy = services.values().all(|s| s.healthy);

        Self {
            healthy,
            services,
            checked_at: chrono::Utc::now(),
        }
    }

    /// Get status of a specific service
    #[must_use]
    pub fn service_status(&self, name: &str) -> Option<&ServiceHealth> {
        self.services.get(name)
    }
}

/// Service for aggregating health checks across all external services
pub struct HealthService {
    config: HealthConfig,
    inference: Arc<dyn InferencePort>,
    email: Option<Arc<dyn EmailPort>>,
    calendar: Option<Arc<dyn CalendarPort>>,
    weather: Option<Arc<dyn WeatherPort>>,
}

impl std::fmt::Debug for HealthService {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HealthService")
            .field("config", &self.config)
            .field("inference", &"<InferencePort>")
            .field("email", &self.email.is_some())
            .field("calendar", &self.calendar.is_some())
            .field("weather", &self.weather.is_some())
            .finish()
    }
}

impl HealthService {
    /// Create a new health service with required inference port
    #[must_use]
    pub fn new(inference: Arc<dyn InferencePort>) -> Self {
        Self {
            config: HealthConfig::default(),
            inference,
            email: None,
            calendar: None,
            weather: None,
        }
    }

    /// Set the health check configuration
    #[must_use]
    pub fn with_config(mut self, config: HealthConfig) -> Self {
        self.config = config;
        self
    }

    /// Add email service for health checking
    #[must_use]
    pub fn with_email(mut self, email: Arc<dyn EmailPort>) -> Self {
        self.email = Some(email);
        self
    }

    /// Add calendar service for health checking
    #[must_use]
    pub fn with_calendar(mut self, calendar: Arc<dyn CalendarPort>) -> Self {
        self.calendar = Some(calendar);
        self
    }

    /// Add weather service for health checking
    #[must_use]
    pub fn with_weather(mut self, weather: Arc<dyn WeatherPort>) -> Self {
        self.weather = Some(weather);
        self
    }

    /// Check health of all configured services
    #[instrument(skip(self))]
    pub async fn check_all(&self) -> HealthReport {
        let mut services = HashMap::new();

        // Check inference (always required)
        services.insert("inference".to_string(), self.check_inference().await);

        // Check optional services
        services.insert("email".to_string(), self.check_email().await);

        services.insert("calendar".to_string(), self.check_calendar().await);

        services.insert("weather".to_string(), self.check_weather().await);

        HealthReport::new(services)
    }

    /// Check only the inference engine health
    #[instrument(skip(self))]
    pub async fn check_inference(&self) -> ServiceHealth {
        let timeout_duration = self.config.timeout_for_service("inference");
        let start = std::time::Instant::now();

        let result = timeout(timeout_duration, self.inference.is_healthy()).await;

        if let Ok(healthy) = result {
            // SAFETY: Response time in milliseconds will never exceed u64::MAX in practice.
            // A health check timeout of 5 seconds = 5000ms, well within u64 range.
            #[allow(clippy::cast_possible_truncation)]
            let response_time = start.elapsed().as_millis() as u64;
            if healthy {
                let model = self.inference.current_model();
                debug!(model = %model, response_time_ms = response_time, "Inference healthy");
                ServiceHealth::healthy_with_info(model).with_response_time(response_time)
            } else {
                warn!(response_time_ms = response_time, "Inference unhealthy");
                ServiceHealth::unhealthy("Inference engine reports unhealthy")
                    .with_response_time(response_time)
            }
        } else {
            warn!("Inference health check timed out");
            ServiceHealth::timeout()
        }
    }

    /// Check email service health
    #[instrument(skip(self))]
    pub async fn check_email(&self) -> ServiceHealth {
        let Some(ref email) = self.email else {
            return ServiceHealth::unconfigured();
        };

        let timeout_duration = self.config.timeout_for_service("email");
        let start = std::time::Instant::now();

        let result = timeout(timeout_duration, email.is_available()).await;

        if let Ok(available) = result {
            // SAFETY: Response time in milliseconds will never exceed u64::MAX in practice.
            #[allow(clippy::cast_possible_truncation)]
            let response_time = start.elapsed().as_millis() as u64;
            if available {
                debug!(response_time_ms = response_time, "Email service healthy");
                ServiceHealth::healthy().with_response_time(response_time)
            } else {
                warn!(response_time_ms = response_time, "Email service unhealthy");
                ServiceHealth::unhealthy("Email service unavailable")
                    .with_response_time(response_time)
            }
        } else {
            warn!("Email health check timed out");
            ServiceHealth::timeout()
        }
    }

    /// Check calendar service health
    #[instrument(skip(self))]
    pub async fn check_calendar(&self) -> ServiceHealth {
        let Some(ref calendar) = self.calendar else {
            return ServiceHealth::unconfigured();
        };

        let timeout_duration = self.config.timeout_for_service("calendar");
        let start = std::time::Instant::now();

        let result = timeout(timeout_duration, calendar.is_available()).await;

        if let Ok(available) = result {
            // SAFETY: Response time in milliseconds will never exceed u64::MAX in practice.
            #[allow(clippy::cast_possible_truncation)]
            let response_time = start.elapsed().as_millis() as u64;
            if available {
                debug!(response_time_ms = response_time, "Calendar service healthy");
                ServiceHealth::healthy().with_response_time(response_time)
            } else {
                warn!(
                    response_time_ms = response_time,
                    "Calendar service unhealthy"
                );
                ServiceHealth::unhealthy("Calendar service unavailable")
                    .with_response_time(response_time)
            }
        } else {
            warn!("Calendar health check timed out");
            ServiceHealth::timeout()
        }
    }

    /// Check weather service health
    #[instrument(skip(self))]
    pub async fn check_weather(&self) -> ServiceHealth {
        let Some(ref weather) = self.weather else {
            return ServiceHealth::unconfigured();
        };

        let timeout_duration = self.config.timeout_for_service("weather");
        let start = std::time::Instant::now();

        let result = timeout(timeout_duration, weather.is_available()).await;

        if let Ok(available) = result {
            // SAFETY: Response time in milliseconds will never exceed u64::MAX in practice.
            #[allow(clippy::cast_possible_truncation)]
            let response_time = start.elapsed().as_millis() as u64;
            if available {
                debug!(response_time_ms = response_time, "Weather service healthy");
                ServiceHealth::healthy().with_response_time(response_time)
            } else {
                warn!(
                    response_time_ms = response_time,
                    "Weather service unhealthy"
                );
                ServiceHealth::unhealthy("Weather service unavailable")
                    .with_response_time(response_time)
            }
        } else {
            warn!("Weather health check timed out");
            ServiceHealth::timeout()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::ApplicationError;
    use crate::ports::{InferenceResult, InferenceStream};
    use domain::Conversation;

    struct MockInference {
        healthy: bool,
        model: String,
    }

    #[async_trait::async_trait]
    impl InferencePort for MockInference {
        async fn generate(&self, _message: &str) -> Result<InferenceResult, ApplicationError> {
            unreachable!("generate should not be called in health service tests")
        }

        async fn generate_with_context(
            &self,
            _conversation: &Conversation,
        ) -> Result<InferenceResult, ApplicationError> {
            unreachable!("generate_with_context should not be called in health service tests")
        }

        async fn generate_with_system(
            &self,
            _system_prompt: &str,
            _message: &str,
        ) -> Result<InferenceResult, ApplicationError> {
            unreachable!("generate_with_system should not be called in health service tests")
        }

        async fn generate_stream(
            &self,
            _message: &str,
        ) -> Result<InferenceStream, ApplicationError> {
            unreachable!("generate_stream should not be called in health service tests")
        }

        async fn generate_stream_with_system(
            &self,
            _system_prompt: &str,
            _message: &str,
        ) -> Result<InferenceStream, ApplicationError> {
            unreachable!("generate_stream_with_system should not be called in health service tests")
        }

        async fn is_healthy(&self) -> bool {
            self.healthy
        }

        fn current_model(&self) -> String {
            self.model.clone()
        }

        async fn list_available_models(&self) -> Result<Vec<String>, ApplicationError> {
            Ok(vec![self.model.clone()])
        }

        async fn switch_model(&self, _model_name: &str) -> Result<(), ApplicationError> {
            Ok(())
        }
    }

    fn create_mock_inference(healthy: bool) -> Arc<dyn InferencePort> {
        Arc::new(MockInference {
            healthy,
            model: "test-model".to_string(),
        })
    }

    #[test]
    fn health_config_default() {
        let config = HealthConfig::default();
        assert_eq!(config.global_timeout_secs, 5);
        assert!(config.service_timeouts.is_empty());
    }

    #[test]
    fn health_config_timeout_for_service() {
        let mut config = HealthConfig::default();
        config.service_timeouts.insert("email".to_string(), 10);

        assert_eq!(config.timeout_for_service("email").as_secs(), 10);
        assert_eq!(config.timeout_for_service("weather").as_secs(), 5); // Falls back to global
    }

    #[test]
    fn service_health_healthy() {
        let status = ServiceHealth::healthy();
        assert!(status.healthy);
        assert!(status.error.is_none());
    }

    #[test]
    fn service_health_healthy_with_info() {
        let status = ServiceHealth::healthy_with_info("qwen-7b");
        assert!(status.healthy);
        assert_eq!(status.info, Some("qwen-7b".to_string()));
    }

    #[test]
    fn service_health_unhealthy() {
        let status = ServiceHealth::unhealthy("Connection refused");
        assert!(!status.healthy);
        assert_eq!(status.error, Some("Connection refused".to_string()));
    }

    #[test]
    fn service_health_timeout() {
        let status = ServiceHealth::timeout();
        assert!(!status.healthy);
        assert!(status.error.as_ref().unwrap().contains("timed out"));
    }

    #[test]
    fn service_health_unconfigured() {
        let status = ServiceHealth::unconfigured();
        assert!(!status.healthy);
        assert!(status.info.as_ref().unwrap().contains("not configured"));
    }

    #[test]
    fn service_health_with_response_time() {
        let status = ServiceHealth::healthy().with_response_time(42);
        assert_eq!(status.response_time_ms, Some(42));
    }

    #[test]
    fn health_report_all_healthy() {
        let mut services = HashMap::new();
        services.insert("inference".to_string(), ServiceHealth::healthy());
        services.insert("email".to_string(), ServiceHealth::healthy());

        let report = HealthReport::new(services);
        assert!(report.healthy);
    }

    #[test]
    fn health_report_one_unhealthy() {
        let mut services = HashMap::new();
        services.insert("inference".to_string(), ServiceHealth::healthy());
        services.insert("email".to_string(), ServiceHealth::unhealthy("down"));

        let report = HealthReport::new(services);
        assert!(!report.healthy);
    }

    #[test]
    fn health_report_service_status() {
        let mut services = HashMap::new();
        services.insert(
            "inference".to_string(),
            ServiceHealth::healthy_with_info("qwen"),
        );

        let report = HealthReport::new(services);
        let status = report.service_status("inference").unwrap();
        assert!(status.healthy);
        assert_eq!(status.info, Some("qwen".to_string()));
    }

    #[tokio::test]
    async fn health_service_check_inference_healthy() {
        let inference = create_mock_inference(true);
        let service = HealthService::new(inference);

        let status = service.check_inference().await;
        assert!(status.healthy);
        assert_eq!(status.info, Some("test-model".to_string()));
    }

    #[tokio::test]
    async fn health_service_check_inference_unhealthy() {
        let inference = create_mock_inference(false);
        let service = HealthService::new(inference);

        let status = service.check_inference().await;
        assert!(!status.healthy);
    }

    #[tokio::test]
    async fn health_service_check_email_unconfigured() {
        let inference = create_mock_inference(true);
        let service = HealthService::new(inference);

        let status = service.check_email().await;
        assert!(!status.healthy);
        assert!(status.info.as_ref().unwrap().contains("not configured"));
    }

    #[tokio::test]
    async fn health_service_check_all() {
        let inference = create_mock_inference(true);
        let service = HealthService::new(inference);

        let report = service.check_all().await;
        assert!(report.services.contains_key("inference"));
        assert!(report.services.contains_key("email"));
        assert!(report.services.contains_key("calendar"));
        assert!(report.services.contains_key("weather"));
    }

    #[test]
    fn health_service_debug() {
        let inference = create_mock_inference(true);
        let service = HealthService::new(inference);
        let debug = format!("{service:?}");
        assert!(debug.contains("HealthService"));
    }

    #[test]
    fn health_config_serialization() {
        let config = HealthConfig::default();
        let json = serde_json::to_string(&config).unwrap();
        assert!(json.contains("global_timeout_secs"));
    }

    #[test]
    fn health_config_deserialization() {
        let json = r#"{"global_timeout_secs":10,"service_timeouts":{"email":15}}"#;
        let config: HealthConfig = serde_json::from_str(json).unwrap();
        assert_eq!(config.global_timeout_secs, 10);
        assert_eq!(config.service_timeouts.get("email"), Some(&15));
    }

    #[test]
    fn health_report_serialization() {
        let mut services = HashMap::new();
        services.insert("test".to_string(), ServiceHealth::healthy());
        let report = HealthReport::new(services);

        let json = serde_json::to_string(&report).unwrap();
        assert!(json.contains("healthy"));
        assert!(json.contains("services"));
        assert!(json.contains("checked_at"));
    }

    #[test]
    fn service_health_serialization() {
        let status = ServiceHealth::healthy_with_info("model").with_response_time(100);
        let json = serde_json::to_string(&status).unwrap();
        assert!(json.contains("healthy"));
        assert!(json.contains("model"));
        assert!(json.contains("100"));
    }
}
