//! AI Core - Inference engine and model management
//!
//! Provides abstractions for LLM inference with Hailo-10H integration.
//! Uses the hailo-ollama server which exposes an OpenAI-compatible API.

pub mod config;
pub mod error;
pub mod hailo;
pub mod ports;
pub mod selector;

pub use config::InferenceConfig;
pub use error::InferenceError;
pub use hailo::HailoInferenceEngine;
pub use ports::{InferenceEngine, InferenceRequest, InferenceResponse, StreamingChunk};
pub use selector::{ModelSelector, ModelSelectorConfig, TaskComplexity};
