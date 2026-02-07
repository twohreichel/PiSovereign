//! Health check handlers

use std::collections::HashMap;

use application::{HealthReport, ServiceHealth};
use axum::{Json, extract::State, http::StatusCode};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::state::AppState;

/// Health check response
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[schema(example = json!({"status": "ok", "version": "0.1.0"}))]
pub struct HealthResponse {
    /// Health status (always "ok" if responding)
    pub status: String,
    /// Application version
    pub version: String,
}

/// Liveness check - is the server running?
#[utoipa::path(
    get,
    path = "/health",
    tag = "health",
    responses(
        (status = 200, description = "Service is alive", body = HealthResponse)
    )
)]
pub async fn health_check() -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ok".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
    })
}

/// Readiness response
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ReadinessResponse {
    /// Whether the service is ready to handle requests
    pub ready: bool,
    /// Inference engine status
    pub inference: ServiceStatus,
}

/// Status of a service
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ServiceStatus {
    /// Whether the service is healthy
    pub healthy: bool,
    /// Current model name if healthy
    pub model: Option<String>,
}

/// Extended readiness response with all external services
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ExtendedReadinessResponse {
    /// Whether the system is ready (all critical services healthy)
    pub ready: bool,
    /// Individual service health statuses
    pub services: HashMap<String, ExtendedServiceStatus>,
    /// Latency percentiles for HTTP requests
    #[serde(skip_serializing_if = "Option::is_none")]
    pub latency: Option<LatencyPercentiles>,
    /// Timestamp when the check was performed
    pub checked_at: String,
}

/// Latency percentiles for monitoring SLOs
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct LatencyPercentiles {
    /// P50 (median) response time in milliseconds
    pub p50_ms: f64,
    /// P90 response time in milliseconds
    pub p90_ms: f64,
    /// P99 response time in milliseconds
    pub p99_ms: f64,
    /// Average response time in milliseconds
    pub avg_ms: f64,
    /// Total number of requests measured
    pub total_requests: u64,
}

/// Extended status of a service with response time
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ExtendedServiceStatus {
    /// Whether the service is healthy
    pub healthy: bool,
    /// Additional information (model name, version, etc.)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub info: Option<String>,
    /// Response time in milliseconds
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response_time_ms: Option<u64>,
    /// Error message if unhealthy
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl From<ServiceHealth> for ExtendedServiceStatus {
    fn from(health: ServiceHealth) -> Self {
        Self {
            healthy: health.healthy,
            info: health.info,
            response_time_ms: health.response_time_ms,
            error: health.error,
        }
    }
}

impl From<HealthReport> for ExtendedReadinessResponse {
    fn from(report: HealthReport) -> Self {
        Self {
            ready: report.healthy,
            services: report
                .services
                .into_iter()
                .map(|(k, v)| (k, v.into()))
                .collect(),
            latency: None, // Set by handler when metrics available
            checked_at: report.checked_at.to_rfc3339(),
        }
    }
}

/// Readiness check - is the server ready to accept requests?
#[utoipa::path(
    get,
    path = "/ready",
    tag = "health",
    responses(
        (status = 200, description = "Service is ready", body = ReadinessResponse),
        (status = 503, description = "Service is not ready", body = ReadinessResponse)
    )
)]
pub async fn readiness_check(
    State(state): State<AppState>,
) -> (StatusCode, Json<ReadinessResponse>) {
    let inference_healthy = state.chat_service.is_healthy().await;
    let model = if inference_healthy {
        Some(state.chat_service.current_model())
    } else {
        None
    };

    let ready = inference_healthy;
    let status_code = if ready {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    };

    (
        status_code,
        Json(ReadinessResponse {
            ready,
            inference: ServiceStatus {
                healthy: inference_healthy,
                model,
            },
        }),
    )
}

