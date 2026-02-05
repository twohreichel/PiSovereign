//! Model registry port
//!
//! Defines the interface for discovering available AI models.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::error::ApplicationError;

/// Information about an available AI model
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelInfo {
    /// Unique model identifier
    pub id: String,
    /// Human-readable model name
    pub name: String,
    /// Model description
    pub description: Option<String>,
    /// Model capabilities/features
    pub capabilities: ModelCapabilities,
    /// Model size/variant
    pub variant: Option<String>,
    /// Whether this model is currently available
    pub available: bool,
}

/// Model capabilities
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[allow(clippy::struct_excessive_bools)]
pub struct ModelCapabilities {
    /// Supports text generation
    pub text_generation: bool,
    /// Supports chat/conversation
    pub chat: bool,
    /// Supports embeddings
    pub embeddings: bool,
    /// Supports code completion
    pub code: bool,
    /// Context window size (tokens)
    pub context_length: Option<u32>,
}

/// Port for model registry operations
#[async_trait]
pub trait ModelRegistryPort: Send + Sync {
    /// List all available models
    async fn list_models(&self) -> Result<Vec<ModelInfo>, ApplicationError>;

    /// Get information about a specific model
    async fn get_model(&self, model_id: &str) -> Result<Option<ModelInfo>, ApplicationError>;

    /// Check if a model is available
    async fn is_model_available(&self, model_id: &str) -> Result<bool, ApplicationError> {
        Ok(self.get_model(model_id).await?.is_some_and(|m| m.available))
    }

    /// Refresh the model list (e.g., re-query the backend)
    async fn refresh(&self) -> Result<(), ApplicationError>;

    /// Get the default model for a capability
    async fn get_default_model(
        &self,
        capability: ModelCapability,
    ) -> Result<Option<ModelInfo>, ApplicationError> {
        let models = self.list_models().await?;
        Ok(models.into_iter().find(|m| {
            m.available
                && match capability {
                    ModelCapability::TextGeneration => m.capabilities.text_generation,
                    ModelCapability::Chat => m.capabilities.chat,
                    ModelCapability::Embeddings => m.capabilities.embeddings,
                    ModelCapability::Code => m.capabilities.code,
                }
        }))
    }
}

/// Model capability for filtering
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModelCapability {
    /// Text generation
    TextGeneration,
    /// Chat/conversation
    Chat,
    /// Text embeddings
    Embeddings,
    /// Code completion/generation
    Code,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn _assert_object_safe(_: &dyn ModelRegistryPort) {}

    #[test]
    fn trait_is_send_sync() {
        fn assert_send_sync<T: Send + Sync + ?Sized>() {}
        assert_send_sync::<dyn ModelRegistryPort>();
    }

    #[test]
    fn model_capabilities_default() {
        let caps = ModelCapabilities::default();
        assert!(!caps.text_generation);
        assert!(!caps.chat);
        assert!(!caps.embeddings);
        assert!(!caps.code);
        assert!(caps.context_length.is_none());
    }

    #[test]
    fn model_info_creation() {
        let model = ModelInfo {
            id: "llama3".to_string(),
            name: "LLaMA 3".to_string(),
            description: Some("Meta's LLaMA 3 model".to_string()),
            capabilities: ModelCapabilities {
                text_generation: true,
                chat: true,
                ..Default::default()
            },
            variant: Some("8B".to_string()),
            available: true,
        };

        assert_eq!(model.id, "llama3");
        assert!(model.available);
        assert!(model.capabilities.chat);
    }
}
