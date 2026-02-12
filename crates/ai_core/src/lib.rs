#![forbid(unsafe_code)]
//! AI Core - Inference engine and model management
//!
//! Provides abstractions for LLM inference with Ollama-compatible backends.
//! Supports standard Ollama (macOS/Linux) and hailo-ollama (Raspberry Pi with Hailo NPU).

pub mod config;
pub mod error;
pub mod ollama;
pub mod ports;
pub mod selector;

pub use config::InferenceConfig;
pub use error::InferenceError;
pub use ollama::{EmbeddingConfig, EmbeddingEngine, OllamaEmbeddingEngine, OllamaInferenceEngine};
pub use ports::{InferenceEngine, InferenceRequest, InferenceResponse, StreamingChunk};
pub use selector::{ModelSelector, ModelSelectorConfig, TaskComplexity};