/// Extended readiness check - health of all external services
#[utoipa::path(
    get,
    path = "/ready/all",
    tag = "health",
    responses(
        (status = 200, description = "All services healthy", body = ExtendedReadinessResponse),
        (status = 503, description = "One or more services unhealthy", body = ExtendedReadinessResponse)
    )
)]
pub async fn extended_readiness_check(
    State(state): State<AppState>,
) -> (StatusCode, Json<ExtendedReadinessResponse>) {
    // Get latency metrics if available
    let latency = {
        let metrics = state.metrics.as_ref();
        let request_metrics = metrics.request_metrics();
        Some(LatencyPercentiles {
            p50_ms: request_metrics.p50_response_time_ms,
            p90_ms: request_metrics.p90_response_time_ms,
            p99_ms: request_metrics.p99_response_time_ms,
            avg_ms: request_metrics.avg_response_time_ms,
            total_requests: request_metrics.total_requests,
        })
    };

    let Some(health_service) = &state.health_service else {
        // Fall back to basic inference check if HealthService not configured
        let inference_healthy = state.chat_service.is_healthy().await;
        let model = if inference_healthy {
            Some(state.chat_service.current_model())
        } else {
            None
        };

        let mut services = HashMap::new();
        services.insert(
            "inference".to_string(),
            ExtendedServiceStatus {
                healthy: inference_healthy,
                info: model,
                response_time_ms: None,
                error: if inference_healthy {
                    None
                } else {
                    Some("Inference unhealthy".to_string())
                },
            },
        );

        let status_code = if inference_healthy {
            StatusCode::OK
        } else {
            StatusCode::SERVICE_UNAVAILABLE
        };

        return (
            status_code,
            Json(ExtendedReadinessResponse {
                ready: inference_healthy,
                services,
                latency,
                checked_at: chrono::Utc::now().to_rfc3339(),
            }),
        );
    };

    let report = health_service.check_all().await;
    let status_code = if report.healthy {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    };

    let mut response: ExtendedReadinessResponse = report.into();
    response.latency = latency;

    (status_code, Json(response))
}

/// Check inference engine health
#[utoipa::path(
    get,
    path = "/health/inference",
    tag = "health",
    responses(
        (status = 200, description = "Inference healthy", body = ExtendedServiceStatus),
        (status = 503, description = "Inference unhealthy", body = ExtendedServiceStatus)
    )
)]
pub async fn inference_health_check(
    State(state): State<AppState>,
) -> (StatusCode, Json<ExtendedServiceStatus>) {
    let status = if let Some(health_service) = &state.health_service {
        health_service.check_inference().await.into()
    } else {
        let healthy = state.chat_service.is_healthy().await;
        ExtendedServiceStatus {
            healthy,
            info: if healthy {
                Some(state.chat_service.current_model())
            } else {
                None
            },
            response_time_ms: None,
            error: if healthy {
                None
            } else {
                Some("Inference unhealthy".to_string())
            },
        }
    };

    let status_code = if status.healthy {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    };

    (status_code, Json(status))
}

/// Check email service health
#[utoipa::path(
    get,
    path = "/health/email",
    tag = "health",
    responses(
        (status = 200, description = "Email service healthy", body = ExtendedServiceStatus),
        (status = 503, description = "Email service unhealthy", body = ExtendedServiceStatus)
    )
)]
pub async fn email_health_check(
    State(state): State<AppState>,
) -> (StatusCode, Json<ExtendedServiceStatus>) {
    let status = if let Some(health_service) = &state.health_service {
        health_service.check_email().await.into()
    } else {
        ExtendedServiceStatus {
            healthy: false,
            info: Some("Health service not configured".to_string()),
            response_time_ms: None,
            error: None,
        }
    };

    let status_code = if status.healthy {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    };

    (status_code, Json(status))
}

/// Check calendar service health
#[utoipa::path(
    get,
    path = "/health/calendar",
    tag = "health",
    responses(
        (status = 200, description = "Calendar service healthy", body = ExtendedServiceStatus),
        (status = 503, description = "Calendar service unhealthy", body = ExtendedServiceStatus)
    )
)]
pub async fn calendar_health_check(
    State(state): State<AppState>,
) -> (StatusCode, Json<ExtendedServiceStatus>) {
    let status = if let Some(health_service) = &state.health_service {
        health_service.check_calendar().await.into()
    } else {
        ExtendedServiceStatus {
            healthy: false,
            info: Some("Health service not configured".to_string()),
            response_time_ms: None,
            error: None,
        }
    };

    let status_code = if status.healthy {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    };

    (status_code, Json(status))
}

