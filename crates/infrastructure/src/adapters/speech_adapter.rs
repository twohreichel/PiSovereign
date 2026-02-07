//! Speech adapter - Implements SpeechPort using ai_speech crate

use std::sync::Arc;

use ai_speech::{
    AudioConverter, AudioData, AudioFormat as AiAudioFormat, OpenAISpeechProvider, SpeechConfig,
    SpeechError, SpeechToText, TextToSpeech,
};
use application::error::ApplicationError;
use application::ports::{
    SpeechPort, SynthesisResult, TranscriptionResult, VoiceConfig, VoiceInfo,
};
use async_trait::async_trait;
use domain::entities::AudioFormat;
use tracing::{debug, instrument};

/// Adapter for speech services using ai_speech crate
pub struct SpeechAdapter {
    provider: Arc<OpenAISpeechProvider>,
    converter: Arc<AudioConverter>,
}

impl std::fmt::Debug for SpeechAdapter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SpeechAdapter")
            .field("provider", &"OpenAISpeechProvider")
            .finish()
    }
}

impl SpeechAdapter {
    /// Create a new speech adapter
    ///
    /// # Errors
    ///
    /// Returns an error if the provider fails to initialize.
    pub fn new(config: SpeechConfig) -> Result<Self, ApplicationError> {
        let provider = OpenAISpeechProvider::new(config)
            .map_err(|e: SpeechError| ApplicationError::Configuration(e.to_string()))?;

        Ok(Self {
            provider: Arc::new(provider),
            converter: Arc::new(AudioConverter::new()),
        })
    }

    /// Create a speech adapter with a custom FFmpeg path for conversion
    ///
    /// # Errors
    ///
    /// Returns an error if the provider fails to initialize.
    pub fn with_ffmpeg_path(
        config: SpeechConfig,
        ffmpeg_path: impl Into<String>,
    ) -> Result<Self, ApplicationError> {
        let provider = OpenAISpeechProvider::new(config)
            .map_err(|e: SpeechError| ApplicationError::Configuration(e.to_string()))?;

        Ok(Self {
            provider: Arc::new(provider),
            converter: Arc::new(AudioConverter::with_ffmpeg_path(ffmpeg_path)),
        })
    }

    /// Convert domain AudioFormat to ai_speech AudioFormat
    const fn domain_to_ai_format(format: AudioFormat) -> AiAudioFormat {
        match format {
            AudioFormat::Opus => AiAudioFormat::Opus,
            AudioFormat::Ogg => AiAudioFormat::Ogg,
            AudioFormat::Mp3 => AiAudioFormat::Mp3,
            AudioFormat::Wav => AiAudioFormat::Wav,
        }
    }

    /// Convert ai_speech AudioFormat to domain AudioFormat
    const fn ai_to_domain_format(format: AiAudioFormat) -> AudioFormat {
        match format {
            AiAudioFormat::Opus => AudioFormat::Opus,
            AiAudioFormat::Ogg => AudioFormat::Ogg,
            AiAudioFormat::Wav => AudioFormat::Wav,
            // Mp3 and other formats (Flac, Webm, M4a) default to Mp3
            AiAudioFormat::Mp3 | AiAudioFormat::Flac | AiAudioFormat::Webm | AiAudioFormat::M4a => {
                AudioFormat::Mp3
            },
        }
    }

    /// Map speech error to application error
    fn map_error(err: SpeechError) -> ApplicationError {
        match err {
            SpeechError::Configuration(e) => ApplicationError::Configuration(e),
            SpeechError::ConnectionFailed(e) | SpeechError::RequestFailed(e) => {
                ApplicationError::ExternalService(e)
            },
            SpeechError::InvalidAudio(e) => {
                ApplicationError::InvalidOperation(format!("Invalid audio: {e}"))
            },
            SpeechError::AudioTooLong {
                duration_ms,
                max_ms,
            } => ApplicationError::InvalidOperation(format!(
                "Audio too long: {duration_ms}ms exceeds max {max_ms}ms"
            )),
            SpeechError::TranscriptionFailed(e) => {
                ApplicationError::ExternalService(format!("Transcription failed: {e}"))
            },
            SpeechError::SynthesisFailed(e) => {
                ApplicationError::ExternalService(format!("Synthesis failed: {e}"))
            },
            SpeechError::InvalidResponse(e) => {
                ApplicationError::Internal(format!("Invalid response: {e}"))
            },
            SpeechError::Timeout(ms) => {
                ApplicationError::ExternalService(format!("Speech service timeout after {ms}ms"))
            },
            SpeechError::RateLimited => ApplicationError::RateLimited,
            SpeechError::VoiceNotFound(v) => {
                ApplicationError::InvalidOperation(format!("Voice not found: {v}"))
            },
            SpeechError::ModelNotAvailable(m) => {
                ApplicationError::InvalidOperation(format!("Model not available: {m}"))
            },
            SpeechError::ServiceUnavailable(e) => ApplicationError::ExternalService(e),
            SpeechError::AudioProcessing(e) => {
                ApplicationError::Internal(format!("Audio processing failed: {e}"))
            },
        }
    }
}

