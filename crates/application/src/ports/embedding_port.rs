//! Embedding port - Interface for generating vector embeddings
//!
//! This port defines how the application layer requests text embeddings
//! from an embedding model (e.g., via Ollama).

use async_trait::async_trait;
#[cfg(test)]
use mockall::automock;

use crate::error::ApplicationError;

/// Information about the embedding model
#[derive(Debug, Clone)]
pub struct EmbeddingModelInfo {
    /// Model identifier (e.g., "nomic-embed-text")
    pub model: String,
    /// Number of dimensions in the embedding vector
    pub dimensions: usize,
    /// Maximum input tokens
    pub max_tokens: Option<usize>,
}

/// Port for generating text embeddings
#[cfg_attr(test, automock)]
#[async_trait]
pub trait EmbeddingPort: Send + Sync {
    /// Generate an embedding for a single text
    ///
    /// Returns a vector of f32 values representing the text in embedding space.
    async fn embed(&self, text: &str) -> Result<Vec<f32>, ApplicationError>;

    /// Generate embeddings for multiple texts in a batch
    ///
    /// More efficient than calling `embed` multiple times.
    /// Returns embeddings in the same order as input texts.
    async fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, ApplicationError>;

    /// Get information about the embedding model
    fn model_info(&self) -> EmbeddingModelInfo;

    /// Calculate cosine similarity between two embeddings
    ///
    /// Default implementation provided - can be overridden for optimization.
    fn cosine_similarity(&self, a: &[f32], b: &[f32]) -> f32 {
        domain::cosine_similarity(a, b)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TestEmbedding;

    #[async_trait]
    impl EmbeddingPort for TestEmbedding {
        async fn embed(&self, _text: &str) -> Result<Vec<f32>, ApplicationError> {
            Ok(vec![0.1, 0.2, 0.3])
        }

        async fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, ApplicationError> {
            Ok(texts.iter().map(|_| vec![0.1, 0.2, 0.3]).collect())
        }

        fn model_info(&self) -> EmbeddingModelInfo {
            EmbeddingModelInfo {
                model: "test".to_string(),
                dimensions: 3,
                max_tokens: None,
            }
        }
    }

    #[test]
    fn cosine_similarity_identical_vectors() {
        let embedding = TestEmbedding;
        let vec = vec![1.0, 0.0, 0.0];
        let similarity = embedding.cosine_similarity(&vec, &vec);
        assert!((similarity - 1.0).abs() < 0.001);
    }

    #[test]
    fn cosine_similarity_orthogonal_vectors() {
        let embedding = TestEmbedding;
        let a = vec![1.0, 0.0, 0.0];
        let b = vec![0.0, 1.0, 0.0];
        let similarity = embedding.cosine_similarity(&a, &b);
        assert!(similarity.abs() < 0.001);
    }

    #[test]
    fn cosine_similarity_opposite_vectors() {
        let embedding = TestEmbedding;
        let a = vec![1.0, 0.0, 0.0];
        let b = vec![-1.0, 0.0, 0.0];
        let similarity = embedding.cosine_similarity(&a, &b);
        assert!((similarity + 1.0).abs() < 0.001);
    }

    #[test]
    fn cosine_similarity_different_lengths() {
        let embedding = TestEmbedding;
        let a = vec![1.0, 0.0];
        let b = vec![1.0, 0.0, 0.0];
        let similarity = embedding.cosine_similarity(&a, &b);
        assert!(similarity.abs() < f32::EPSILON);
    }

    #[test]
    fn cosine_similarity_empty_vectors() {
        let embedding = TestEmbedding;
        let a: Vec<f32> = vec![];
        let b: Vec<f32> = vec![];
        let similarity = embedding.cosine_similarity(&a, &b);
        assert!(similarity.abs() < f32::EPSILON);
    }

    #[test]
    fn cosine_similarity_zero_vector() {
        let embedding = TestEmbedding;
        let a = vec![0.0, 0.0, 0.0];
        let b = vec![1.0, 0.0, 0.0];
        let similarity = embedding.cosine_similarity(&a, &b);
        assert!(similarity.abs() < f32::EPSILON);
    }

    #[test]
    fn model_info_accessible() {
        let embedding = TestEmbedding;
        let info = embedding.model_info();
        assert_eq!(info.model, "test");
        assert_eq!(info.dimensions, 3);
    }
}