/// Check weather service health
#[utoipa::path(
    get,
    path = "/health/weather",
    tag = "health",
    responses(
        (status = 200, description = "Weather service healthy", body = ExtendedServiceStatus),
        (status = 503, description = "Weather service unhealthy", body = ExtendedServiceStatus)
    )
)]
pub async fn weather_health_check(
    State(state): State<AppState>,
) -> (StatusCode, Json<ExtendedServiceStatus>) {
    let status = if let Some(health_service) = &state.health_service {
        health_service.check_weather().await.into()
    } else {
        ExtendedServiceStatus {
            healthy: false,
            info: Some("Health service not configured".to_string()),
            response_time_ms: None,
            error: None,
        }
    };

    let status_code = if status.healthy {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    };

    (status_code, Json(status))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn health_response_creation() {
        let resp = HealthResponse {
            status: "ok".to_string(),
            version: "0.1.0".to_string(),
        };
        assert_eq!(resp.status, "ok");
        assert_eq!(resp.version, "0.1.0");
    }

    #[test]
    fn health_response_serialization() {
        let resp = HealthResponse {
            status: "ok".to_string(),
            version: "0.1.0".to_string(),
        };
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("status"));
        assert!(json.contains("ok"));
        assert!(json.contains("version"));
    }

    #[test]
    fn health_response_deserialization() {
        let json = r#"{"status":"ok","version":"0.1.0"}"#;
        let resp: HealthResponse = serde_json::from_str(json).unwrap();
        assert_eq!(resp.status, "ok");
        assert_eq!(resp.version, "0.1.0");
    }

    #[test]
    fn service_status_healthy() {
        let status = ServiceStatus {
            healthy: true,
            model: Some("qwen".to_string()),
        };
        assert!(status.healthy);
        assert_eq!(status.model, Some("qwen".to_string()));
    }

    #[test]
    fn service_status_unhealthy() {
        let status = ServiceStatus {
            healthy: false,
            model: None,
        };
        assert!(!status.healthy);
        assert!(status.model.is_none());
    }

    #[test]
    fn service_status_serialization() {
        let status = ServiceStatus {
            healthy: true,
            model: Some("llama".to_string()),
        };
        let json = serde_json::to_string(&status).unwrap();
        assert!(json.contains("healthy"));
        assert!(json.contains("true"));
        assert!(json.contains("model"));
    }

    #[test]
    fn readiness_response_ready() {
        let resp = ReadinessResponse {
            ready: true,
            inference: ServiceStatus {
                healthy: true,
                model: Some("qwen".to_string()),
            },
        };
        assert!(resp.ready);
        assert!(resp.inference.healthy);
    }

    #[test]
    fn readiness_response_not_ready() {
        let resp = ReadinessResponse {
            ready: false,
            inference: ServiceStatus {
                healthy: false,
                model: None,
            },
        };
        assert!(!resp.ready);
        assert!(!resp.inference.healthy);
    }

    #[test]
    fn readiness_response_serialization() {
        let resp = ReadinessResponse {
            ready: true,
            inference: ServiceStatus {
                healthy: true,
                model: None,
            },
        };
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("ready"));
        assert!(json.contains("inference"));
        assert!(json.contains("healthy"));
    }

    #[test]
    fn health_response_has_debug() {
        let resp = HealthResponse {
            status: "ok".to_string(),
            version: "0.1.0".to_string(),
        };
        let debug = format!("{resp:?}");
        assert!(debug.contains("HealthResponse"));
    }

    #[test]
    fn service_status_clone() {
        let status = ServiceStatus {
            healthy: true,
            model: Some("test".to_string()),
        };
        #[allow(clippy::redundant_clone)]
        let cloned = status.clone();
        assert_eq!(status.healthy, cloned.healthy);
        assert_eq!(status.model, cloned.model);
    }

    #[tokio::test]
    async fn health_check_returns_ok() {
        let response = health_check().await;
        assert_eq!(response.status, "ok");
        assert!(!response.version.is_empty());
    }

    #[test]
    fn readiness_response_clone() {
        let resp = ReadinessResponse {
            ready: true,
            inference: ServiceStatus {
                healthy: true,
                model: Some("qwen".to_string()),
            },
        };
        #[allow(clippy::redundant_clone)]
        let cloned = resp.clone();
        assert_eq!(resp.ready, cloned.ready);
    }

    #[test]
    fn health_response_clone() {
        let resp = HealthResponse {
            status: "ok".to_string(),
            version: "1.0".to_string(),
        };
        #[allow(clippy::redundant_clone)]
        let cloned = resp.clone();
        assert_eq!(resp.status, cloned.status);
    }

    #[test]
    fn readiness_response_has_debug() {
        let resp = ReadinessResponse {
            ready: false,
            inference: ServiceStatus {
                healthy: false,
                model: None,
            },
        };
        let debug = format!("{resp:?}");
        assert!(debug.contains("ReadinessResponse"));
    }

    #[test]
    fn service_status_has_debug() {
        let status = ServiceStatus {
            healthy: true,
            model: None,
        };
        let debug = format!("{status:?}");
        assert!(debug.contains("ServiceStatus"));
    }

    #[test]
    fn readiness_response_deserialization() {
        let json = r#"{"ready":true,"inference":{"healthy":true,"model":"qwen"}}"#;
        let resp: ReadinessResponse = serde_json::from_str(json).unwrap();
        assert!(resp.ready);
        assert!(resp.inference.healthy);
    }

    #[test]
    fn service_status_deserialization() {
        let json = r#"{"healthy":false,"model":null}"#;
        let status: ServiceStatus = serde_json::from_str(json).unwrap();
        assert!(!status.healthy);
        assert!(status.model.is_none());
    }

    // Extended status tests
    #[test]
    fn extended_service_status_creation() {
        let status = ExtendedServiceStatus {
            healthy: true,
            info: Some("test-model".to_string()),
            response_time_ms: Some(42),
            error: None,
        };
        assert!(status.healthy);
        assert_eq!(status.info, Some("test-model".to_string()));
        assert_eq!(status.response_time_ms, Some(42));
    }

    #[test]
    fn extended_service_status_unhealthy() {
        let status = ExtendedServiceStatus {
            healthy: false,
            info: None,
            response_time_ms: None,
            error: Some("Connection refused".to_string()),
        };
        assert!(!status.healthy);
        assert!(status.error.is_some());
    }

    #[test]
    fn extended_service_status_serialization() {
        let status = ExtendedServiceStatus {
            healthy: true,
            info: Some("model".to_string()),
            response_time_ms: Some(100),
            error: None,
        };
        let json = serde_json::to_string(&status).unwrap();
        assert!(json.contains("healthy"));
        assert!(json.contains("true"));
        assert!(json.contains("model"));
        assert!(json.contains("100"));
        // error should be skipped when None
        assert!(!json.contains("error"));
    }

    #[test]
    fn extended_service_status_deserialization() {
        let json = r#"{"healthy":true,"info":"test","response_time_ms":50}"#;
        let status: ExtendedServiceStatus = serde_json::from_str(json).unwrap();
        assert!(status.healthy);
        assert_eq!(status.info, Some("test".to_string()));
        assert_eq!(status.response_time_ms, Some(50));
    }

    #[test]
    fn extended_readiness_response_creation() {
        let mut services = HashMap::new();
        services.insert(
            "inference".to_string(),
            ExtendedServiceStatus {
                healthy: true,
                info: Some("qwen".to_string()),
                response_time_ms: Some(10),
                error: None,
            },
        );

        let resp = ExtendedReadinessResponse {
            ready: true,
            services,
            latency: None,
            checked_at: "2024-01-01T00:00:00Z".to_string(),
        };
        assert!(resp.ready);
        assert!(resp.services.contains_key("inference"));
    }

    #[test]
    fn extended_readiness_response_serialization() {
        let mut services = HashMap::new();
        services.insert(
            "inference".to_string(),
            ExtendedServiceStatus {
                healthy: true,
                info: None,
                response_time_ms: None,
                error: None,
            },
        );

        let resp = ExtendedReadinessResponse {
            ready: true,
            services,
            latency: Some(LatencyPercentiles {
                p50_ms: 5.0,
                p90_ms: 10.0,
                p99_ms: 25.0,
                avg_ms: 7.5,
                total_requests: 100,
            }),
            checked_at: "2024-01-01T00:00:00Z".to_string(),
        };
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("ready"));
        assert!(json.contains("services"));
        assert!(json.contains("checked_at"));
    }

    #[test]
    fn extended_service_status_clone() {
        let status = ExtendedServiceStatus {
            healthy: true,
            info: Some("test".to_string()),
            response_time_ms: Some(50),
            error: None,
        };
        #[allow(clippy::redundant_clone)]
        let cloned = status.clone();
        assert_eq!(status.healthy, cloned.healthy);
        assert_eq!(status.info, cloned.info);
    }

    #[test]
    fn extended_readiness_response_clone() {
        let resp = ExtendedReadinessResponse {
            ready: true,
            services: HashMap::new(),
            latency: None,
            checked_at: "2024-01-01T00:00:00Z".to_string(),
        };
        #[allow(clippy::redundant_clone)]
        let cloned = resp.clone();
        assert_eq!(resp.ready, cloned.ready);
    }

    #[test]
    fn extended_service_status_debug() {
        let status = ExtendedServiceStatus {
            healthy: true,
            info: None,
            response_time_ms: None,
            error: None,
        };
        let debug = format!("{status:?}");
        assert!(debug.contains("ExtendedServiceStatus"));
    }

    #[test]
    fn extended_readiness_response_debug() {
        let resp = ExtendedReadinessResponse {
            ready: true,
            services: HashMap::new(),
            latency: None,
            checked_at: "2024-01-01T00:00:00Z".to_string(),
        };
        let debug = format!("{resp:?}");
        assert!(debug.contains("ExtendedReadinessResponse"));
    }

    #[test]
    fn service_health_to_extended_status() {
        use application::ServiceHealth;

        let health = ServiceHealth::healthy_with_info("model").with_response_time(100);
        let status: ExtendedServiceStatus = health.into();

        assert!(status.healthy);
        assert_eq!(status.info, Some("model".to_string()));
        assert_eq!(status.response_time_ms, Some(100));
    }

    #[test]
    fn health_report_to_extended_readiness() {
        use application::HealthReport;

        let mut services = std::collections::HashMap::new();
        services.insert(
            "inference".to_string(),
            application::ServiceHealth::healthy(),
        );

        let report = HealthReport::new(services);
        let resp: ExtendedReadinessResponse = report.into();

        assert!(resp.ready);
        assert!(resp.services.contains_key("inference"));
    }
}
