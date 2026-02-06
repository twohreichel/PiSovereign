//! Metrics and observability handlers
//!
//! Provides endpoints for collecting and exposing application metrics
//! in a structured format suitable for monitoring systems.

use std::{
    sync::atomic::{AtomicU64, Ordering},
    time::Instant,
};

use axum::{Json, extract::State};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::state::AppState;

/// Metrics response containing all application metrics
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct MetricsResponse {
    /// Application metadata
    pub app: AppMetrics,
    /// Request statistics
    pub requests: RequestMetrics,
    /// Inference engine metrics
    pub inference: InferenceMetrics,
    /// System metrics
    pub system: SystemMetrics,
}

/// Application metadata
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct AppMetrics {
    /// Application version
    pub version: String,
    /// Application name
    pub name: String,
    /// Uptime in seconds
    pub uptime_seconds: u64,
    /// Build timestamp or commit hash
    pub build_info: String,
}

/// Request statistics
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct RequestMetrics {
    /// Total requests received
    pub total_requests: u64,
    /// Successful requests (2xx)
    pub success_count: u64,
    /// Client errors (4xx)
    pub client_error_count: u64,
    /// Server errors (5xx)
    pub server_error_count: u64,
    /// Average response time in milliseconds
    pub avg_response_time_ms: f64,
    /// Current active requests
    pub active_requests: u64,
}

/// Inference engine metrics
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct InferenceMetrics {
    /// Total inference requests
    pub total_inferences: u64,
    /// Successful inferences
    pub successful_inferences: u64,
    /// Failed inferences
    pub failed_inferences: u64,
    /// Average inference time in milliseconds
    pub avg_inference_time_ms: f64,
    /// Total tokens generated
    pub total_tokens_generated: u64,
    /// Current model name
    pub current_model: String,
    /// Whether the inference engine is healthy
    pub healthy: bool,
}

/// System metrics
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct SystemMetrics {
    /// Rust allocator memory usage estimate (if available)
    pub memory_estimate_bytes: Option<u64>,
    /// Number of active async tasks (estimate)
    pub active_tasks: Option<u64>,
}

/// Atomic counters for request metrics
#[derive(Debug)]
pub struct MetricsCollector {
    /// Server start time
    start_time: Instant,
    /// Total requests
    total_requests: AtomicU64,
    /// Successful requests
    success_count: AtomicU64,
    /// Client errors
    client_error_count: AtomicU64,
    /// Server errors
    server_error_count: AtomicU64,
    /// Active requests
    active_requests: AtomicU64,
    /// Total response time in microseconds
    total_response_time_us: AtomicU64,
    /// Inference requests
    total_inferences: AtomicU64,
    /// Successful inferences
    successful_inferences: AtomicU64,
    /// Failed inferences
    failed_inferences: AtomicU64,
    /// Total inference time in microseconds
    total_inference_time_us: AtomicU64,
    /// Total tokens generated
    total_tokens_generated: AtomicU64,
}

impl Default for MetricsCollector {
    fn default() -> Self {
        Self::new()
    }
}

impl MetricsCollector {
    /// Create a new metrics collector
    #[must_use]
    pub fn new() -> Self {
        Self {
            start_time: Instant::now(),
            total_requests: AtomicU64::new(0),
            success_count: AtomicU64::new(0),
            client_error_count: AtomicU64::new(0),
            server_error_count: AtomicU64::new(0),
            active_requests: AtomicU64::new(0),
            total_response_time_us: AtomicU64::new(0),
            total_inferences: AtomicU64::new(0),
            successful_inferences: AtomicU64::new(0),
            failed_inferences: AtomicU64::new(0),
            total_inference_time_us: AtomicU64::new(0),
            total_tokens_generated: AtomicU64::new(0),
        }
    }

    /// Record start of a request
    pub fn request_start(&self) {
        self.total_requests.fetch_add(1, Ordering::Relaxed);
        self.active_requests.fetch_add(1, Ordering::Relaxed);
    }

