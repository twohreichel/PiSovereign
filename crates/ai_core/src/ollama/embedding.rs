//! Ollama embedding engine implementation
//!
//! Provides text embeddings using Ollama-compatible embedding models
//! such as nomic-embed-text, mxbai-embed-large, or bge-m3.

use std::time::Duration;

use async_trait::async_trait;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use tracing::{debug, info, instrument, warn};

use crate::error::InferenceError;

/// Configuration for the embedding engine
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbeddingConfig {
    /// Base URL of the Ollama server
    #[serde(default = "default_base_url")]
    pub base_url: String,

    /// Embedding model to use
    #[serde(default = "default_embedding_model")]
    pub model: String,

    /// Request timeout in milliseconds
    #[serde(default = "default_timeout_ms")]
    pub timeout_ms: u64,

    /// Number of embedding dimensions (for validation)
    #[serde(default = "default_dimensions")]
    pub dimensions: usize,
}

fn default_base_url() -> String {
    "http://localhost:11434".to_string()
}

fn default_embedding_model() -> String {
    "nomic-embed-text".to_string()
}

const fn default_timeout_ms() -> u64 {
    30000 // 30 seconds
}

const fn default_dimensions() -> usize {
    384 // nomic-embed-text dimensions
}

impl Default for EmbeddingConfig {
    fn default() -> Self {
        Self {
            base_url: default_base_url(),
            model: default_embedding_model(),
            timeout_ms: default_timeout_ms(),
            dimensions: default_dimensions(),
        }
    }
}

impl EmbeddingConfig {
    /// Configuration for nomic-embed-text (384 dimensions)
    #[must_use]
    pub fn nomic_embed_text() -> Self {
        Self {
            base_url: default_base_url(),
            model: "nomic-embed-text".to_string(),
            timeout_ms: default_timeout_ms(),
            dimensions: 384,
        }
    }

    /// Configuration for mxbai-embed-large (1024 dimensions)
    #[must_use]
    pub fn mxbai_embed_large() -> Self {
        Self {
            base_url: default_base_url(),
            model: "mxbai-embed-large".to_string(),
            timeout_ms: default_timeout_ms(),
            dimensions: 1024,
        }
    }

    /// Configuration for bge-m3 (1024 dimensions, multilingual)
    #[must_use]
    pub fn bge_m3() -> Self {
        Self {
            base_url: default_base_url(),
            model: "bge-m3".to_string(),
            timeout_ms: default_timeout_ms(),
            dimensions: 1024,
        }
    }
}

/// Ollama-compatible embedding engine
///
/// Generates text embeddings using Ollama's /api/embed endpoint.
#[derive(Debug)]
pub struct OllamaEmbeddingEngine {
    client: Client,
    config: EmbeddingConfig,
}

impl OllamaEmbeddingEngine {
    /// Create a new embedding engine with the given configuration
    pub fn new(config: EmbeddingConfig) -> Result<Self, InferenceError> {
        let client = Client::builder()
            .timeout(Duration::from_millis(config.timeout_ms))
            .build()
            .map_err(|e| InferenceError::ConnectionFailed(e.to_string()))?;

        info!(
            base_url = %config.base_url,
            model = %config.model,
            dimensions = config.dimensions,
            "Initialized Ollama embedding engine"
        );

        Ok(Self { client, config })
    }

    /// Create with default configuration (nomic-embed-text)
    pub fn with_defaults() -> Result<Self, InferenceError> {
        Self::new(EmbeddingConfig::default())
    }

    /// Build the API URL for the embed endpoint
    fn embed_url(&self) -> String {
        format!("{}/api/embed", self.config.base_url)
    }

    /// Get the configured model name
    #[must_use]
    pub fn model(&self) -> &str {
        &self.config.model
    }

    /// Get the expected embedding dimensions
    #[must_use]
    pub const fn dimensions(&self) -> usize {
        self.config.dimensions
    }

