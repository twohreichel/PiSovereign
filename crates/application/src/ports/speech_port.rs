//! Speech port - Interface for speech-to-text and text-to-speech operations

use async_trait::async_trait;
use domain::entities::AudioFormat;
#[cfg(test)]
use mockall::automock;

use crate::error::ApplicationError;

/// Result of a transcription operation
#[derive(Debug, Clone)]
pub struct TranscriptionResult {
    /// Transcribed text
    pub text: String,
    /// Detected language code (e.g., "en", "de")
    pub detected_language: Option<String>,
    /// Confidence score (0.0 - 1.0)
    pub confidence: Option<f32>,
    /// Duration of audio in milliseconds
    pub duration_ms: Option<u64>,
}

/// Result of a speech synthesis operation
#[derive(Debug, Clone)]
pub struct SynthesisResult {
    /// Generated audio data
    pub audio_data: Vec<u8>,
    /// Format of the audio
    pub format: AudioFormat,
    /// Duration of audio in milliseconds (if known)
    pub duration_ms: Option<u64>,
}

/// Voice configuration for synthesis
#[derive(Debug, Clone)]
pub struct VoiceConfig {
    /// Voice identifier (e.g., "nova", "alloy")
    pub voice_id: String,
    /// Speech speed (0.25 - 4.0, default 1.0)
    pub speed: f32,
}

impl Default for VoiceConfig {
    fn default() -> Self {
        Self {
            voice_id: "nova".to_string(),
            speed: 1.0,
        }
    }
}

/// Information about an available voice
#[derive(Debug, Clone)]
pub struct VoiceInfo {
    /// Voice identifier
    pub id: String,
    /// Human-readable name
    pub name: String,
    /// Optional description
    pub description: Option<String>,
    /// Language codes this voice supports
    pub languages: Vec<String>,
}

/// Port for speech processing operations
#[cfg_attr(test, automock)]
#[async_trait]
pub trait SpeechPort: Send + Sync {
    /// Transcribe audio data to text (Speech-to-Text)
    ///
    /// # Arguments
    /// * `audio_data` - Raw audio bytes
    /// * `format` - Format of the audio
    /// * `language_hint` - Optional language hint (e.g., "en", "de")
    ///
    /// # Returns
    /// Transcription result with text and metadata
    async fn transcribe(
        &self,
        audio_data: Vec<u8>,
        format: AudioFormat,
        language_hint: Option<String>,
    ) -> Result<TranscriptionResult, ApplicationError>;

    /// Synthesize speech from text (Text-to-Speech)
    ///
    /// # Arguments
    /// * `text` - Text to synthesize
    /// * `voice` - Optional voice configuration
    ///
    /// # Returns
    /// Synthesis result with audio data and metadata
    async fn synthesize(
        &self,
        text: String,
        voice: Option<VoiceConfig>,
    ) -> Result<SynthesisResult, ApplicationError>;

    /// Check if the speech service is available
    async fn is_available(&self) -> bool;

    /// List available voices for synthesis
    async fn list_voices(&self) -> Result<Vec<VoiceInfo>, ApplicationError>;

    /// Get the default output format for synthesized audio
    fn default_output_format(&self) -> AudioFormat;

    /// Check if a specific audio format is supported for transcription
    fn supports_format(&self, format: AudioFormat) -> bool;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn voice_config_default() {
        let config = VoiceConfig::default();
        assert_eq!(config.voice_id, "nova");
        assert!((config.speed - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn transcription_result_debug() {
        let result = TranscriptionResult {
            text: "Hello".to_string(),
            detected_language: Some("en".to_string()),
            confidence: Some(0.95),
            duration_ms: Some(1000),
        };
        let debug = format!("{result:?}");
        assert!(debug.contains("Hello"));
        assert!(debug.contains("en"));
    }

    #[test]
    fn synthesis_result_debug() {
        let result = SynthesisResult {
            audio_data: vec![1, 2, 3],
            format: AudioFormat::Opus,
            duration_ms: Some(2000),
        };
        let debug = format!("{result:?}");
        assert!(debug.contains("Opus"));
    }

    #[test]
    fn voice_info_creation() {
        let info = VoiceInfo {
            id: "nova".to_string(),
            name: "Nova".to_string(),
            description: Some("A warm voice".to_string()),
            languages: vec!["en".to_string(), "de".to_string()],
        };
        assert_eq!(info.id, "nova");
        assert_eq!(info.languages.len(), 2);
    }

    #[tokio::test]
    async fn mock_speech_port_transcribe() {
        let mut mock = MockSpeechPort::new();
        mock.expect_transcribe().returning(|_, _, _| {
            Ok(TranscriptionResult {
                text: "Test transcription".to_string(),
                detected_language: Some("en".to_string()),
                confidence: Some(0.99),
                duration_ms: Some(5000),
            })
        });

        let result = mock
            .transcribe(vec![1, 2, 3], AudioFormat::Opus, Some("en".to_string()))
            .await
            .unwrap();
        assert_eq!(result.text, "Test transcription");
    }

    #[tokio::test]
    async fn mock_speech_port_synthesize() {
        let mut mock = MockSpeechPort::new();
        mock.expect_synthesize().returning(|_, _| {
            Ok(SynthesisResult {
                audio_data: vec![1, 2, 3, 4],
                format: AudioFormat::Opus,
                duration_ms: Some(3000),
            })
        });

        let result = mock.synthesize("Hello".to_string(), None).await.unwrap();
        assert_eq!(result.audio_data.len(), 4);
        assert_eq!(result.format, AudioFormat::Opus);
    }

    #[tokio::test]
    async fn mock_speech_port_is_available() {
        let mut mock = MockSpeechPort::new();
        mock.expect_is_available().returning(|| true);

        assert!(mock.is_available().await);
    }

    #[test]
    fn mock_speech_port_supports_format() {
        let mut mock = MockSpeechPort::new();
        mock.expect_supports_format()
            .returning(|format| matches!(format, AudioFormat::Mp3 | AudioFormat::Wav));

        assert!(mock.supports_format(AudioFormat::Mp3));
        assert!(mock.supports_format(AudioFormat::Wav));
        assert!(!mock.supports_format(AudioFormat::Opus));
    }

    #[test]
    fn mock_speech_port_default_output_format() {
        let mut mock = MockSpeechPort::new();
        mock.expect_default_output_format()
            .returning(|| AudioFormat::Opus);

        assert_eq!(mock.default_output_format(), AudioFormat::Opus);
    }

    #[tokio::test]
    async fn mock_speech_port_list_voices() {
        let mut mock = MockSpeechPort::new();
        mock.expect_list_voices().returning(|| {
            Ok(vec![VoiceInfo {
                id: "nova".to_string(),
                name: "Nova".to_string(),
                description: None,
                languages: vec!["en".to_string()],
            }])
        });

        let voices = mock.list_voices().await.unwrap();
        assert_eq!(voices.len(), 1);
        assert_eq!(voices[0].id, "nova");
    }
}
