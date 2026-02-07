//! Audio format converter for speech processing
//!
//! Provides functionality to convert audio between formats, particularly
//! for converting WhatsApp's OGG/Opus format to formats supported by
//! OpenAI's Whisper API.

use std::process::Stdio;

use tokio::io::AsyncWriteExt;
use tokio::process::Command;
use tracing::{debug, instrument};

use crate::error::SpeechError;
use crate::types::{AudioData, AudioFormat};

/// Audio converter for transforming between audio formats
///
/// Uses FFmpeg for audio conversion. FFmpeg must be installed on the system.
#[derive(Debug, Clone, Default)]
pub struct AudioConverter {
    /// FFmpeg binary path (defaults to "ffmpeg" in PATH)
    ffmpeg_path: Option<String>,
}

impl AudioConverter {
    /// Create a new audio converter with default settings
    #[must_use]
    pub const fn new() -> Self {
        Self { ffmpeg_path: None }
    }

    /// Create a new audio converter with a custom FFmpeg path
    #[must_use]
    pub fn with_ffmpeg_path(path: impl Into<String>) -> Self {
        Self {
            ffmpeg_path: Some(path.into()),
        }
    }

    /// Get the FFmpeg binary path
    fn ffmpeg_path(&self) -> &str {
        self.ffmpeg_path.as_deref().unwrap_or("ffmpeg")
    }

    /// Check if FFmpeg is available on the system
    #[instrument(skip(self))]
    pub async fn is_available(&self) -> bool {
        Command::new(self.ffmpeg_path())
            .arg("-version")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .await
            .is_ok_and(|status| status.success())
    }

