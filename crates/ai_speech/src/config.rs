//! Configuration for speech processing

use std::collections::HashMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::types::AudioFormat;

/// Configuration for speech processing services
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpeechConfig {
    /// Speech provider to use
    #[serde(default = "default_provider")]
    pub provider: SpeechProvider,

    /// OpenAI API key (for OpenAI provider)
    #[serde(default)]
    pub openai_api_key: Option<String>,

    /// OpenAI API base URL (for custom endpoints)
    #[serde(default = "default_openai_base_url")]
    pub openai_base_url: String,

    /// Speech-to-text model
    #[serde(default = "default_stt_model")]
    pub stt_model: String,

    /// Text-to-speech model
    #[serde(default = "default_tts_model")]
    pub tts_model: String,

    /// Default voice for TTS
    #[serde(default = "default_voice")]
    pub default_voice: String,

    /// Output audio format for TTS
    #[serde(default = "default_output_format")]
    pub output_format: AudioFormat,

    /// Request timeout in milliseconds
    #[serde(default = "default_timeout_ms")]
    pub timeout_ms: u64,

    /// Maximum audio duration in milliseconds
    #[serde(default = "default_max_audio_duration_ms")]
    pub max_audio_duration_ms: u64,

    /// Whether to include transcription in response
    #[serde(default = "default_include_transcription")]
    pub include_transcription: bool,

    /// Response format preference
    #[serde(default)]
    pub response_format: ResponseFormatPreference,

    /// TTS speaking speed (0.25 to 4.0)
    #[serde(default = "default_speed")]
    pub speed: f32,
}

/// Speech provider selection
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum SpeechProvider {
    /// OpenAI Whisper and TTS (cloud)
    OpenAI,
    /// Local processing (whisper.cpp + Piper)
    Local,
    /// Hybrid: local first, cloud fallback
    #[default]
    Hybrid,
}

/// Configuration for local STT (whisper.cpp)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocalSttConfig {
    /// Path to whisper.cpp executable
    #[serde(default = "default_whisper_executable")]
    pub executable_path: PathBuf,

    /// Path to GGML model file
    #[serde(default = "default_whisper_model")]
    pub model_path: PathBuf,

    /// Number of threads to use
    #[serde(default = "default_threads")]
    pub threads: u32,

    /// Default language hint (ISO 639-1)
    #[serde(default)]
    pub default_language: Option<String>,
}

impl Default for LocalSttConfig {
    fn default() -> Self {
        Self {
            executable_path: default_whisper_executable(),
            model_path: default_whisper_model(),
            threads: default_threads(),
            default_language: Some("de".to_string()),
        }
    }
}

impl LocalSttConfig {
    /// Validate the configuration
    pub fn validate(&self) -> Result<(), String> {
        // Model path should end with .bin
        if let Some(ext) = self.model_path.extension() {
            if ext != "bin" {
                return Err("Model path should point to a .bin GGML model file".to_string());
            }
        }

        if self.threads == 0 {
            return Err("Threads must be greater than 0".to_string());
        }

        Ok(())
    }
}

/// Configuration for local TTS (Piper)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocalTtsConfig {
    /// Path to Piper executable
    #[serde(default = "default_piper_executable")]
    pub executable_path: PathBuf,

    /// Path to default voice model (.onnx)
    #[serde(default = "default_piper_model")]
    pub default_model_path: PathBuf,

    /// Default voice name
    #[serde(default = "default_piper_voice")]
    pub default_voice: String,

    /// Map of voice names to model paths
    #[serde(default)]
    pub voices: HashMap<String, PathBuf>,

    /// Output audio format
    #[serde(default = "default_output_format")]
    pub output_format: AudioFormat,

    /// Speaking rate (1.0 = normal)
    #[serde(default = "default_length_scale")]
    pub length_scale: f32,

    /// Silence between sentences in seconds
    #[serde(default = "default_sentence_silence")]
    pub sentence_silence: f32,
}

impl Default for LocalTtsConfig {
    fn default() -> Self {
        Self {
            executable_path: default_piper_executable(),
            default_model_path: default_piper_model(),
            default_voice: default_piper_voice(),
            voices: HashMap::new(),
            output_format: default_output_format(),
            length_scale: default_length_scale(),
            sentence_silence: default_sentence_silence(),
        }
    }
}

