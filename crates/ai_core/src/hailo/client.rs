//! Hailo-Ollama client implementation

use async_trait::async_trait;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::time::Duration;
use tracing::{debug, info, instrument, warn};

use crate::config::InferenceConfig;
use crate::error::InferenceError;
use crate::ports::{
    InferenceEngine, InferenceRequest, InferenceResponse, StreamingResponse, TokenUsage,
};

use super::streaming::create_stream;

/// Hailo-10H inference engine using hailo-ollama
pub struct HailoInferenceEngine {
    client: Client,
    config: InferenceConfig,
}

impl HailoInferenceEngine {
    /// Create a new Hailo inference engine
    pub fn new(config: InferenceConfig) -> Result<Self, InferenceError> {
        let client = Client::builder()
            .timeout(Duration::from_millis(config.timeout_ms))
            .build()
            .map_err(|e| InferenceError::ConnectionFailed(e.to_string()))?;

        info!(
            base_url = %config.base_url,
            model = %config.default_model,
            "Initialized Hailo inference engine"
        );

        Ok(Self { client, config })
    }

    /// Create with default configuration for Hailo-10H
    pub fn with_defaults() -> Result<Self, InferenceError> {
        Self::new(InferenceConfig::hailo_qwen())
    }

    /// Build the API URL for a given endpoint
    fn api_url(&self, endpoint: &str) -> String {
        format!("{}/api/{}", self.config.base_url, endpoint.trim_start_matches('/'))
    }

    /// Get the model to use for a request
    fn resolve_model(&self, request: &InferenceRequest) -> &str {
        request
            .model
            .as_deref()
            .unwrap_or(&self.config.default_model)
    }
}

/// Ollama-format chat request
#[derive(Debug, Serialize)]
struct OllamaChatRequest {
    model: String,
    messages: Vec<OllamaMessage>,
    stream: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    options: Option<OllamaOptions>,
}

#[derive(Debug, Serialize)]
struct OllamaMessage {
    role: String,
    content: String,
}

#[derive(Debug, Serialize)]
struct OllamaOptions {
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    num_predict: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    top_p: Option<f32>,
}

/// Ollama-format chat response
#[derive(Debug, Deserialize)]
struct OllamaChatResponse {
    model: String,
    message: OllamaResponseMessage,
    done: bool,
    #[serde(default)]
    prompt_eval_count: Option<u32>,
    #[serde(default)]
    eval_count: Option<u32>,
}

#[derive(Debug, Deserialize)]
struct OllamaResponseMessage {
    role: String,
    content: String,
}

/// Ollama models list response
#[derive(Debug, Deserialize)]
struct OllamaModelsResponse {
    models: Vec<OllamaModel>,
}

#[derive(Debug, Deserialize)]
struct OllamaModel {
    name: String,
}

#[async_trait]
impl InferenceEngine for HailoInferenceEngine {
    #[instrument(skip(self, request), fields(model = %self.resolve_model(&request)))]
    async fn generate(&self, request: InferenceRequest) -> Result<InferenceResponse, InferenceError> {
        let model = self.resolve_model(&request).to_string();

        let ollama_request = OllamaChatRequest {
            model: model.clone(),
            messages: request
                .messages
                .iter()
                .map(|m| OllamaMessage {
                    role: m.role.clone(),
                    content: m.content.clone(),
                })
                .collect(),
            stream: false,
            options: Some(OllamaOptions {
                temperature: request.temperature.or(Some(self.config.temperature)),
                num_predict: request.max_tokens.or(Some(self.config.max_tokens)),
                top_p: Some(self.config.top_p),
            }),
        };

        debug!("Sending request to hailo-ollama");

        let response = self
            .client
            .post(self.api_url("chat"))
            .json(&ollama_request)
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            warn!(status = %status, body = %body, "Inference request failed");
            return Err(InferenceError::ServerError(format!(
                "Status {}: {}",
                status, body
            )));
        }

        let ollama_response: OllamaChatResponse = response
            .json()
            .await
            .map_err(|e| InferenceError::InvalidResponse(e.to_string()))?;

        let usage = match (ollama_response.prompt_eval_count, ollama_response.eval_count) {
            (Some(prompt), Some(completion)) => Some(TokenUsage {
                prompt_tokens: prompt,
                completion_tokens: completion,
                total_tokens: prompt + completion,
            }),
            _ => None,
        };

        debug!(
            tokens = ?usage,
            "Inference completed"
        );

        Ok(InferenceResponse {
            content: ollama_response.message.content,
            model: ollama_response.model,
            usage,
            finish_reason: if ollama_response.done {
                Some("stop".to_string())
            } else {
                None
            },
        })
    }

    #[instrument(skip(self, request), fields(model = %self.resolve_model(&request)))]
    async fn generate_stream(
        &self,
        request: InferenceRequest,
    ) -> Result<StreamingResponse, InferenceError> {
        let model = self.resolve_model(&request).to_string();

        let ollama_request = OllamaChatRequest {
            model,
            messages: request
                .messages
                .iter()
                .map(|m| OllamaMessage {
                    role: m.role.clone(),
                    content: m.content.clone(),
                })
                .collect(),
            stream: true,
            options: Some(OllamaOptions {
                temperature: request.temperature.or(Some(self.config.temperature)),
                num_predict: request.max_tokens.or(Some(self.config.max_tokens)),
                top_p: Some(self.config.top_p),
            }),
        };

        debug!("Starting streaming request to hailo-ollama");

        let response = self
            .client
            .post(self.api_url("chat"))
            .json(&ollama_request)
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(InferenceError::ServerError(format!(
                "Status {}: {}",
                status, body
            )));
        }

        Ok(create_stream(response))
    }

    #[instrument(skip(self))]
    async fn health_check(&self) -> Result<bool, InferenceError> {
        let response = self
            .client
            .get(format!("{}/api/tags", self.config.base_url))
            .timeout(Duration::from_secs(5))
            .send()
            .await;

        match response {
            Ok(resp) => Ok(resp.status().is_success()),
            Err(e) if e.is_timeout() => Ok(false),
            Err(e) if e.is_connect() => Ok(false),
            Err(e) => Err(InferenceError::RequestFailed(e.to_string())),
        }
    }

    #[instrument(skip(self))]
    async fn list_models(&self) -> Result<Vec<String>, InferenceError> {
        let response = self
            .client
            .get(format!("{}/api/tags", self.config.base_url))
            .send()
            .await?;

        if !response.status().is_success() {
            return Err(InferenceError::ServerError(
                response.status().to_string(),
            ));
        }

        let models_response: OllamaModelsResponse = response
            .json()
            .await
            .map_err(|e| InferenceError::InvalidResponse(e.to_string()))?;

        Ok(models_response
            .models
            .into_iter()
            .map(|m| m.name)
            .collect())
    }

    fn default_model(&self) -> &str {
        &self.config.default_model
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_creates_correct_urls() {
        let config = InferenceConfig::default();
        let engine = HailoInferenceEngine::new(config).unwrap();

        assert_eq!(
            engine.api_url("chat"),
            "http://localhost:11434/api/chat"
        );
        assert_eq!(
            engine.api_url("/tags"),
            "http://localhost:11434/api/tags"
        );
    }

    #[test]
    fn default_model_is_qwen() {
        let engine = HailoInferenceEngine::with_defaults().unwrap();
        assert_eq!(engine.default_model(), "qwen2.5-1.5b-instruct");
    }
}
