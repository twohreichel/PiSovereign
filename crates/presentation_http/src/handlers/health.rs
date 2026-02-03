//! Health check handlers

use axum::{Json, extract::State, http::StatusCode};
use serde::{Deserialize, Serialize};

use crate::state::AppState;

/// Health check response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthResponse {
    pub status: String,
    pub version: String,
}

/// Liveness check - is the server running?
pub async fn health_check() -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ok".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
    })
}

/// Readiness response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReadinessResponse {
    pub ready: bool,
    pub inference: ServiceStatus,
}

/// Status of a service
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceStatus {
    pub healthy: bool,
    pub model: Option<String>,
}

/// Readiness check - is the server ready to accept requests?
pub async fn readiness_check(
    State(state): State<AppState>,
) -> (StatusCode, Json<ReadinessResponse>) {
    let inference_healthy = state.chat_service.is_healthy().await;
    let model = if inference_healthy {
        Some(state.chat_service.current_model().to_string())
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
}