impl LocalTtsConfig {
    /// Validate the configuration
    pub fn validate(&self) -> Result<(), String> {
        // Model path should end with .onnx
        if let Some(ext) = self.default_model_path.extension() {
            if ext != "onnx" {
                return Err("Model path should point to an .onnx voice model file".to_string());
            }
        }

        if self.length_scale <= 0.0 || self.length_scale > 4.0 {
            return Err(format!(
                "Length scale must be between 0.0 and 4.0, got {}",
                self.length_scale
            ));
        }

        if self.sentence_silence < 0.0 {
            return Err("Sentence silence cannot be negative".to_string());
        }

        Ok(())
    }
}

/// Configuration for hybrid mode
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HybridConfig {
    /// Prefer local processing over cloud
    #[serde(default = "default_prefer_local")]
    pub prefer_local: bool,

    /// Allow fallback to cloud if local fails
    #[serde(default = "default_allow_cloud_fallback")]
    pub allow_cloud_fallback: bool,

    /// Maximum retries for local provider before fallback
    #[serde(default = "default_local_retries")]
    pub local_retries: u32,
}

impl Default for HybridConfig {
    fn default() -> Self {
        Self {
            prefer_local: default_prefer_local(),
            allow_cloud_fallback: default_allow_cloud_fallback(),
            local_retries: default_local_retries(),
        }
    }
}

// Default functions for local configs

fn default_whisper_executable() -> PathBuf {
    PathBuf::from("whisper-cpp")
}

fn default_whisper_model() -> PathBuf {
    // Common location on Raspberry Pi
    PathBuf::from("/usr/local/share/whisper/ggml-base.bin")
}

fn default_threads() -> u32 {
    4 // Good default for Raspberry Pi 5
}

fn default_piper_executable() -> PathBuf {
    PathBuf::from("piper")
}

fn default_piper_model() -> PathBuf {
    PathBuf::from("/usr/local/share/piper/voices/de_DE-thorsten-medium.onnx")
}

fn default_piper_voice() -> String {
    "de_DE-thorsten-medium".to_string()
}

fn default_length_scale() -> f32 {
    1.0
}

fn default_sentence_silence() -> f32 {
    0.2
}

const fn default_prefer_local() -> bool {
    true // Local first by default
}

const fn default_allow_cloud_fallback() -> bool {
    true // Allow fallback by default
}

const fn default_local_retries() -> u32 {
    1
}

/// Preference for how the bot should respond
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum ResponseFormatPreference {
    /// Mirror the input format (voice → voice, text → text)
    #[default]
    Mirror,
    /// Always respond with text
    Text,
    /// Always respond with voice
    Voice,
}

const fn default_provider() -> SpeechProvider {
    SpeechProvider::OpenAI
}

fn default_openai_base_url() -> String {
    "https://api.openai.com/v1".to_string()
}

fn default_stt_model() -> String {
    "whisper-1".to_string()
}

fn default_tts_model() -> String {
    "tts-1".to_string()
}

fn default_voice() -> String {
    "nova".to_string()
}

const fn default_output_format() -> AudioFormat {
    AudioFormat::Opus
}

const fn default_timeout_ms() -> u64 {
    30000 // 30 seconds
}

const fn default_max_audio_duration_ms() -> u64 {
    120_000 // 2 minutes
}

const fn default_include_transcription() -> bool {
    true
}

const fn default_speed() -> f32 {
    1.0
}

impl Default for SpeechConfig {
    fn default() -> Self {
        Self {
            provider: default_provider(),
            openai_api_key: None,
            openai_base_url: default_openai_base_url(),
            stt_model: default_stt_model(),
            tts_model: default_tts_model(),
            default_voice: default_voice(),
            output_format: default_output_format(),
            timeout_ms: default_timeout_ms(),
            max_audio_duration_ms: default_max_audio_duration_ms(),
            include_transcription: default_include_transcription(),
            response_format: ResponseFormatPreference::default(),
            speed: default_speed(),
        }
    }
}

impl SpeechConfig {
    /// Create a minimal config for testing
    #[cfg(test)]
    pub fn test() -> Self {
        Self {
            openai_api_key: Some("test-key".to_string()),
            ..Default::default()
        }
    }

