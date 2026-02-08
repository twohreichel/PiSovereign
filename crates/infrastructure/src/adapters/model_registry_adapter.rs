//! Model registry adapter - Implements ModelRegistryPort using Ollama API

use application::error::ApplicationError;
use application::ports::{ModelCapabilities, ModelInfo, ModelRegistryPort};
use async_trait::async_trait;
use parking_lot::RwLock;
use reqwest::Client;
use serde::Deserialize;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tracing::{debug, instrument};

use super::{CircuitBreaker, CircuitBreakerConfig};

/// Configuration for the Ollama model registry
#[derive(Debug, Clone)]
pub struct OllamaModelRegistryConfig {
    /// Base URL for the Ollama-compatible API
    pub base_url: String,
    /// Request timeout
    pub timeout: Duration,
    /// Cache TTL for model list
    pub cache_ttl: Duration,
}

impl Default for OllamaModelRegistryConfig {
    fn default() -> Self {
        Self {
            base_url: "http://localhost:11434".to_string(),
            timeout: Duration::from_secs(10),
            cache_ttl: Duration::from_secs(300), // 5 minutes
        }
    }
}

/// Cached model information
struct ModelCache {
    models: Vec<ModelInfo>,
    fetched_at: Instant,
}

/// Adapter for model registry using Ollama API
pub struct OllamaModelRegistryAdapter {
    client: Client,
    config: OllamaModelRegistryConfig,
    cache: Arc<RwLock<Option<ModelCache>>>,
    circuit_breaker: Option<CircuitBreaker>,
}

impl std::fmt::Debug for OllamaModelRegistryAdapter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OllamaModelRegistryAdapter")
            .field("base_url", &self.config.base_url)
            .field(
                "circuit_breaker",
                &self.circuit_breaker.as_ref().map(CircuitBreaker::name),
            )
            .finish_non_exhaustive()
    }
}

impl OllamaModelRegistryAdapter {
    /// Create a new adapter with default configuration
    ///
    /// # Errors
    ///
    /// Returns an error if the HTTP client fails to initialize.
    pub fn new() -> Result<Self, ApplicationError> {
        Self::with_config(OllamaModelRegistryConfig::default())
    }

    /// Create with custom configuration
    ///
    /// # Errors
    ///
    /// Returns an error if the HTTP client fails to initialize.
    pub fn with_config(config: OllamaModelRegistryConfig) -> Result<Self, ApplicationError> {
        let client = Client::builder()
            .timeout(config.timeout)
            .build()
            .map_err(|e| ApplicationError::Internal(e.to_string()))?;

        Ok(Self {
            client,
            config,
            cache: Arc::new(RwLock::new(None)),
            circuit_breaker: None,
        })
    }

    /// Enable circuit breaker with default configuration
    #[must_use]
    pub fn with_circuit_breaker(mut self) -> Self {
        self.circuit_breaker = Some(CircuitBreaker::new("hailo-models"));
        self
    }

    /// Enable circuit breaker with custom configuration
    #[must_use]
    pub fn with_circuit_breaker_config(mut self, config: CircuitBreakerConfig) -> Self {
        self.circuit_breaker = Some(CircuitBreaker::with_config("hailo-models", config));
        self
    }

    /// Check circuit and return error if open
    fn check_circuit(&self) -> Result<(), ApplicationError> {
        if let Some(ref cb) = self.circuit_breaker {
            if cb.is_open() {
                return Err(ApplicationError::ExternalService(
                    "Ollama model registry circuit breaker is open".into(),
                ));
            }
        }
        Ok(())
    }

    /// Get cached models if valid
    fn get_cached_models(&self) -> Option<Vec<ModelInfo>> {
        let cache = self.cache.read();
        cache
            .as_ref()
            .filter(|c| c.fetched_at.elapsed() < self.config.cache_ttl)
            .map(|c| c.models.clone())
    }

    /// Update cache with new models
    fn update_cache(&self, models: Vec<ModelInfo>) {
        let mut cache = self.cache.write();
        *cache = Some(ModelCache {
            models,
            fetched_at: Instant::now(),
        });
    }