#[async_trait]
impl SpeechPort for SpeechAdapter {
    #[instrument(skip(self, audio_data), fields(format = ?format, data_size = audio_data.len()))]
    async fn transcribe(
        &self,
        audio_data: Vec<u8>,
        format: AudioFormat,
        language_hint: Option<String>,
    ) -> Result<TranscriptionResult, ApplicationError> {
        let ai_format = Self::domain_to_ai_format(format);

        // Create AudioData from raw bytes
        let audio = AudioData::new(audio_data, ai_format);

        // Convert to Whisper-compatible format if necessary
        let audio_for_whisper: AudioData = if ai_format.is_whisper_supported() {
            audio
        } else {
            debug!(
                "Converting audio from {:?} to Whisper-compatible format",
                format
            );
            self.converter
                .convert_for_whisper(&audio)
                .await
                .map_err(Self::map_error)?
        };

        // Perform transcription (with or without language hint)
        let transcription: ai_speech::Transcription = match language_hint {
            Some(ref lang) => self
                .provider
                .transcribe_with_language(audio_for_whisper, lang)
                .await
                .map_err(Self::map_error)?,
            None => self
                .provider
                .transcribe(audio_for_whisper)
                .await
                .map_err(Self::map_error)?,
        };

        debug!(
            text_len = transcription.text.len(),
            language = ?transcription.language,
            confidence = ?transcription.confidence,
            "Transcription complete"
        );

        Ok(TranscriptionResult {
            text: transcription.text,
            detected_language: transcription.language,
            confidence: transcription.confidence,
            duration_ms: transcription.duration_ms,
        })
    }

    #[instrument(skip(self, text), fields(text_len = text.len()))]
    async fn synthesize(
        &self,
        text: String,
        voice: Option<VoiceConfig>,
    ) -> Result<SynthesisResult, ApplicationError> {
        // Map voice config
        let voice_id = voice.as_ref().map(|v| v.voice_id.as_str());

        // Perform synthesis
        let audio: AudioData = self
            .provider
            .synthesize(&text, voice_id)
            .await
            .map_err(Self::map_error)?;

        let format = Self::ai_to_domain_format(audio.format());
        let duration_ms = audio.duration_ms();

        debug!(
            audio_size = audio.data().len(),
            format = ?format,
            duration_ms = ?duration_ms,
            "Synthesis complete"
        );

        Ok(SynthesisResult {
            audio_data: audio.into_data(),
            format,
            duration_ms,
        })
    }

    async fn is_available(&self) -> bool {
        <OpenAISpeechProvider as SpeechToText>::is_available(&self.provider).await
    }

    async fn list_voices(&self) -> Result<Vec<VoiceInfo>, ApplicationError> {
        let voices: Vec<ai_speech::VoiceInfo> =
            self.provider.list_voices().await.map_err(Self::map_error)?;

        Ok(voices
            .into_iter()
            .map(|v| VoiceInfo {
                id: v.id,
                name: v.name,
                description: v.description,
                languages: v.languages,
            })
            .collect())
    }

    fn default_output_format(&self) -> AudioFormat {
        // OpenAI TTS defaults to Opus for efficiency
        AudioFormat::Opus
    }