    /// Record end of a request
    pub fn request_end(&self, response_time_us: u64, status_code: u16) {
        self.active_requests.fetch_sub(1, Ordering::Relaxed);
        self.total_response_time_us
            .fetch_add(response_time_us, Ordering::Relaxed);

        match status_code {
            200..=299 => {
                self.success_count.fetch_add(1, Ordering::Relaxed);
            },
            400..=499 => {
                self.client_error_count.fetch_add(1, Ordering::Relaxed);
            },
            500..=599 => {
                self.server_error_count.fetch_add(1, Ordering::Relaxed);
            },
            _ => {},
        }
    }

    /// Record an inference operation
    pub fn record_inference(&self, success: bool, duration_us: u64, tokens: u64) {
        self.total_inferences.fetch_add(1, Ordering::Relaxed);
        self.total_inference_time_us
            .fetch_add(duration_us, Ordering::Relaxed);
        self.total_tokens_generated
            .fetch_add(tokens, Ordering::Relaxed);

        if success {
            self.successful_inferences.fetch_add(1, Ordering::Relaxed);
        } else {
            self.failed_inferences.fetch_add(1, Ordering::Relaxed);
        }
    }

    /// Get uptime in seconds
    #[must_use]
    pub fn uptime_seconds(&self) -> u64 {
        self.start_time.elapsed().as_secs()
    }

    /// Get request metrics
    #[must_use]
    pub fn request_metrics(&self) -> RequestMetrics {
        let total = self.total_requests.load(Ordering::Relaxed);
        let total_time = self.total_response_time_us.load(Ordering::Relaxed);

        RequestMetrics {
            total_requests: total,
            success_count: self.success_count.load(Ordering::Relaxed),
            client_error_count: self.client_error_count.load(Ordering::Relaxed),
            server_error_count: self.server_error_count.load(Ordering::Relaxed),
            #[allow(clippy::cast_precision_loss)]
            avg_response_time_ms: if total > 0 {
                (total_time as f64) / (total as f64) / 1000.0
            } else {
                0.0
            },
            active_requests: self.active_requests.load(Ordering::Relaxed),
        }
    }

    /// Get inference metrics
    #[must_use]
    pub fn inference_metrics(&self, current_model: String, healthy: bool) -> InferenceMetrics {
        let total = self.total_inferences.load(Ordering::Relaxed);
        let total_time = self.total_inference_time_us.load(Ordering::Relaxed);

        InferenceMetrics {
            total_inferences: total,
            successful_inferences: self.successful_inferences.load(Ordering::Relaxed),
            failed_inferences: self.failed_inferences.load(Ordering::Relaxed),
            #[allow(clippy::cast_precision_loss)]
            avg_inference_time_ms: if total > 0 {
                (total_time as f64) / (total as f64) / 1000.0
            } else {
                0.0
            },
            total_tokens_generated: self.total_tokens_generated.load(Ordering::Relaxed),
            current_model,
            healthy,
        }
    }
}

/// Get metrics endpoint
#[utoipa::path(
    get,
    path = "/metrics",
    tag = "metrics",
    responses(
        (status = 200, description = "Application metrics", body = MetricsResponse)
    )
)]
pub async fn get_metrics(State(state): State<AppState>) -> Json<MetricsResponse> {
    let inference_healthy = state.chat_service.is_healthy().await;
    let current_model = state.chat_service.current_model();

    let metrics = state.metrics.as_ref();

    Json(MetricsResponse {
        app: AppMetrics {
            version: env!("CARGO_PKG_VERSION").to_string(),
            name: env!("CARGO_PKG_NAME").to_string(),
            uptime_seconds: metrics.uptime_seconds(),
            build_info: option_env!("GIT_HASH").unwrap_or("unknown").to_string(),
        },
        requests: metrics.request_metrics(),
        inference: metrics.inference_metrics(current_model, inference_healthy),
        system: SystemMetrics {
            memory_estimate_bytes: None, // Would require jemalloc or similar
            active_tasks: None,          // Would require tokio runtime handle
        },
    })
}

