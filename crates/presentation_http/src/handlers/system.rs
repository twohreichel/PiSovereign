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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_response_serialize() {
        let response = StatusResponse {
            version: "0.1.0".to_string(),
            model: "qwen2.5-1.5b".to_string(),
            inference_healthy: true,
            uptime_info: "Running".to_string(),
        };
        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("0.1.0"));
        assert!(json.contains("qwen2.5-1.5b"));
        assert!(json.contains("true"));
    }

    #[test]
    fn status_response_debug() {
        let response = StatusResponse {
            version: "0.1.0".to_string(),
            model: "test".to_string(),
            inference_healthy: false,
            uptime_info: "Test".to_string(),
        };
        let debug = format!("{response:?}");
        assert!(debug.contains("StatusResponse"));
    }

    #[test]
    fn models_response_serialize() {
        let response = ModelsResponse {
            current: "qwen".to_string(),
            available: vec![ModelInfo {
                name: "qwen".to_string(),
                description: "Qwen model".to_string(),
                parameters: "1.5B".to_string(),
            }],
        };
        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("qwen"));
        assert!(json.contains("1.5B"));
    }

    #[test]
    fn models_response_debug() {
        let response = ModelsResponse {
            current: "test".to_string(),
            available: vec![],
        };
        let debug = format!("{response:?}");
        assert!(debug.contains("ModelsResponse"));
    }

    #[test]
    fn model_info_serialize() {
        let info = ModelInfo {
            name: "llama".to_string(),
            description: "Llama model".to_string(),
            parameters: "1B".to_string(),
        };
        let json = serde_json::to_string(&info).unwrap();
        assert!(json.contains("llama"));
        assert!(json.contains("1B"));
    }

    #[test]
    fn model_info_debug() {
        let info = ModelInfo {
            name: "test".to_string(),
            description: "Test model".to_string(),
            parameters: "100M".to_string(),
        };
        let debug = format!("{info:?}");
        assert!(debug.contains("ModelInfo"));
    }

    #[test]
    fn status_response_healthy_true() {
        let response = StatusResponse {
            version: "1.0.0".to_string(),
            model: "model".to_string(),
            inference_healthy: true,
            uptime_info: "OK".to_string(),
        };
        assert!(response.inference_healthy);
    }

    #[test]
    fn status_response_healthy_false() {
        let response = StatusResponse {
            version: "1.0.0".to_string(),
            model: "model".to_string(),
            inference_healthy: false,
            uptime_info: "Error".to_string(),
        };
        assert!(!response.inference_healthy);
    }

    #[test]
    fn models_response_empty_available() {
        let response = ModelsResponse {
            current: "default".to_string(),
            available: vec![],
        };
        assert!(response.available.is_empty());
    }

    #[test]
    fn models_response_multiple_models() {
        let response = ModelsResponse {
            current: "qwen".to_string(),
            available: vec![
                ModelInfo {
                    name: "qwen".to_string(),
                    description: "Qwen".to_string(),
                    parameters: "1.5B".to_string(),
                },
                ModelInfo {
                    name: "llama".to_string(),
                    description: "Llama".to_string(),
                    parameters: "1B".to_string(),
                },
            ],
        };
        assert_eq!(response.available.len(), 2);
    }
}