    /// Convert audio data to the target format
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - FFmpeg is not available
    /// - The conversion fails
    /// - The input/output formats are not supported
    #[instrument(skip(self, audio), fields(
        input_format = %audio.format(),
        target_format = %target_format
    ))]
    pub async fn convert(
        &self,
        audio: &AudioData,
        target_format: AudioFormat,
    ) -> Result<AudioData, SpeechError> {
        // If already in target format, return a clone
        if audio.format() == target_format {
            debug!("Audio already in target format, skipping conversion");
            return Ok(audio.clone());
        }

        debug!(
            "Converting audio from {} to {}",
            audio.format(),
            target_format
        );

        // Build FFmpeg command
        // -i pipe:0 reads from stdin
        // -f <format> specifies output format
        // pipe:1 writes to stdout
        let mut cmd = Command::new(self.ffmpeg_path());
        cmd.arg("-i")
            .arg("pipe:0")
            .arg("-f")
            .arg(Self::format_to_ffmpeg(target_format))
            .arg("-y") // Overwrite output
            .arg("-loglevel")
            .arg("error")
            .arg("pipe:1")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        // Add format-specific options
        Self::add_format_options(&mut cmd, target_format);

        let mut child = cmd
            .spawn()
            .map_err(|e| SpeechError::AudioProcessing(format!("Failed to spawn FFmpeg: {e}")))?;

        // Write input data to stdin
        if let Some(mut stdin) = child.stdin.take() {
            stdin.write_all(audio.data()).await.map_err(|e| {
                SpeechError::AudioProcessing(format!("Failed to write to FFmpeg stdin: {e}"))
            })?;
            // Drop stdin to signal EOF
            drop(stdin);
        }

        // Wait for completion and read output
        let output = child
            .wait_with_output()
            .await
            .map_err(|e| SpeechError::AudioProcessing(format!("Failed to wait for FFmpeg: {e}")))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(SpeechError::AudioProcessing(format!(
                "FFmpeg conversion failed: {stderr}"
            )));
        }

        if output.stdout.is_empty() {
            return Err(SpeechError::AudioProcessing(
                "FFmpeg produced empty output".to_string(),
            ));
        }

        debug!(
            "Conversion successful, output size: {} bytes",
            output.stdout.len()
        );

        Ok(AudioData::new(output.stdout, target_format))
    }

    /// Convert audio to a Whisper-compatible format
    ///
    /// Converts the audio to MP3 if it's not already in a Whisper-supported format.
    ///
    /// # Errors
    ///
    /// Returns an error if the conversion fails
    #[instrument(skip(self, audio), fields(input_format = %audio.format()))]
    pub async fn convert_for_whisper(&self, audio: &AudioData) -> Result<AudioData, SpeechError> {
        if audio.format().is_whisper_supported() {
            debug!(
                "Audio format {} is already Whisper-compatible",
                audio.format()
            );
            return Ok(audio.clone());
        }

        // Convert to MP3 as it's widely supported and has good compression
        self.convert(audio, AudioFormat::Mp3).await
    }

    /// Get the FFmpeg format name for an audio format
    const fn format_to_ffmpeg(format: AudioFormat) -> &'static str {
        match format {
            AudioFormat::Opus => "opus",
            AudioFormat::Ogg => "ogg",
            AudioFormat::Mp3 => "mp3",
            AudioFormat::Wav => "wav",
            AudioFormat::Flac => "flac",
            AudioFormat::Webm => "webm",
            AudioFormat::M4a => "ipod", // FFmpeg uses "ipod" for m4a
        }
    }

    /// Add format-specific encoding options
    fn add_format_options(cmd: &mut Command, format: AudioFormat) {
        match format {
            AudioFormat::Mp3 => {
                // Use good quality for speech
                cmd.args(["-codec:a", "libmp3lame", "-q:a", "2"]);
            },
            AudioFormat::Opus => {
                // Optimize for speech
                cmd.args(["-codec:a", "libopus", "-application", "voip", "-b:a", "32k"]);
            },
            AudioFormat::Wav => {
                // PCM 16-bit, mono, 16kHz for speech processing
                cmd.args(["-codec:a", "pcm_s16le", "-ar", "16000", "-ac", "1"]);
            },
            AudioFormat::Flac => {
                // Lossless compression
                cmd.args(["-codec:a", "flac", "-compression_level", "5"]);
            },
            AudioFormat::M4a => {
                // AAC encoding
                cmd.args(["-codec:a", "aac", "-b:a", "128k"]);
            },
            AudioFormat::Ogg | AudioFormat::Webm => {
                // Use Vorbis for OGG, VP9 audio for WebM
                cmd.args(["-codec:a", "libvorbis", "-q:a", "4"]);
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converter_creation() {
        let converter = AudioConverter::new();
        assert!(converter.ffmpeg_path.is_none());
    }

    #[test]
    fn converter_with_custom_path() {
        let converter = AudioConverter::with_ffmpeg_path("/usr/local/bin/ffmpeg");
        assert_eq!(
            converter.ffmpeg_path.as_deref(),
            Some("/usr/local/bin/ffmpeg")
        );
    }

    #[test]
    fn ffmpeg_path_default() {
        let converter = AudioConverter::new();
        assert_eq!(converter.ffmpeg_path(), "ffmpeg");
    }

    #[test]
    fn ffmpeg_path_custom() {
        let converter = AudioConverter::with_ffmpeg_path("/custom/ffmpeg");
        assert_eq!(converter.ffmpeg_path(), "/custom/ffmpeg");
    }

    #[test]
    fn format_to_ffmpeg_mapping() {
        assert_eq!(AudioConverter::format_to_ffmpeg(AudioFormat::Mp3), "mp3");
        assert_eq!(AudioConverter::format_to_ffmpeg(AudioFormat::Wav), "wav");
        assert_eq!(AudioConverter::format_to_ffmpeg(AudioFormat::Opus), "opus");
        assert_eq!(AudioConverter::format_to_ffmpeg(AudioFormat::Ogg), "ogg");
        assert_eq!(AudioConverter::format_to_ffmpeg(AudioFormat::Flac), "flac");
        assert_eq!(AudioConverter::format_to_ffmpeg(AudioFormat::Webm), "webm");
        assert_eq!(AudioConverter::format_to_ffmpeg(AudioFormat::M4a), "ipod");
    }

    #[test]
    fn converter_has_debug() {
        let converter = AudioConverter::new();
        let debug = format!("{converter:?}");
        assert!(debug.contains("AudioConverter"));
    }

    #[test]
    fn converter_clone() {
        let converter = AudioConverter::with_ffmpeg_path("/path/to/ffmpeg");
        let cloned = converter.clone();
        assert_eq!(cloned.ffmpeg_path, converter.ffmpeg_path);
    }

    #[test]
    fn converter_default() {
        let converter = AudioConverter::default();
        assert!(converter.ffmpeg_path.is_none());
    }

    #[test]
    fn audio_format_display() {
        assert_eq!(format!("{}", AudioFormat::Mp3), "mp3");
        assert_eq!(format!("{}", AudioFormat::Wav), "wav");
        assert_eq!(format!("{}", AudioFormat::Opus), "opus");
        assert_eq!(format!("{}", AudioFormat::Ogg), "ogg");
        assert_eq!(format!("{}", AudioFormat::Flac), "flac");
        assert_eq!(format!("{}", AudioFormat::Webm), "webm");
        assert_eq!(format!("{}", AudioFormat::M4a), "m4a");
    }

    #[tokio::test]
    async fn is_available_returns_false_for_invalid_path() {
        let converter = AudioConverter::with_ffmpeg_path("/nonexistent/path/to/ffmpeg");
        assert!(!converter.is_available().await);
    }

    #[tokio::test]
    async fn convert_same_format_returns_clone() {
        // Create test audio data
        let audio = AudioData::new(vec![0, 1, 2, 3], AudioFormat::Mp3);
        let converter = AudioConverter::new();

        let result = converter.convert(&audio, AudioFormat::Mp3).await.unwrap();
        assert_eq!(result.format(), AudioFormat::Mp3);
        assert_eq!(result.data(), audio.data());
    }

    #[tokio::test]
    async fn convert_for_whisper_supported_format_returns_clone() {
        let audio = AudioData::new(vec![0, 1, 2, 3], AudioFormat::Mp3);
        let converter = AudioConverter::new();

        let result = converter.convert_for_whisper(&audio).await.unwrap();
        assert_eq!(result.format(), AudioFormat::Mp3);
        assert_eq!(result.data(), audio.data());
    }

    #[tokio::test]
    async fn convert_for_whisper_opus_needs_conversion() {
        let audio = AudioData::new(vec![0, 1, 2, 3], AudioFormat::Opus);
        let converter = AudioConverter::with_ffmpeg_path("/nonexistent/ffmpeg");

        // With invalid FFmpeg path, conversion should fail
        let result = converter.convert_for_whisper(&audio).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn convert_fails_with_invalid_ffmpeg() {
        let audio = AudioData::new(vec![0, 1, 2, 3], AudioFormat::Opus);
        let converter = AudioConverter::with_ffmpeg_path("/nonexistent/ffmpeg");

        let result = converter.convert(&audio, AudioFormat::Mp3).await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, SpeechError::AudioProcessing(_)));
    }
}