/// Prometheus-style metrics endpoint
#[utoipa::path(
    get,
    path = "/metrics/prometheus",
    tag = "metrics",
    responses(
        (status = 200, description = "Prometheus metrics", content_type = "text/plain")
    )
)]
pub async fn get_metrics_prometheus(State(state): State<AppState>) -> String {
    let metrics = state.metrics.as_ref();
    let request_metrics = metrics.request_metrics();

    let inference_healthy = state.chat_service.is_healthy().await;
    let inference_metrics =
        metrics.inference_metrics(state.chat_service.current_model(), inference_healthy);

    let mut output = String::new();

    // Application metrics
    output.push_str(&format!(
        "# HELP app_uptime_seconds Application uptime in seconds\n\
         # TYPE app_uptime_seconds counter\n\
         app_uptime_seconds {}\n\n",
        metrics.uptime_seconds()
    ));

    // Request metrics
    output.push_str(&format!(
        "# HELP http_requests_total Total HTTP requests\n\
         # TYPE http_requests_total counter\n\
         http_requests_total {}\n\n",
        request_metrics.total_requests
    ));

    output.push_str(&format!(
        "# HELP http_requests_success_total Successful HTTP requests\n\
         # TYPE http_requests_success_total counter\n\
         http_requests_success_total {}\n\n",
        request_metrics.success_count
    ));

    output.push_str(&format!(
        "# HELP http_requests_client_error_total Client error HTTP requests\n\
         # TYPE http_requests_client_error_total counter\n\
         http_requests_client_error_total {}\n\n",
        request_metrics.client_error_count
    ));

    output.push_str(&format!(
        "# HELP http_requests_server_error_total Server error HTTP requests\n\
         # TYPE http_requests_server_error_total counter\n\
         http_requests_server_error_total {}\n\n",
        request_metrics.server_error_count
    ));

    output.push_str(&format!(
        "# HELP http_requests_active Current active HTTP requests\n\
         # TYPE http_requests_active gauge\n\
         http_requests_active {}\n\n",
        request_metrics.active_requests
    ));

    output.push_str(&format!(
        "# HELP http_response_time_avg_ms Average response time in milliseconds\n\
         # TYPE http_response_time_avg_ms gauge\n\
         http_response_time_avg_ms {:.2}\n\n",
        request_metrics.avg_response_time_ms
    ));

    // Inference metrics
    output.push_str(&format!(
        "# HELP inference_requests_total Total inference requests\n\
         # TYPE inference_requests_total counter\n\
         inference_requests_total {}\n\n",
        inference_metrics.total_inferences
    ));

    output.push_str(&format!(
        "# HELP inference_requests_success_total Successful inference requests\n\
         # TYPE inference_requests_success_total counter\n\
         inference_requests_success_total {}\n\n",
        inference_metrics.successful_inferences
    ));

    output.push_str(&format!(
        "# HELP inference_requests_failed_total Failed inference requests\n\
         # TYPE inference_requests_failed_total counter\n\
         inference_requests_failed_total {}\n\n",
        inference_metrics.failed_inferences
    ));

    output.push_str(&format!(
        "# HELP inference_time_avg_ms Average inference time in milliseconds\n\
         # TYPE inference_time_avg_ms gauge\n\
         inference_time_avg_ms {:.2}\n\n",
        inference_metrics.avg_inference_time_ms
    ));

    output.push_str(&format!(
        "# HELP inference_tokens_total Total tokens generated\n\
         # TYPE inference_tokens_total counter\n\
         inference_tokens_total {}\n\n",
        inference_metrics.total_tokens_generated
    ));

    output.push_str(&format!(
        "# HELP inference_healthy Inference engine health status\n\
         # TYPE inference_healthy gauge\n\
         inference_healthy {}\n",
        i32::from(inference_metrics.healthy)
    ));

    output
}

#[cfg(test)]
mod tests {
    use super::*;

    // === MetricsCollector Tests ===