    /// Clear the cache
    fn clear_cache(&self) {
        let mut cache = self.cache.write();
        *cache = None;
    }

    /// Fetch models from the API
    async fn fetch_models(&self) -> Result<Vec<ModelInfo>, ApplicationError> {
        let url = format!("{}/v1/models", self.config.base_url);

        let response = self
            .client
            .get(&url)
            .send()
            .await
            .map_err(|e| ApplicationError::ExternalService(e.to_string()))?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(ApplicationError::ExternalService(format!(
                "API returned {status}: {body}"
            )));
        }

        let api_response: OllamaModelsResponse = response
            .json()
            .await
            .map_err(|e| ApplicationError::Internal(format!("Failed to parse response: {e}")))?;

        let models = api_response.data.iter().map(Self::convert_model).collect();

        Ok(models)
    }

    /// Convert API model to ModelInfo
    fn convert_model(model: &OllamaModel) -> ModelInfo {
        // Parse model name to extract variant
        let (name, variant) = Self::parse_model_name(&model.id);

        // Estimate context length based on model name patterns
        let context_length = Self::estimate_context_length(&model.id);

        ModelInfo {
            id: model.id.clone(),
            name,
            description: None,
            capabilities: ModelCapabilities {
                text_generation: true,
                chat: true,
                embeddings: model.id.contains("embed"),
                code: model.id.contains("code") || model.id.contains("starcoder"),
                context_length: Some(context_length),
            },
            variant,
            available: true,
        }
    }

    /// Parse model name to extract base name and variant
    fn parse_model_name(model_id: &str) -> (String, Option<String>) {
        // Common patterns: llama3:8b, mistral:7b-instruct, etc.
        if let Some((base, variant)) = model_id.split_once(':') {
            (Self::format_name(base), Some(variant.to_uppercase()))
        } else {
            (Self::format_name(model_id), None)
        }
    }

    /// Format model name nicely
    fn format_name(name: &str) -> String {
        // Simple capitalization
        let mut chars = name.chars();
        chars.next().map_or_else(String::new, |first| {
            first.to_uppercase().chain(chars).collect()
        })
    }

    /// Estimate context length based on model name
    fn estimate_context_length(model_id: &str) -> u32 {
        let id = model_id.to_lowercase();

        // Check for explicit context sizes in name
        if id.contains("128k") {
            return 131_072;
        }
        if id.contains("64k") {
            return 65_536;
        }
        if id.contains("32k") {
            return 32_768;
        }
        if id.contains("16k") {
            return 16_384;
        }

        // Default estimates based on model families
        if id.contains("llama3") {
            8_192
        } else if id.contains("llama2") {
            4_096
        } else if id.contains("mistral") || id.contains("mixtral") {
            32_768
        } else if id.contains("phi") {
            2_048
        } else if id.contains("gemma") {
            8_192
        } else {
            4_096 // Default
        }
    }
}

#[async_trait]
impl ModelRegistryPort for OllamaModelRegistryAdapter {
    #[instrument(skip(self))]
    async fn list_models(&self) -> Result<Vec<ModelInfo>, ApplicationError> {
        self.check_circuit()?;

        // Try cache first
        if let Some(models) = self.get_cached_models() {
            debug!(count = models.len(), "Returning cached models");
            return Ok(models);
        }

        // Fetch from API
        let models = self.fetch_models().await?;
        debug!(count = models.len(), "Fetched models from API");

        self.update_cache(models.clone());
        Ok(models)
    }

    #[instrument(skip(self), fields(model_id))]
    async fn get_model(&self, model_id: &str) -> Result<Option<ModelInfo>, ApplicationError> {
        let models = self.list_models().await?;
        Ok(models.into_iter().find(|m| m.id == model_id))
    }

    #[instrument(skip(self))]
    async fn refresh(&self) -> Result<(), ApplicationError> {
        self.check_circuit()?;
        self.clear_cache();

        // Fetch and cache new models
        let models = self.fetch_models().await?;
        self.update_cache(models);

        debug!("Model registry refreshed");
        Ok(())
    }
}