    /// Validate the configuration
    ///
    /// # Errors
    ///
    /// Returns an error if the configuration is invalid.
    pub fn validate(&self) -> Result<(), String> {
        // Check API key for OpenAI provider
        if self.provider == SpeechProvider::OpenAI && self.openai_api_key.is_none() {
            return Err("OpenAI API key is required for OpenAI provider".to_string());
        }

        // Validate speed range
        if !(0.25..=4.0).contains(&self.speed) {
            return Err(format!(
                "Speed must be between 0.25 and 4.0, got {}",
                self.speed
            ));
        }

        // Validate timeout
        if self.timeout_ms == 0 {
            return Err("Timeout must be greater than 0".to_string());
        }

        // Validate max audio duration
        if self.max_audio_duration_ms == 0 {
            return Err("Max audio duration must be greater than 0".to_string());
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_has_expected_values() {
        let config = SpeechConfig::default();

        assert_eq!(config.provider, SpeechProvider::OpenAI);
        assert!(config.openai_api_key.is_none());
        assert_eq!(config.openai_base_url, "https://api.openai.com/v1");
        assert_eq!(config.stt_model, "whisper-1");
        assert_eq!(config.tts_model, "tts-1");
        assert_eq!(config.default_voice, "nova");
        assert_eq!(config.output_format, AudioFormat::Opus);
        assert_eq!(config.timeout_ms, 30000);
        assert_eq!(config.max_audio_duration_ms, 120000);
        assert!(config.include_transcription);
        assert_eq!(config.response_format, ResponseFormatPreference::Mirror);
        assert!((config.speed - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_config_has_api_key() {
        let config = SpeechConfig::test();
        assert_eq!(config.openai_api_key, Some("test-key".to_string()));
    }

    #[test]
    fn validate_fails_without_api_key() {
        let config = SpeechConfig::default();
        assert!(config.validate().is_err());
    }

    #[test]
    fn validate_succeeds_with_api_key() {
        let config = SpeechConfig::test();
        assert!(config.validate().is_ok());
    }

    #[test]
    fn validate_fails_with_invalid_speed() {
        let mut config = SpeechConfig::test();
        config.speed = 0.1; // Below minimum
        assert!(config.validate().is_err());

        config.speed = 5.0; // Above maximum
        assert!(config.validate().is_err());
    }

    #[test]
    fn validate_fails_with_zero_timeout() {
        let mut config = SpeechConfig::test();
        config.timeout_ms = 0;
        assert!(config.validate().is_err());
    }

    #[test]
    fn validate_fails_with_zero_max_duration() {
        let mut config = SpeechConfig::test();
        config.max_audio_duration_ms = 0;
        assert!(config.validate().is_err());
    }

    #[test]
    fn speech_provider_serializes_lowercase() {
        let openai = serde_json::to_string(&SpeechProvider::OpenAI).unwrap();
        let local = serde_json::to_string(&SpeechProvider::Local).unwrap();

        assert_eq!(openai, "\"openai\"");
        assert_eq!(local, "\"local\"");
    }

    #[test]
    fn response_format_preference_serializes_lowercase() {
        let mirror = serde_json::to_string(&ResponseFormatPreference::Mirror).unwrap();
        let text = serde_json::to_string(&ResponseFormatPreference::Text).unwrap();
        let voice = serde_json::to_string(&ResponseFormatPreference::Voice).unwrap();

        assert_eq!(mirror, "\"mirror\"");
        assert_eq!(text, "\"text\"");
        assert_eq!(voice, "\"voice\"");
    }

    #[test]
    fn config_deserializes_from_toml() {
        let toml = r#"
            provider = "openai"
            openai_api_key = "sk-test"
            stt_model = "whisper-1"
            tts_model = "tts-1-hd"
            default_voice = "alloy"
            output_format = "mp3"
            timeout_ms = 60000
            max_audio_duration_ms = 180000
            include_transcription = false
            response_format = "text"
            speed = 1.25
        "#;

        let config: SpeechConfig = toml::from_str(toml).unwrap();

        assert_eq!(config.provider, SpeechProvider::OpenAI);
        assert_eq!(config.openai_api_key, Some("sk-test".to_string()));
        assert_eq!(config.tts_model, "tts-1-hd");
        assert_eq!(config.default_voice, "alloy");
        assert_eq!(config.output_format, AudioFormat::Mp3);
        assert_eq!(config.timeout_ms, 60000);
        assert!(!config.include_transcription);
        assert_eq!(config.response_format, ResponseFormatPreference::Text);
        assert!((config.speed - 1.25).abs() < f32::EPSILON);
    }
}