    #[test]
    fn metrics_collector_default() {
        let collector = MetricsCollector::default();
        let metrics = collector.request_metrics();
        assert_eq!(metrics.total_requests, 0);
        assert_eq!(metrics.success_count, 0);
        assert_eq!(metrics.active_requests, 0);
    }

    #[test]
    fn request_start_increments_counters() {
        let collector = MetricsCollector::new();
        collector.request_start();

        let metrics = collector.request_metrics();
        assert_eq!(metrics.total_requests, 1);
        assert_eq!(metrics.active_requests, 1);
    }

    #[test]
    fn request_end_decrements_active() {
        let collector = MetricsCollector::new();
        collector.request_start();
        collector.request_end(1000, 200);

        let metrics = collector.request_metrics();
        assert_eq!(metrics.total_requests, 1);
        assert_eq!(metrics.active_requests, 0);
        assert_eq!(metrics.success_count, 1);
    }

    #[test]
    fn request_end_tracks_success_codes() {
        let collector = MetricsCollector::new();

        for status in [200, 201, 204] {
            collector.request_start();
            collector.request_end(1000, status);
        }

        let metrics = collector.request_metrics();
        assert_eq!(metrics.success_count, 3);
        assert_eq!(metrics.client_error_count, 0);
        assert_eq!(metrics.server_error_count, 0);
    }

    #[test]
    fn request_end_tracks_client_errors() {
        let collector = MetricsCollector::new();

        for status in [400, 401, 404, 422] {
            collector.request_start();
            collector.request_end(1000, status);
        }

        let metrics = collector.request_metrics();
        assert_eq!(metrics.success_count, 0);
        assert_eq!(metrics.client_error_count, 4);
        assert_eq!(metrics.server_error_count, 0);
    }

    #[test]
    fn request_end_tracks_server_errors() {
        let collector = MetricsCollector::new();

        for status in [500, 502, 503] {
            collector.request_start();
            collector.request_end(1000, status);
        }

        let metrics = collector.request_metrics();
        assert_eq!(metrics.success_count, 0);
        assert_eq!(metrics.client_error_count, 0);
        assert_eq!(metrics.server_error_count, 3);
    }

    #[test]
    fn avg_response_time_calculation() {
        let collector = MetricsCollector::new();

        // 3 requests with 1000us, 2000us, 3000us = avg 2000us = 2ms
        collector.request_start();
        collector.request_end(1000, 200);
        collector.request_start();
        collector.request_end(2000, 200);
        collector.request_start();
        collector.request_end(3000, 200);

        let metrics = collector.request_metrics();
        assert!((metrics.avg_response_time_ms - 2.0).abs() < 0.01);
    }

    #[test]
    fn avg_response_time_zero_when_no_requests() {
        let collector = MetricsCollector::new();
        let metrics = collector.request_metrics();
        assert!(metrics.avg_response_time_ms.abs() < f64::EPSILON);
    }

    // === Inference Metrics Tests ===

    #[test]
    fn record_inference_success() {
        let collector = MetricsCollector::new();
        collector.record_inference(true, 5000, 100);

        let metrics = collector.inference_metrics("test-model".to_string(), true);
        assert_eq!(metrics.total_inferences, 1);
        assert_eq!(metrics.successful_inferences, 1);
        assert_eq!(metrics.failed_inferences, 0);
        assert_eq!(metrics.total_tokens_generated, 100);
    }

    #[test]
    fn record_inference_failure() {
        let collector = MetricsCollector::new();
        collector.record_inference(false, 5000, 0);

        let metrics = collector.inference_metrics("test-model".to_string(), false);
        assert_eq!(metrics.total_inferences, 1);
        assert_eq!(metrics.successful_inferences, 0);
        assert_eq!(metrics.failed_inferences, 1);
    }

    #[test]
    fn inference_avg_time_calculation() {
        let collector = MetricsCollector::new();

        // 2 inferences: 2000us and 4000us = avg 3ms
        collector.record_inference(true, 2000, 50);
        collector.record_inference(true, 4000, 50);

        let metrics = collector.inference_metrics("model".to_string(), true);
        assert!((metrics.avg_inference_time_ms - 3.0).abs() < 0.01);
    }