    /// Generate an embedding for a single text
    #[instrument(skip(self, text), fields(model = %self.config.model, text_len = text.len()))]
    pub async fn embed(&self, text: &str) -> Result<Vec<f32>, InferenceError> {
        let request = OllamaEmbedRequest {
            model: self.config.model.clone(),
            input: EmbedInput::Single(text.to_string()),
        };

        debug!("Sending embed request to Ollama");

        let response = self
            .client
            .post(self.embed_url())
            .json(&request)
            .send()
            .await
            .map_err(|e| {
                warn!(error = %e, "Failed to connect to Ollama server");
                InferenceError::ConnectionFailed(e.to_string())
            })?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            warn!(status = %status, error = %error_text, "Ollama embed request failed");
            return Err(InferenceError::ServerError(format!(
                "Ollama returned {status}: {error_text}"
            )));
        }

        let result: OllamaEmbedResponse = response.json().await.map_err(|e| {
            warn!(error = %e, "Failed to parse Ollama response");
            InferenceError::InvalidResponse(e.to_string())
        })?;

        // Extract embedding from the response
        let embedding = match result.embeddings {
            Some(mut embeddings) if !embeddings.is_empty() => embeddings.swap_remove(0),
            _ => match result.embedding {
                Some(embedding) => embedding,
                None => {
                    return Err(InferenceError::InvalidResponse(
                        "No embedding in response".to_string(),
                    ));
                },
            },
        };

        debug!(
            dimensions = embedding.len(),
            "Received embedding from Ollama"
        );

        Ok(embedding)
    }

    /// Generate embeddings for multiple texts in a batch
    #[instrument(skip(self, texts), fields(model = %self.config.model, batch_size = texts.len()))]
    pub async fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, InferenceError> {
        if texts.is_empty() {
            return Ok(Vec::new());
        }

        let request = OllamaEmbedRequest {
            model: self.config.model.clone(),
            input: EmbedInput::Batch(texts.to_vec()),
        };

        debug!("Sending batch embed request to Ollama");

        let response = self
            .client
            .post(self.embed_url())
            .json(&request)
            .send()
            .await
            .map_err(|e| {
                warn!(error = %e, "Failed to connect to Ollama server");
                InferenceError::ConnectionFailed(e.to_string())
            })?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            warn!(status = %status, error = %error_text, "Ollama batch embed request failed");
            return Err(InferenceError::ServerError(format!(
                "Ollama returned {status}: {error_text}"
            )));
        }

        let result: OllamaEmbedResponse = response.json().await.map_err(|e| {
            warn!(error = %e, "Failed to parse Ollama response");
            InferenceError::InvalidResponse(e.to_string())
        })?;

        let embeddings = result.embeddings.unwrap_or_default();

        if embeddings.len() != texts.len() {
            warn!(
                expected = texts.len(),
                got = embeddings.len(),
                "Mismatch in batch embedding count"
            );
        }

        debug!(
            count = embeddings.len(),
            "Received batch embeddings from Ollama"
        );

        Ok(embeddings)
    }

    /// Calculate cosine similarity between two embeddings
    #[must_use]
    pub fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
        domain::cosine_similarity(a, b)
    }
}

/// Ollama embed request format
#[derive(Debug, Serialize)]
struct OllamaEmbedRequest {
    model: String,
    input: EmbedInput,
}

/// Input for embed request - can be single or batch
#[derive(Debug, Serialize)]
#[serde(untagged)]
enum EmbedInput {
    Single(String),
    Batch(Vec<String>),
}

/// Ollama embed response format
#[derive(Debug, Deserialize)]
struct OllamaEmbedResponse {
    /// Single embedding (older API format)
    #[serde(default)]
    embedding: Option<Vec<f32>>,
    /// Multiple embeddings (newer API format)
    #[serde(default)]
    embeddings: Option<Vec<Vec<f32>>>,
}

/// Trait for embedding engines
#[async_trait]
pub trait EmbeddingEngine: Send + Sync {
    /// Generate an embedding for a single text
    async fn embed(&self, text: &str) -> Result<Vec<f32>, InferenceError>;