    fn supports_format(&self, format: AudioFormat) -> bool {
        // Whisper supports these formats directly; others can be converted
        matches!(
            format,
            AudioFormat::Mp3 | AudioFormat::Wav | AudioFormat::Opus | AudioFormat::Ogg
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adapter_has_debug() {
        // We can't easily test without a real config, but we can test the format conversion
        let format = SpeechAdapter::domain_to_ai_format(AudioFormat::Opus);
        assert_eq!(format, AiAudioFormat::Opus);
    }

    #[test]
    fn domain_to_ai_format_conversion() {
        assert_eq!(
            SpeechAdapter::domain_to_ai_format(AudioFormat::Opus),
            AiAudioFormat::Opus
        );
        assert_eq!(
            SpeechAdapter::domain_to_ai_format(AudioFormat::Ogg),
            AiAudioFormat::Ogg
        );
        assert_eq!(
            SpeechAdapter::domain_to_ai_format(AudioFormat::Mp3),
            AiAudioFormat::Mp3
        );
        assert_eq!(
            SpeechAdapter::domain_to_ai_format(AudioFormat::Wav),
            AiAudioFormat::Wav
        );
    }

    #[test]
    fn ai_to_domain_format_conversion() {
        assert_eq!(
            SpeechAdapter::ai_to_domain_format(AiAudioFormat::Opus),
            AudioFormat::Opus
        );
        assert_eq!(
            SpeechAdapter::ai_to_domain_format(AiAudioFormat::Ogg),
            AudioFormat::Ogg
        );
        assert_eq!(
            SpeechAdapter::ai_to_domain_format(AiAudioFormat::Mp3),
            AudioFormat::Mp3
        );
        assert_eq!(
            SpeechAdapter::ai_to_domain_format(AiAudioFormat::Wav),
            AudioFormat::Wav
        );
        // Unsupported formats fall back to Mp3
        assert_eq!(
            SpeechAdapter::ai_to_domain_format(AiAudioFormat::Flac),
            AudioFormat::Mp3
        );
        assert_eq!(
            SpeechAdapter::ai_to_domain_format(AiAudioFormat::Webm),
            AudioFormat::Mp3
        );
        assert_eq!(
            SpeechAdapter::ai_to_domain_format(AiAudioFormat::M4a),
            AudioFormat::Mp3
        );
    }

    #[test]
    fn error_mapping_configuration() {
        let err = SpeechAdapter::map_error(SpeechError::Configuration("bad config".to_string()));
        assert!(matches!(err, ApplicationError::Configuration(_)));
    }

    #[test]
    fn error_mapping_connection() {
        let err =
            SpeechAdapter::map_error(SpeechError::ConnectionFailed("network error".to_string()));
        assert!(matches!(err, ApplicationError::ExternalService(_)));
    }

    #[test]
    fn error_mapping_rate_limited() {
        let err = SpeechAdapter::map_error(SpeechError::RateLimited);
        assert!(matches!(err, ApplicationError::RateLimited));
    }

    #[test]
    fn error_mapping_timeout() {
        let err = SpeechAdapter::map_error(SpeechError::Timeout(30000));
        assert!(matches!(err, ApplicationError::ExternalService(_)));
    }

    #[test]
    fn error_mapping_audio_too_long() {
        let err = SpeechAdapter::map_error(SpeechError::AudioTooLong {
            duration_ms: 200_000,
            max_ms: 120_000,
        });
        assert!(matches!(err, ApplicationError::InvalidOperation(_)));
    }

    #[test]
    fn error_mapping_transcription_failed() {
        let err =
            SpeechAdapter::map_error(SpeechError::TranscriptionFailed("API error".to_string()));
        assert!(matches!(err, ApplicationError::ExternalService(_)));
    }

    #[test]
    fn error_mapping_synthesis_failed() {
        let err = SpeechAdapter::map_error(SpeechError::SynthesisFailed("TTS error".to_string()));
        assert!(matches!(err, ApplicationError::ExternalService(_)));
    }

    #[test]
    fn error_mapping_voice_not_found() {
        let err = SpeechAdapter::map_error(SpeechError::VoiceNotFound("unknown".to_string()));
        assert!(matches!(err, ApplicationError::InvalidOperation(_)));
    }

    #[test]
    fn error_mapping_audio_processing() {
        let err = SpeechAdapter::map_error(SpeechError::AudioProcessing(
            "conversion failed".to_string(),
        ));
        assert!(matches!(err, ApplicationError::Internal(_)));
    }
}
