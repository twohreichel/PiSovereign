//! Ollama-compatible inference engine implementation
//!
//! Connects to any Ollama-compatible server (standard Ollama, hailo-ollama, etc.)
//! which provides an OpenAI-compatible chat API.

mod client;
mod embedding;
mod streaming;

pub use client::OllamaInferenceEngine;
pub use embedding::{EmbeddingConfig, EmbeddingEngine, OllamaEmbeddingEngine};
