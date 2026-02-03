//! Hailo-10H inference engine implementation
//!
//! Connects to hailo-ollama server which provides an OpenAI-compatible API.

mod client;
mod streaming;

pub use client::HailoInferenceEngine;
