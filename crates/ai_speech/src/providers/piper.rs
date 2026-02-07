//! Piper Local Text-to-Speech Provider
//!
//! Implements `TextToSpeech` using the Piper CLI for local speech synthesis.
//!
//! # Prerequisites
//!
//! - Piper must be installed and available in PATH
//! - A voice model (.onnx) and config (.json) file
//!
//! # Installation on Raspberry Pi
//!
//! ```bash
//! # Download Piper release
//! wget https://github.com/rhasspy/piper/releases/download/v1.2.0/piper_arm64.tar.gz
//! tar -xzf piper_arm64.tar.gz
//! sudo mv piper /usr/local/bin/
//!
//! # Download a voice (e.g., German)
//! mkdir -p ~/.local/share/piper/voices
//! cd ~/.local/share/piper/voices
//! wget https://huggingface.co/rhasspy/piper-voices/resolve/main/de/de_DE/thorsten/medium/de_DE-thorsten-medium.onnx
//! wget https://huggingface.co/rhasspy/piper-voices/resolve/main/de/de_DE/thorsten/medium/de_DE-thorsten-medium.onnx.json
//! ```
//!
//! # Recommended Voices
//!
//! | Language | Voice | Quality | Notes |
//! |----------|-------|---------|-------|
//! | German | de_DE-thorsten-medium | Good | Natural German male voice |
//! | English | en_US-lessac-medium | Good | Clear American English |
//! | English | en_GB-alan-medium | Good | British English |

use std::path::{Path, PathBuf};
use std::process::Stdio;

use async_trait::async_trait;
use tempfile::NamedTempFile;
use tokio::process::Command;
use tracing::{debug, error, instrument, warn};

use crate::config::LocalTtsConfig;
use crate::error::SpeechError;
use crate::ports::TextToSpeech;
use crate::types::{AudioData, AudioFormat, VoiceInfo};

/// Local TTS provider using Piper
#[derive(Debug, Clone)]
pub struct PiperProvider {
    config: LocalTtsConfig,
}

impl PiperProvider {
    /// Create a new Piper provider
    ///
    /// # Arguments
    ///
    /// * `config` - Local TTS configuration
    ///
    /// # Returns
    ///
    /// Returns the provider instance.
    ///
    /// # Errors
    ///
    /// Returns `SpeechError::Configuration` if the configuration is invalid.
    pub fn new(config: LocalTtsConfig) -> Result<Self, SpeechError> {
        config.validate().map_err(SpeechError::Configuration)?;
        Ok(Self { config })
    }

    /// Get the Piper executable path
    fn executable(&self) -> &Path {
        &self.config.executable_path
    }

    /// Get the voice model path for a voice name
    fn voice_model_path(&self, voice: Option<&str>) -> &Path {
        // Use specified voice or default
        let voice_name = voice.unwrap_or(&self.config.default_voice);

        // Look up in voice map or use default path
        self.config
            .voices
            .get(voice_name)
            .map_or(&self.config.default_model_path, PathBuf::as_path)
    }

    /// Run Piper to synthesize speech
    #[instrument(skip(self, text), fields(voice = ?voice, text_len = text.len()))]
    async fn run_piper(&self, text: &str, voice: Option<&str>) -> Result<Vec<u8>, SpeechError> {
        let model_path = self.voice_model_path(voice);

        // Create temp file for output
        let output_file = NamedTempFile::with_suffix(".wav").map_err(|e| {
            SpeechError::SynthesisFailed(format!("Failed to create temp file: {e}"))
        })?;

        let mut cmd = Command::new(self.executable());

        cmd.arg("--model")
            .arg(model_path)
            .arg("--output_file")
            .arg(output_file.path())
            .arg("--length_scale")
            .arg(self.config.length_scale.to_string())
            .arg("--sentence_silence")
            .arg(self.config.sentence_silence.to_string())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        debug!("Running piper: {:?}", cmd);

        let mut child = cmd.spawn().map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                SpeechError::NotAvailable(format!(
                    "Piper not found at '{}'. Please install Piper.",
                    self.executable().display()
                ))
            } else {
                SpeechError::SynthesisFailed(format!("Failed to run piper: {e}"))
            }
        })?;

        // Write text to stdin
        if let Some(mut stdin) = child.stdin.take() {
            use tokio::io::AsyncWriteExt;
            stdin.write_all(text.as_bytes()).await.map_err(|e| {
                SpeechError::SynthesisFailed(format!("Failed to write to piper stdin: {e}"))
            })?;
            // stdin is dropped here, closing it
        }

        let output = child
            .wait_with_output()
            .await
            .map_err(|e| SpeechError::SynthesisFailed(format!("Failed to wait for piper: {e}")))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            error!("Piper failed: {}", stderr);
            return Err(SpeechError::SynthesisFailed(format!(
                "Piper exited with status {}: {}",
                output.status,
                stderr.trim()
            )));
        }

        // Read the output WAV file
        let audio_data = tokio::fs::read(output_file.path()).await.map_err(|e| {
            SpeechError::SynthesisFailed(format!("Failed to read piper output: {e}"))
        })?;

        if audio_data.is_empty() {
            warn!("Piper produced empty output");
            return Err(SpeechError::SynthesisFailed(
                "Piper produced empty output".to_string(),
            ));
        }

        Ok(audio_data)
    }

    /// Convert WAV to the requested format
    async fn convert_format(
        &self,
        wav_data: Vec<u8>,
        format: AudioFormat,
    ) -> Result<Vec<u8>, SpeechError> {
        if format == AudioFormat::Wav {
            return Ok(wav_data);
        }

        // Use the audio converter
        let audio = AudioData::new(wav_data, AudioFormat::Wav);
        let converter = crate::AudioConverter::new();
        let result = converter.convert(&audio, format).await?;

        Ok(result.into_data())
    }
}

