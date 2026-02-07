//! Port definitions for speech processing
//!
//! Defines the traits (ports) that speech processing adapters must implement.

use async_trait::async_trait;

use crate::error::SpeechError;
use crate::types::{AudioData, Transcription, VoiceInfo};

/// Port for Speech-to-Text (STT) implementations
///
/// Implementations of this trait convert audio data to text transcriptions.
///
/// # Example
///
/// ```ignore
/// use ai_speech::{SpeechToText, AudioData, AudioFormat};
///
/// async fn transcribe_voice_message(
///     stt: &impl SpeechToText,
///     audio: AudioData,
/// ) -> Result<String, SpeechError> {
///     let transcription = stt.transcribe(audio).await?;
///     Ok(transcription.text)
/// }
/// ```
#[async_trait]
pub trait SpeechToText: Send + Sync {
    /// Transcribe audio to text
    ///
    /// # Arguments
    ///
    /// * `audio` - Audio data to transcribe
    ///
    /// # Returns
    ///
    /// Returns a `Transcription` containing the transcribed text and metadata.
    ///
    /// # Errors
    ///
    /// Returns `SpeechError` if transcription fails.
    async fn transcribe(&self, audio: AudioData) -> Result<Transcription, SpeechError>;

    /// Transcribe audio with a specific language hint
    ///
    /// # Arguments
    ///
    /// * `audio` - Audio data to transcribe
    /// * `language` - ISO 639-1 language code (e.g., "en", "de", "es")
    ///
    /// # Returns
    ///
    /// Returns a `Transcription` containing the transcribed text.
    ///
    /// # Errors
    ///
    /// Returns `SpeechError` if transcription fails.
    async fn transcribe_with_language(
        &self,
        audio: AudioData,
        language: &str,
    ) -> Result<Transcription, SpeechError>;

    /// Check if the STT service is available
    ///
    /// # Returns
    ///
    /// Returns `true` if the service is available and ready to process requests.
    async fn is_available(&self) -> bool;

    /// Get the name of the current STT model
    ///
    /// # Returns
    ///
    /// Returns the model name or identifier.
    fn model_name(&self) -> &str;
}

/// Port for Text-to-Speech (TTS) implementations
///
/// Implementations of this trait convert text to audio speech.
///
/// # Example
///
/// ```ignore
/// use ai_speech::{TextToSpeech, AudioFormat};
///
/// async fn create_voice_response(
///     tts: &impl TextToSpeech,
///     text: &str,
/// ) -> Result<Vec<u8>, SpeechError> {
///     let audio = tts.synthesize(text, None).await?;
///     Ok(audio.into_data())
/// }
/// ```
#[async_trait]
pub trait TextToSpeech: Send + Sync {
    /// Convert text to speech
    ///
    /// # Arguments
    ///
    /// * `text` - Text to synthesize
    /// * `voice` - Optional voice ID to use (uses default if None)
    ///
    /// # Returns
    ///
    /// Returns `AudioData` containing the synthesized speech.
    ///
    /// # Errors
    ///
    /// Returns `SpeechError` if synthesis fails.
    async fn synthesize(&self, text: &str, voice: Option<&str>) -> Result<AudioData, SpeechError>;

    /// Convert text to speech with specific output format
    ///
    /// # Arguments
    ///
    /// * `text` - Text to synthesize
    /// * `voice` - Optional voice ID to use
    /// * `format` - Desired output audio format
    ///
    /// # Returns
    ///
    /// Returns `AudioData` in the requested format.
    ///
    /// # Errors
    ///
    /// Returns `SpeechError` if synthesis fails or format is not supported.
    async fn synthesize_with_format(
        &self,
        text: &str,
        voice: Option<&str>,
        format: crate::types::AudioFormat,
    ) -> Result<AudioData, SpeechError>;

    /// List available voices
    ///
    /// # Returns
    ///
    /// Returns a list of available voices with metadata.
    ///
    /// # Errors
    ///
    /// Returns `SpeechError` if listing fails.
    async fn list_voices(&self) -> Result<Vec<VoiceInfo>, SpeechError>;

    /// Check if the TTS service is available
    ///
    /// # Returns
    ///
    /// Returns `true` if the service is available and ready.
    async fn is_available(&self) -> bool;

    /// Get the name of the current TTS model
    ///
    /// # Returns
    ///
    /// Returns the model name or identifier.
    fn model_name(&self) -> &str;

    /// Get the default voice ID
    ///
    /// # Returns
    ///
    /// Returns the default voice identifier.
    fn default_voice(&self) -> &str;
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Mock implementation for testing
    struct MockSpeechToText {
        model: String,
        available: bool,
    }