    /// Generate embeddings for multiple texts in a batch
    async fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, InferenceError>;

    /// Get the model name
    fn model(&self) -> &str;

    /// Get the embedding dimensions
    fn dimensions(&self) -> usize;
}

#[async_trait]
impl EmbeddingEngine for OllamaEmbeddingEngine {
    async fn embed(&self, text: &str) -> Result<Vec<f32>, InferenceError> {
        self.embed(text).await
    }

    async fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, InferenceError> {
        self.embed_batch(texts).await
    }

    fn model(&self) -> &str {
        self.model()
    }

    fn dimensions(&self) -> usize {
        self.dimensions()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config() {
        let config = EmbeddingConfig::default();
        assert_eq!(config.model, "nomic-embed-text");
        assert_eq!(config.dimensions, 384);
        assert_eq!(config.base_url, "http://localhost:11434");
    }

    #[test]
    fn nomic_config() {
        let config = EmbeddingConfig::nomic_embed_text();
        assert_eq!(config.model, "nomic-embed-text");
        assert_eq!(config.dimensions, 384);
    }

    #[test]
    fn mxbai_config() {
        let config = EmbeddingConfig::mxbai_embed_large();
        assert_eq!(config.model, "mxbai-embed-large");
        assert_eq!(config.dimensions, 1024);
    }

    #[test]
    fn bge_config() {
        let config = EmbeddingConfig::bge_m3();
        assert_eq!(config.model, "bge-m3");
        assert_eq!(config.dimensions, 1024);
    }

    #[test]
    fn cosine_similarity_identical() {
        let vec = vec![1.0, 0.0, 0.0];
        let similarity = OllamaEmbeddingEngine::cosine_similarity(&vec, &vec);
        assert!((similarity - 1.0).abs() < 0.001);
    }

    #[test]
    fn cosine_similarity_orthogonal() {
        let a = vec![1.0, 0.0, 0.0];
        let b = vec![0.0, 1.0, 0.0];
        let similarity = OllamaEmbeddingEngine::cosine_similarity(&a, &b);
        assert!(similarity.abs() < 0.001);
    }

    #[test]
    fn cosine_similarity_opposite() {
        let a = vec![1.0, 0.0, 0.0];
        let b = vec![-1.0, 0.0, 0.0];
        let similarity = OllamaEmbeddingEngine::cosine_similarity(&a, &b);
        assert!((similarity + 1.0).abs() < 0.001);
    }

    #[test]
    fn cosine_similarity_real_vectors() {
        let a = vec![0.1, 0.2, 0.3, 0.4];
        let b = vec![0.1, 0.2, 0.3, 0.4];
        let similarity = OllamaEmbeddingEngine::cosine_similarity(&a, &b);
        assert!((similarity - 1.0).abs() < 0.001);
    }

    #[test]
    fn cosine_similarity_different_lengths() {
        let a = vec![1.0, 0.0];
        let b = vec![1.0, 0.0, 0.0];
        let similarity = OllamaEmbeddingEngine::cosine_similarity(&a, &b);
        assert!(similarity.abs() < f32::EPSILON);
    }

    #[test]
    fn cosine_similarity_empty() {
        let a: Vec<f32> = vec![];
        let b: Vec<f32> = vec![];
        let similarity = OllamaEmbeddingEngine::cosine_similarity(&a, &b);
        assert!(similarity.abs() < f32::EPSILON);
    }

    #[test]
    fn embed_url_construction() {
        let config = EmbeddingConfig {
            base_url: "http://example.com:8080".to_string(),
            ..Default::default()
        };
        let engine = OllamaEmbeddingEngine::new(config).unwrap();
        assert_eq!(engine.embed_url(), "http://example.com:8080/api/embed");
    }

    #[test]
    fn config_serialization() {
        let config = EmbeddingConfig::nomic_embed_text();
        let json = serde_json::to_string(&config).unwrap();
        let parsed: EmbeddingConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(config.model, parsed.model);
        assert_eq!(config.dimensions, parsed.dimensions);
    }
}
