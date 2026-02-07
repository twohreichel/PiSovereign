//! Whisper.cpp Local Speech-to-Text Provider
//!
//! Implements `SpeechToText` using the whisper.cpp CLI for local transcription.
//!
//! # Prerequisites
//!
//! - whisper.cpp must be installed and available in PATH
//! - A GGML model file (e.g., ggml-base.bin, ggml-small.bin)
//!
//! # Installation on Raspberry Pi
//!
//! ```bash
//! # Clone and build whisper.cpp
//! git clone https://github.com/ggerganov/whisper.cpp
//! cd whisper.cpp
//! make -j4
//!
//! # Download a model (base is good for Pi)
//! ./models/download-ggml-model.sh base
//!
//! # Optional: Install system-wide
//! sudo cp main /usr/local/bin/whisper-cpp
//! ```

use std::path::Path;
use std::process::Stdio;

use async_trait::async_trait;
use tempfile::NamedTempFile;
use tokio::io::AsyncWriteExt;
use tokio::process::Command;
use tracing::{debug, error, instrument, warn};

use crate::config::LocalSttConfig;
use crate::error::SpeechError;
use crate::ports::SpeechToText;
use crate::types::{AudioData, AudioFormat, Transcription};

/// Local STT provider using whisper.cpp
#[derive(Debug, Clone)]
pub struct WhisperCppProvider {
    config: LocalSttConfig,
}

impl WhisperCppProvider {
    /// Create a new whisper.cpp provider
    ///
    /// # Arguments
    ///
    /// * `config` - Local STT configuration
    ///
    /// # Returns
    ///
    /// Returns the provider instance.
    ///
    /// # Errors
    ///
    /// Returns `SpeechError::Configuration` if the configuration is invalid.
    pub fn new(config: LocalSttConfig) -> Result<Self, SpeechError> {
        config.validate().map_err(SpeechError::Configuration)?;
        Ok(Self { config })
    }

    /// Get the whisper.cpp executable path
    fn executable(&self) -> &Path {
        &self.config.executable_path
    }

    /// Get the model path
    fn model(&self) -> &Path {
        &self.config.model_path
    }

    /// Run whisper.cpp on an audio file
    #[instrument(skip(self, audio_path), fields(model = %self.model().display()))]
    async fn run_whisper(&self, audio_path: &Path, language: Option<&str>) -> Result<String, SpeechError> {
        let mut cmd = Command::new(self.executable());

        cmd.arg("-m").arg(self.model())
            .arg("-f").arg(audio_path)
            .arg("--output-txt")
            .arg("--no-timestamps")
            .arg("-t").arg(self.config.threads.to_string());

        // Add language hint if provided
        if let Some(lang) = language {
            cmd.arg("-l").arg(lang);
        } else if let Some(ref default_lang) = self.config.default_language {
            cmd.arg("-l").arg(default_lang);
        }

        // Suppress output
        cmd.stdout(Stdio::piped())
            .stderr(Stdio::piped());

        debug!("Running whisper.cpp: {:?}", cmd);

        let output = cmd.output().await.map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                SpeechError::NotAvailable(format!(
                    "whisper.cpp not found at '{}'. Please install whisper.cpp.",
                    self.executable().display()
                ))
            } else {
                SpeechError::TranscriptionFailed(format!("Failed to run whisper.cpp: {e}"))
            }
        })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            error!("whisper.cpp failed: {}", stderr);
            return Err(SpeechError::TranscriptionFailed(format!(
                "whisper.cpp exited with status {}: {}",
                output.status,
                stderr.trim()
            )));
        }

        // Read the output text file
        let txt_path = audio_path.with_extension("txt");
        let text = tokio::fs::read_to_string(&txt_path).await.map_err(|e| {
            SpeechError::TranscriptionFailed(format!("Failed to read transcription output: {e}"))
        })?;

        // Clean up output file
        let _ = tokio::fs::remove_file(&txt_path).await;

        Ok(text.trim().to_string())
    }

    /// Write audio data to a temporary WAV file for whisper.cpp
    async fn write_temp_audio(&self, audio: &AudioData) -> Result<NamedTempFile, SpeechError> {
        let temp_file = NamedTempFile::with_suffix(".wav").map_err(|e| {
            SpeechError::TranscriptionFailed(format!("Failed to create temp file: {e}"))
        })?;

        // Write audio data
        let mut file = tokio::fs::File::create(temp_file.path()).await.map_err(|e| {
            SpeechError::TranscriptionFailed(format!("Failed to write temp file: {e}"))
        })?;

        file.write_all(audio.data()).await.map_err(|e| {
            SpeechError::TranscriptionFailed(format!("Failed to write audio data: {e}"))
        })?;

        file.flush().await.map_err(|e| {
            SpeechError::TranscriptionFailed(format!("Failed to flush temp file: {e}"))
        })?;

        Ok(temp_file)
    }
}