    #[async_trait]
    impl SpeechToText for MockSpeechToText {
        async fn transcribe(&self, _audio: AudioData) -> Result<Transcription, SpeechError> {
            Ok(Transcription::new("Mock transcription"))
        }

        async fn transcribe_with_language(
            &self,
            _audio: AudioData,
            language: &str,
        ) -> Result<Transcription, SpeechError> {
            Ok(Transcription::new("Mock transcription").with_language(language))
        }

        async fn is_available(&self) -> bool {
            self.available
        }

        fn model_name(&self) -> &str {
            &self.model
        }
    }

    struct MockTextToSpeech {
        model: String,
        voice: String,
        available: bool,
    }

    #[async_trait]
    impl TextToSpeech for MockTextToSpeech {
        async fn synthesize(
            &self,
            _text: &str,
            _voice: Option<&str>,
        ) -> Result<AudioData, SpeechError> {
            Ok(AudioData::new(
                vec![0, 1, 2, 3],
                crate::types::AudioFormat::Mp3,
            ))
        }

        async fn synthesize_with_format(
            &self,
            _text: &str,
            _voice: Option<&str>,
            format: crate::types::AudioFormat,
        ) -> Result<AudioData, SpeechError> {
            Ok(AudioData::new(vec![0, 1, 2, 3], format))
        }

        async fn list_voices(&self) -> Result<Vec<VoiceInfo>, SpeechError> {
            Ok(vec![VoiceInfo::new("alloy", "Alloy")])
        }

        async fn is_available(&self) -> bool {
            self.available
        }

        fn model_name(&self) -> &str {
            &self.model
        }

        fn default_voice(&self) -> &str {
            &self.voice
        }
    }

    #[tokio::test]
    async fn mock_stt_transcribes() {
        let stt = MockSpeechToText {
            model: "mock-whisper".to_string(),
            available: true,
        };

        let audio = AudioData::new(vec![0, 1, 2], crate::types::AudioFormat::Mp3);
        let result = stt.transcribe(audio).await;

        assert!(result.is_ok());
        assert_eq!(result.unwrap().text, "Mock transcription");
    }

    #[tokio::test]
    async fn mock_stt_transcribes_with_language() {
        let stt = MockSpeechToText {
            model: "mock-whisper".to_string(),
            available: true,
        };

        let audio = AudioData::new(vec![0, 1, 2], crate::types::AudioFormat::Mp3);
        let result = stt.transcribe_with_language(audio, "de").await;

        assert!(result.is_ok());
        let transcription = result.unwrap();
        assert_eq!(transcription.text, "Mock transcription");
        assert_eq!(transcription.language, Some("de".to_string()));
    }

    #[tokio::test]
    async fn mock_stt_availability() {
        let available_stt = MockSpeechToText {
            model: "mock".to_string(),
            available: true,
        };
        let unavailable_stt = MockSpeechToText {
            model: "mock".to_string(),
            available: false,
        };

        assert!(available_stt.is_available().await);
        assert!(!unavailable_stt.is_available().await);
    }

    #[tokio::test]
    async fn mock_tts_synthesizes() {
        let tts = MockTextToSpeech {
            model: "mock-tts".to_string(),
            voice: "alloy".to_string(),
            available: true,
        };

        let result = tts.synthesize("Hello", None).await;

        assert!(result.is_ok());
        assert!(!result.unwrap().is_empty());
    }

    #[tokio::test]
    async fn mock_tts_synthesizes_with_format() {
        let tts = MockTextToSpeech {
            model: "mock-tts".to_string(),
            voice: "alloy".to_string(),
            available: true,
        };

        let result = tts
            .synthesize_with_format("Hello", None, crate::types::AudioFormat::Opus)
            .await;

        assert!(result.is_ok());
        assert_eq!(result.unwrap().format(), crate::types::AudioFormat::Opus);
    }

    #[tokio::test]
    async fn mock_tts_lists_voices() {
        let tts = MockTextToSpeech {
            model: "mock-tts".to_string(),
            voice: "alloy".to_string(),
            available: true,
        };

        let voices = tts.list_voices().await.unwrap();

        assert_eq!(voices.len(), 1);
        assert_eq!(voices[0].id, "alloy");
    }

    #[test]
    fn mock_stt_model_name() {
        let stt = MockSpeechToText {
            model: "whisper-1".to_string(),
            available: true,
        };

        assert_eq!(stt.model_name(), "whisper-1");
    }

    #[test]
    fn mock_tts_default_voice() {
        let tts = MockTextToSpeech {
            model: "tts-1".to_string(),
            voice: "nova".to_string(),
            available: true,
        };

        assert_eq!(tts.default_voice(), "nova");
    }
}
