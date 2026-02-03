//! Health check handlers

use axum::{extract::State, http::StatusCode, Json};
use serde::Serialize;

use crate::state::AppState;

/// Health check response
#[derive(Serialize)]
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
#[derive(Serialize)]
pub struct ReadinessResponse {
    pub ready: bool,
    pub inference: ServiceStatus,
}

/// Status of a service
#[derive(Serialize)]
pub struct ServiceStatus {
    pub healthy: bool,
    pub model: Option<String>,
}

/// Readiness check - is the server ready to accept requests?
pub async fn readiness_check(State(state): State<AppState>) -> (StatusCode, Json<ReadinessResponse>) {
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