#[async_trait]
impl SpeechToText for WhisperCppProvider {
    #[instrument(skip(self, audio), fields(format = ?audio.format()))]
    async fn transcribe(&self, audio: AudioData) -> Result<Transcription, SpeechError> {
        self.transcribe_with_language(audio, self.config.default_language.as_deref().unwrap_or("")).await
    }

    #[instrument(skip(self, audio), fields(format = ?audio.format(), language = %language))]
    async fn transcribe_with_language(
        &self,
        audio: AudioData,
        language: &str,
    ) -> Result<Transcription, SpeechError> {
        debug!("Transcribing audio with whisper.cpp, format: {:?}", audio.format());

        // Whisper.cpp works best with WAV
        let needs_conversion = audio.format() != AudioFormat::Wav;
        
        let audio_to_process = if needs_conversion {
            // Use the audio converter to convert to WAV
            let converter = crate::AudioConverter::new();
            converter.convert(&audio, AudioFormat::Wav).await?
        } else {
            audio
        };

        // Write to temp file
        let temp_file = self.write_temp_audio(&audio_to_process).await?;

        // Run whisper.cpp
        let lang = if language.is_empty() { None } else { Some(language) };
        let text = self.run_whisper(temp_file.path(), lang).await?;

        // Temp file is automatically cleaned up when dropped

        if text.is_empty() {
            warn!("whisper.cpp returned empty transcription");
        }

        let mut transcription = Transcription::new(text);
        if !language.is_empty() {
            transcription = transcription.with_language(language);
        }
        Ok(transcription)
    }

    async fn is_available(&self) -> bool {
        // Check if executable exists and model file is present
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

        let model_exists = self.model().exists();

        debug!(
            "whisper.cpp availability: executable={}, model={}",
            executable_exists, model_exists
        );

        executable_exists && model_exists
    }

    fn model_name(&self) -> &str {
        self.model()
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("whisper.cpp")
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    fn test_config() -> LocalSttConfig {
        LocalSttConfig {
            executable_path: PathBuf::from("whisper-cpp"),
            model_path: PathBuf::from("/models/ggml-base.bin"),
            threads: 4,
            default_language: Some("en".to_string()),
        }
    }

    #[test]
    fn creates_provider_with_valid_config() {
        let config = test_config();
        let provider = WhisperCppProvider::new(config);
        assert!(provider.is_ok());
    }

    #[test]
    fn model_name_extracts_from_path() {
        let config = test_config();
        let provider = WhisperCppProvider::new(config).unwrap();
        assert_eq!(provider.model_name(), "ggml-base");
    }

    #[test]
    fn model_name_handles_complex_paths() {
        let mut config = test_config();
        config.model_path = PathBuf::from("/home/pi/models/ggml-small.en.bin");
        let provider = WhisperCppProvider::new(config).unwrap();
        assert_eq!(provider.model_name(), "ggml-small.en");
    }

    #[tokio::test]
    async fn is_available_returns_false_when_not_installed() {
        let mut config = test_config();
        config.executable_path = PathBuf::from("/nonexistent/whisper-cpp");
        let provider = WhisperCppProvider::new(config).unwrap();
        
        // Should return false since executable doesn't exist
        assert!(!provider.is_available().await);
    }
}