    #[test]
    fn tokens_are_accumulated() {
        let collector = MetricsCollector::new();

        collector.record_inference(true, 1000, 100);
        collector.record_inference(true, 1000, 150);
        collector.record_inference(true, 1000, 50);

        let metrics = collector.inference_metrics("model".to_string(), true);
        assert_eq!(metrics.total_tokens_generated, 300);
    }

    // === Uptime Tests ===

    #[test]
    fn uptime_starts_at_zero_or_near_zero() {
        let collector = MetricsCollector::new();
        // Uptime should be 0 or very small (< 1 second)
        assert!(collector.uptime_seconds() < 2);
    }

    // === Serialization Tests ===

    #[test]
    fn metrics_response_serializes() {
        let response = MetricsResponse {
            app: AppMetrics {
                version: "0.1.0".to_string(),
                name: "test".to_string(),
                uptime_seconds: 100,
                build_info: "abc123".to_string(),
            },
            requests: RequestMetrics {
                total_requests: 1000,
                success_count: 950,
                client_error_count: 40,
                server_error_count: 10,
                avg_response_time_ms: 15.5,
                active_requests: 5,
            },
            inference: InferenceMetrics {
                total_inferences: 500,
                successful_inferences: 495,
                failed_inferences: 5,
                avg_inference_time_ms: 250.0,
                total_tokens_generated: 50000,
                current_model: "qwen2.5".to_string(),
                healthy: true,
            },
            system: SystemMetrics {
                memory_estimate_bytes: Some(1024 * 1024 * 100),
                active_tasks: Some(10),
            },
        };

        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("\"version\":\"0.1.0\""));
        assert!(json.contains("\"total_requests\":1000"));
        assert!(json.contains("\"total_inferences\":500"));
    }

    #[test]
    fn app_metrics_creation() {
        let app = AppMetrics {
            version: "0.1.0".to_string(),
            name: "pisovereign".to_string(),
            uptime_seconds: 3600,
            build_info: "abc123".to_string(),
        };

        assert_eq!(app.version, "0.1.0");
        assert_eq!(app.uptime_seconds, 3600);
    }

    #[test]
    fn request_metrics_creation() {
        let metrics = RequestMetrics {
            total_requests: 100,
            success_count: 90,
            client_error_count: 8,
            server_error_count: 2,
            avg_response_time_ms: 25.0,
            active_requests: 3,
        };

        assert_eq!(metrics.total_requests, 100);
        assert_eq!(metrics.success_count, 90);
    }

    #[test]
    fn inference_metrics_creation() {
        let metrics = InferenceMetrics {
            total_inferences: 50,
            successful_inferences: 48,
            failed_inferences: 2,
            avg_inference_time_ms: 500.0,
            total_tokens_generated: 10000,
            current_model: "test".to_string(),
            healthy: true,
        };

        assert_eq!(metrics.total_inferences, 50);
        assert!(metrics.healthy);
    }

    #[test]
    fn system_metrics_optional_fields() {
        let metrics = SystemMetrics {
            memory_estimate_bytes: None,
            active_tasks: None,
        };

        assert!(metrics.memory_estimate_bytes.is_none());
        assert!(metrics.active_tasks.is_none());
    }

    // === Thread Safety Tests ===

    #[test]
    fn metrics_collector_is_thread_safe() {
        use std::thread;

        let collector = std::sync::Arc::new(MetricsCollector::new());
        let mut handles = vec![];

        for _ in 0..10 {
            let collector = std::sync::Arc::clone(&collector);
            handles.push(thread::spawn(move || {
                for _ in 0..100 {
                    collector.request_start();
                    collector.request_end(1000, 200);
                }
            }));
        }

        for handle in handles {
            handle.join().unwrap();
        }

        let metrics = collector.request_metrics();
        assert_eq!(metrics.total_requests, 1000);
        assert_eq!(metrics.success_count, 1000);
        assert_eq!(metrics.active_requests, 0);
    }
}