/// Ollama API models response
#[derive(Debug, Deserialize)]
struct OllamaModelsResponse {
    data: Vec<OllamaModel>,
}

/// Ollama API model
#[derive(Debug, Deserialize)]
struct OllamaModel {
    id: String,
    #[serde(default)]
    #[allow(dead_code)]
    object: String,
    #[serde(default)]
    #[allow(dead_code)]
    owned_by: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_default() {
        let config = OllamaModelRegistryConfig::default();
        assert_eq!(config.base_url, "http://localhost:11434");
        assert_eq!(config.timeout, Duration::from_secs(10));
        assert_eq!(config.cache_ttl, Duration::from_secs(300));
    }

    #[test]
    fn new_creates_adapter() {
        let adapter = OllamaModelRegistryAdapter::new();
        assert!(adapter.is_ok());
    }

    #[test]
    fn with_circuit_breaker() {
        let adapter = OllamaModelRegistryAdapter::new()
            .unwrap()
            .with_circuit_breaker();
        assert!(adapter.circuit_breaker.is_some());
    }

    #[test]
    fn debug_impl() {
        let adapter = OllamaModelRegistryAdapter::new().unwrap();
        let debug_str = format!("{adapter:?}");
        assert!(debug_str.contains("OllamaModelRegistryAdapter"));
        assert!(debug_str.contains("localhost:11434"));
    }

    #[test]
    fn parse_model_name_with_variant() {
        let (name, variant) = OllamaModelRegistryAdapter::parse_model_name("llama3:8b");
        assert_eq!(name, "Llama3");
        assert_eq!(variant, Some("8B".to_string()));
    }

    #[test]
    fn parse_model_name_without_variant() {
        let (name, variant) = OllamaModelRegistryAdapter::parse_model_name("mistral");
        assert_eq!(name, "Mistral");
        assert!(variant.is_none());
    }

    #[test]
    fn estimate_context_llama3() {
        let ctx = OllamaModelRegistryAdapter::estimate_context_length("llama3:8b");
        assert_eq!(ctx, 8_192);
    }

    #[test]
    fn estimate_context_mistral() {
        let ctx = OllamaModelRegistryAdapter::estimate_context_length("mistral:7b");
        assert_eq!(ctx, 32_768);
    }

    #[test]
    fn estimate_context_explicit() {
        let ctx = OllamaModelRegistryAdapter::estimate_context_length("model-128k");
        assert_eq!(ctx, 131_072);
    }

    #[test]
    fn estimate_context_default() {
        let ctx = OllamaModelRegistryAdapter::estimate_context_length("unknown-model");
        assert_eq!(ctx, 4_096);
    }

    #[test]
    fn convert_model_basic() {
        let api_model = OllamaModel {
            id: "llama3:8b".to_string(),
            object: "model".to_string(),
            owned_by: "library".to_string(),
        };

        let model = OllamaModelRegistryAdapter::convert_model(&api_model);
        assert_eq!(model.id, "llama3:8b");
        assert_eq!(model.name, "Llama3");
        assert_eq!(model.variant, Some("8B".to_string()));
        assert!(model.capabilities.text_generation);
        assert!(model.capabilities.chat);
        assert!(!model.capabilities.embeddings);
        assert!(model.available);
    }

    #[test]
    fn convert_model_embeddings() {
        let api_model = OllamaModel {
            id: "nomic-embed-text".to_string(),
            object: "model".to_string(),
            owned_by: "library".to_string(),
        };

        let model = OllamaModelRegistryAdapter::convert_model(&api_model);
        assert!(model.capabilities.embeddings);
    }

    #[test]
    fn convert_model_code() {
        let api_model = OllamaModel {
            id: "codellama:7b".to_string(),
            object: "model".to_string(),
            owned_by: "library".to_string(),
        };

        let model = OllamaModelRegistryAdapter::convert_model(&api_model);
        assert!(model.capabilities.code);
    }

    #[test]
    fn trait_is_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<OllamaModelRegistryAdapter>();
    }
}