#[async_trait]
impl TextToSpeech for PiperProvider {
    #[instrument(skip(self, text), fields(text_len = text.len()))]
    async fn synthesize(&self, text: &str, voice: Option<&str>) -> Result<AudioData, SpeechError> {
        self.synthesize_with_format(text, voice, self.config.output_format)
            .await
    }

    #[instrument(skip(self, text), fields(text_len = text.len(), format = ?format))]
    async fn synthesize_with_format(
        &self,
        text: &str,
        voice: Option<&str>,
        format: AudioFormat,
    ) -> Result<AudioData, SpeechError> {
        if text.is_empty() {
            return Err(SpeechError::SynthesisFailed(
                "Cannot synthesize empty text".to_string(),
            ));
        }

        debug!("Synthesizing {} chars with Piper", text.len());

        // Run piper to get WAV
        let wav_data = self.run_piper(text, voice).await?;

        // Convert to requested format
        let audio_data = self.convert_format(wav_data, format).await?;

        Ok(AudioData::new(audio_data, format))
    }

    async fn list_voices(&self) -> Result<Vec<VoiceInfo>, SpeechError> {
        // Return configured voices
        let mut voices: Vec<VoiceInfo> = self
            .config
            .voices
            .keys()
            .map(|name| {
                // Parse voice info from name (e.g., "de_DE-thorsten-medium")
                let parts: Vec<&str> = name.split('-').collect();
                let locale = parts.first().copied().unwrap_or("unknown");
                let speaker = parts.get(1).copied().unwrap_or("default");

                let mut voice = VoiceInfo::new(name.clone(), format!("{speaker} ({locale})"));
                voice.languages = vec![locale.replace('_', "-")];
                voice.description = Some(format!("Piper voice: {name}"));
                voice
            })
            .collect();

        // Add default voice if not already in list
        if !voices.iter().any(|v| v.id == self.config.default_voice) {
            let mut default_voice =
                VoiceInfo::new(self.config.default_voice.clone(), "Default".to_string());
            default_voice.languages = vec!["en".to_string()];
            default_voice.description = Some("Default Piper voice".to_string());
            voices.push(default_voice);
        }

        Ok(voices)
    }

    async fn is_available(&self) -> bool {
        // Check if executable exists and default model is present
        let executable_exists = self.executable().exists() || {
            // Try to find in PATH
            Command::new(self.executable())
                .arg("--help")
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status()
                .await
                .map(|s| s.success())
                .unwrap_or(false)
        };

        let model_exists = self.config.default_model_path.exists();

        debug!(
            "Piper availability: executable={}, model={}",
            executable_exists, model_exists
        );

        executable_exists && model_exists
    }

    fn model_name(&self) -> &str {
        self.config
            .default_model_path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("piper")
    }

    fn default_voice(&self) -> &str {
        &self.config.default_voice
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::path::PathBuf;

    use super::*;

    fn test_config() -> LocalTtsConfig {
        let mut voices = HashMap::new();
        voices.insert(
            "de_DE-thorsten-medium".to_string(),
            PathBuf::from("/models/de_DE-thorsten-medium.onnx"),
        );
        voices.insert(
            "en_US-lessac-medium".to_string(),
            PathBuf::from("/models/en_US-lessac-medium.onnx"),
        );

        LocalTtsConfig {
            executable_path: PathBuf::from("piper"),
            default_model_path: PathBuf::from("/models/de_DE-thorsten-medium.onnx"),
            default_voice: "de_DE-thorsten-medium".to_string(),
            voices,
            output_format: AudioFormat::Wav,
            length_scale: 1.0,
            sentence_silence: 0.2,
        }
    }

    #[test]
    fn creates_provider_with_valid_config() {
        let config = test_config();
        let provider = PiperProvider::new(config);
        assert!(provider.is_ok());
    }

    #[test]
    fn model_name_extracts_from_path() {
        let config = test_config();
        let provider = PiperProvider::new(config).unwrap();
        assert_eq!(provider.model_name(), "de_DE-thorsten-medium");
    }

    #[test]
    fn default_voice_returns_configured_voice() {
        let config = test_config();
        let provider = PiperProvider::new(config).unwrap();
        assert_eq!(provider.default_voice(), "de_DE-thorsten-medium");
    }

    #[test]
    fn voice_model_path_uses_default_when_none() {
        let config = test_config();
        let provider = PiperProvider::new(config).unwrap();
        let path = provider.voice_model_path(None);
        assert_eq!(path, Path::new("/models/de_DE-thorsten-medium.onnx"));
    }

    #[test]
    fn voice_model_path_looks_up_voice() {
        let config = test_config();
        let provider = PiperProvider::new(config).unwrap();
        let path = provider.voice_model_path(Some("en_US-lessac-medium"));
        assert_eq!(path, Path::new("/models/en_US-lessac-medium.onnx"));
    }

    #[tokio::test]
    async fn list_voices_returns_configured_voices() {
        let config = test_config();
        let provider = PiperProvider::new(config).unwrap();
        let voices = provider.list_voices().await.unwrap();

        assert!(voices.len() >= 2);
        assert!(voices.iter().any(|v| v.id == "de_DE-thorsten-medium"));
        assert!(voices.iter().any(|v| v.id == "en_US-lessac-medium"));
    }

    #[tokio::test]
    async fn is_available_returns_false_when_not_installed() {
        let mut config = test_config();
        config.executable_path = PathBuf::from("/nonexistent/piper");
        let provider = PiperProvider::new(config).unwrap();

        assert!(!provider.is_available().await);
    }
}
