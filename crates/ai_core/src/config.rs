//! Configuration for inference engine

use serde::{Deserialize, Serialize};

/// Configuration for the inference engine
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InferenceConfig {
    /// Base URL of the inference server (hailo-ollama)
    #[serde(default = "default_base_url")]
    pub base_url: String,

    /// Default model to use
    #[serde(default = "default_model")]
    pub default_model: String,

    /// Request timeout in milliseconds
    #[serde(default = "default_timeout_ms")]
    pub timeout_ms: u64,

    /// Maximum tokens to generate
    #[serde(default = "default_max_tokens")]
    pub max_tokens: u32,

    /// Temperature for sampling (0.0 - 2.0)
    #[serde(default = "default_temperature")]
    pub temperature: f32,

    /// Top-p (nucleus) sampling
    #[serde(default = "default_top_p")]
    pub top_p: f32,

    /// System prompt to use by default
    #[serde(default)]
    pub system_prompt: Option<String>,
}

fn default_base_url() -> String {
    "http://localhost:11434".to_string()
}

fn default_model() -> String {
    "qwen2.5-1.5b-instruct".to_string()
}

const fn default_timeout_ms() -> u64 {
    60000 // 60 seconds
}

const fn default_max_tokens() -> u32 {
    2048
}

const fn default_temperature() -> f32 {
    0.7
}

const fn default_top_p() -> f32 {
    0.9
}

impl Default for InferenceConfig {
    fn default() -> Self {
        Self {
            base_url: default_base_url(),
            default_model: default_model(),
            timeout_ms: default_timeout_ms(),
            max_tokens: default_max_tokens(),
            temperature: default_temperature(),
            top_p: default_top_p(),
            system_prompt: None,
        }
    }
}

impl InferenceConfig {
    /// Create config for Hailo-10H with qwen2.5-1.5b-instruct
    pub fn hailo_qwen() -> Self {
        Self {
            default_model: "qwen2.5-1.5b-instruct".to_string(),
            ..Default::default()
        }
    }

    /// Create config for Hailo-10H with llama3.2-1b-instruct
    pub fn hailo_llama() -> Self {
        Self {
            default_model: "llama3.2-1b-instruct".to_string(),
            ..Default::default()
        }
    }

    /// Create config for function-calling model
    pub fn hailo_function_calling() -> Self {
        Self {
            default_model: "qwen2-1.5b-function-calling".to_string(),
            temperature: 0.1, // Lower temp for structured output
            ..Default::default()
        }
    }
}
