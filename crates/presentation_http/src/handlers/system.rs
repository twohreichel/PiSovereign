//! System handlers

use axum::{Json, extract::State};
use serde::Serialize;

use crate::state::AppState;

/// System status response
#[derive(Debug, Serialize)]
pub struct StatusResponse {
    pub version: String,
    pub model: String,
    pub inference_healthy: bool,
    pub uptime_info: String,
}

/// Get system status
pub async fn status(State(state): State<AppState>) -> Json<StatusResponse> {
    let inference_healthy = state.chat_service.is_healthy().await;

    Json(StatusResponse {
        version: env!("CARGO_PKG_VERSION").to_string(),
        model: state.chat_service.current_model().to_string(),
        inference_healthy,
        uptime_info: "Running on Raspberry Pi 5 + Hailo-10H".to_string(),
    })
}

/// Models list response
#[derive(Debug, Serialize)]
pub struct ModelsResponse {
    pub current: String,
    pub available: Vec<ModelInfo>,
}

/// Information about a model
#[derive(Debug, Serialize)]
pub struct ModelInfo {
    pub name: String,
    pub description: String,
    pub parameters: String,
}

/// List available models
pub async fn list_models(State(state): State<AppState>) -> Json<ModelsResponse> {
    // TODO: Query actual available models from Hailo
    let available = vec![
        ModelInfo {
            name: "qwen2.5-1.5b-instruct".to_string(),
            description: "Qwen 2.5 1.5B Instruct - General purpose".to_string(),
            parameters: "1.5B".to_string(),
        },
        ModelInfo {
            name: "llama3.2-1b-instruct".to_string(),
            description: "Llama 3.2 1B Instruct - Fast responses".to_string(),
            parameters: "1B".to_string(),
        },
        ModelInfo {
            name: "qwen2-1.5b-function-calling".to_string(),
            description: "Qwen 2 1.5B Function Calling - Optimized for tools".to_string(),
            parameters: "1.5B".to_string(),
        },
    ];

    Json(ModelsResponse {
        current: state.chat_service.current_model().to_string(),
        available,
    })
}
