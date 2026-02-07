//! Voice message service - Handles voice message processing workflow
//!
//! This service orchestrates the complete voice message flow:
//! 1. Receive audio from user
//! 2. Transcribe audio to text (STT)
//! 3. Process text through AI
//! 4. Synthesize response audio (TTS)
//! 5. Return audio response

use std::{fmt, sync::Arc, time::Instant};

use domain::entities::{AudioFormat, VoiceMessage, VoiceMessageStatus};
use domain::value_objects::ConversationId;
use tracing::{debug, info, instrument, warn};

use crate::{
    error::ApplicationError,
    ports::{SpeechPort, SynthesisResult, TranscriptionResult, VoiceConfig},
    services::ChatService,
};

/// Configuration for voice message processing
#[derive(Debug, Clone)]
pub struct VoiceMessageConfig {
    /// Whether to mirror response format (respond with audio to audio)
    pub mirror_response_format: bool,
    /// Default voice for TTS synthesis
    pub default_voice: String,
    /// Speech speed (0.25 - 4.0)
    pub speech_speed: f32,
    /// Output audio format for TTS
    pub output_format: AudioFormat,
    /// Language hint for transcription (e.g., "en", "de")
    pub language_hint: Option<String>,
}

impl Default for VoiceMessageConfig {
    fn default() -> Self {
        Self {
            mirror_response_format: true,
            default_voice: "nova".to_string(),
            speech_speed: 1.0,
            output_format: AudioFormat::Opus,
            language_hint: None,
        }
    }
}

/// Result of processing a voice message
#[derive(Debug)]
pub struct VoiceMessageResult {
    /// The processed voice message with updated status
    pub voice_message: VoiceMessage,
    /// Transcribed text from the user's audio
    pub transcription: String,
    /// AI-generated text response
    pub response_text: String,
    /// Synthesized audio response (if mirror_response_format is true)
    pub response_audio: Option<SynthesisResult>,
    /// Total processing time in milliseconds
    pub processing_time_ms: u64,
}

/// Service for processing voice messages
pub struct VoiceMessageService {
    speech_port: Arc<dyn SpeechPort>,
    chat_service: Arc<ChatService>,
    config: VoiceMessageConfig,
}

impl fmt::Debug for VoiceMessageService {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("VoiceMessageService")
            .field("config", &self.config)
            .finish_non_exhaustive()
    }
}

impl VoiceMessageService {
    /// Create a new voice message service
    pub fn new(speech_port: Arc<dyn SpeechPort>, chat_service: Arc<ChatService>) -> Self {
        Self {
            speech_port,
            chat_service,
            config: VoiceMessageConfig::default(),
        }
    }

    /// Create a voice message service with custom configuration
    pub fn with_config(
        speech_port: Arc<dyn SpeechPort>,
        chat_service: Arc<ChatService>,
        config: VoiceMessageConfig,
    ) -> Self {
        Self {
            speech_port,
            chat_service,
            config,
        }
    }

    /// Process a voice message end-to-end
    ///
    /// This method handles the complete workflow:
    /// 1. Transcribe audio to text
    /// 2. Process through AI chat
    /// 3. Synthesize response (if configured)
    #[instrument(skip(self, audio_data), fields(
        audio_size = audio_data.len(),
        format = %format,
        conversation_id = %conversation_id
    ))]
    pub async fn process_voice_message(
        &self,
        audio_data: Vec<u8>,
        format: AudioFormat,
        conversation_id: ConversationId,
        external_id: Option<String>,
    ) -> Result<VoiceMessageResult, ApplicationError> {
        let start = Instant::now();

        // Create voice message entity
        let mut voice_message =
            VoiceMessage::new_incoming(conversation_id, format, audio_data.len());

        if let Some(ext_id) = external_id {
            voice_message = voice_message.with_external_id(ext_id);
        }

        // Step 1: Transcribe audio
        info!("Starting voice message transcription");
        voice_message.start_transcription();

        let transcription = match self.transcribe(&audio_data, format).await {
            Ok(t) => t,
            Err(e) => {
                warn!(error = %e, "Transcription failed");
                voice_message.mark_failed(format!("Transcription failed: {e}"));
                return Err(e);
            },
        };

        voice_message.complete_transcription(
            transcription.text.clone(),
            transcription.detected_language.clone(),
            transcription.confidence,
        );

        debug!(
            transcription = %transcription.text,
            language = ?transcription.detected_language,
            confidence = ?transcription.confidence,
            "Transcription complete"
        );

        // Step 2: Process through AI
        info!("Processing transcription through AI");
        voice_message.start_processing();

        let ai_response = match self.chat_service.chat(&transcription.text).await {
            Ok(response) => response.content,
            Err(e) => {
                warn!(error = %e, "AI processing failed");
                voice_message.mark_failed(format!("AI processing failed: {e}"));
                return Err(e);
            },
        };

        voice_message.complete_processing();

        debug!(response_len = ai_response.len(), "AI response generated");

        // Step 3: Synthesize response audio (if configured)
        let response_audio = if self.config.mirror_response_format {
            info!("Synthesizing audio response");
            voice_message.start_synthesis();

            match self.synthesize(&ai_response).await {
                Ok(audio) => {
                    voice_message.complete_synthesis();
                    Some(audio)
                },
                Err(e) => {
                    warn!(error = %e, "Synthesis failed, falling back to text");
                    // Don't fail the whole operation, just skip audio
                    None
                },
            }
        } else {
            None
        };

        // Mark as ready for delivery
        voice_message.status = VoiceMessageStatus::ResponseReady;

        #[allow(clippy::cast_possible_truncation)]
        let processing_time_ms = start.elapsed().as_millis() as u64;

        info!(
            processing_time_ms = processing_time_ms,
            has_audio_response = response_audio.is_some(),
            "Voice message processing complete"
        );

        Ok(VoiceMessageResult {
            voice_message,
            transcription: transcription.text,
            response_text: ai_response,
            response_audio,
            processing_time_ms,
        })
    }

    /// Transcribe audio to text
    #[instrument(skip(self, audio_data), fields(format = %format))]
    pub async fn transcribe(
        &self,
        audio_data: &[u8],
        format: AudioFormat,
    ) -> Result<TranscriptionResult, ApplicationError> {
        self.speech_port
            .transcribe(
                audio_data.to_vec(),
                format,
                self.config.language_hint.clone(),
            )
            .await
    }

    /// Synthesize text to speech
    #[instrument(skip(self, text), fields(text_len = text.len()))]
    pub async fn synthesize(&self, text: &str) -> Result<SynthesisResult, ApplicationError> {
        let voice_config = VoiceConfig {
            voice_id: self.config.default_voice.clone(),
            speed: self.config.speech_speed,
        };

        self.speech_port
            .synthesize(text.to_string(), Some(voice_config))
            .await
    }

    /// Check if the speech service is available
    pub async fn is_available(&self) -> bool {
        self.speech_port.is_available().await
    }

    /// Get the current configuration
    #[must_use]
    pub const fn config(&self) -> &VoiceMessageConfig {
        &self.config
    }

    /// Update the configuration
    pub fn set_config(&mut self, config: VoiceMessageConfig) {
        self.config = config;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ports::MockSpeechPort;
    use std::sync::Arc;

    fn create_mock_chat_service() -> Arc<ChatService> {
        // We need a mock inference port for the chat service
        use crate::ports::InferencePort;
        use crate::ports::{InferenceResult, InferenceStream};
        use async_trait::async_trait;

        struct MockInferencePort;

        #[async_trait]
        impl InferencePort for MockInferencePort {
            async fn generate(&self, _message: &str) -> Result<InferenceResult, ApplicationError> {
                Ok(InferenceResult {
                    content: "Hello! I received your voice message.".to_string(),
                    model: "test-model".to_string(),
                    tokens_used: Some(10),
                    latency_ms: 100,
                })
            }

            async fn generate_with_context(
                &self,
                _conversation: &domain::Conversation,
            ) -> Result<InferenceResult, ApplicationError> {
                self.generate("").await
            }

            async fn generate_with_system(
                &self,
                _system_prompt: &str,
                _message: &str,
            ) -> Result<InferenceResult, ApplicationError> {
                self.generate("").await
            }

            async fn generate_stream(
                &self,
                _message: &str,
            ) -> Result<InferenceStream, ApplicationError> {
                unimplemented!()
            }

            async fn generate_stream_with_system(
                &self,
                _system_prompt: &str,
                _message: &str,
            ) -> Result<InferenceStream, ApplicationError> {
                unimplemented!()
            }

            async fn is_healthy(&self) -> bool {
                true
            }

            fn current_model(&self) -> String {
                "test-model".to_string()
            }

            async fn list_available_models(&self) -> Result<Vec<String>, ApplicationError> {
                Ok(vec!["test-model".to_string()])
            }

            async fn switch_model(&self, _model_name: &str) -> Result<(), ApplicationError> {
                Ok(())
            }
        }

        Arc::new(ChatService::new(Arc::new(MockInferencePort)))
    }

    #[test]
    fn voice_message_config_default() {
        let config = VoiceMessageConfig::default();
        assert!(config.mirror_response_format);
        assert_eq!(config.default_voice, "nova");
        assert!((config.speech_speed - 1.0).abs() < f32::EPSILON);
        assert_eq!(config.output_format, AudioFormat::Opus);
        assert!(config.language_hint.is_none());
    }

    #[test]
    fn service_has_debug() {
        let mut mock_speech = MockSpeechPort::new();
        mock_speech.expect_is_available().returning(|| true);

        let service = VoiceMessageService::new(Arc::new(mock_speech), create_mock_chat_service());

        let debug = format!("{service:?}");
        assert!(debug.contains("VoiceMessageService"));
        assert!(debug.contains("config"));
    }

    #[test]
    fn service_config_getter() {
        let mut mock_speech = MockSpeechPort::new();
        mock_speech.expect_is_available().returning(|| true);

        let service = VoiceMessageService::new(Arc::new(mock_speech), create_mock_chat_service());

        assert_eq!(service.config().default_voice, "nova");
    }

    #[test]
    fn service_config_setter() {
        let mut mock_speech = MockSpeechPort::new();
        mock_speech.expect_is_available().returning(|| true);

        let mut service =
            VoiceMessageService::new(Arc::new(mock_speech), create_mock_chat_service());

        let new_config = VoiceMessageConfig {
            default_voice: "alloy".to_string(),
            ..Default::default()
        };

        service.set_config(new_config);
        assert_eq!(service.config().default_voice, "alloy");
    }

    #[tokio::test]
    async fn service_is_available() {
        let mut mock_speech = MockSpeechPort::new();
        mock_speech.expect_is_available().returning(|| true);

        let service = VoiceMessageService::new(Arc::new(mock_speech), create_mock_chat_service());

        assert!(service.is_available().await);
    }

    #[tokio::test]
    async fn transcribe_delegates_to_port() {
        let mut mock_speech = MockSpeechPort::new();
        mock_speech.expect_transcribe().returning(|_, _, _| {
            Ok(TranscriptionResult {
                text: "Hello world".to_string(),
                detected_language: Some("en".to_string()),
                confidence: Some(0.95),
                duration_ms: Some(2000),
            })
        });

        let service = VoiceMessageService::new(Arc::new(mock_speech), create_mock_chat_service());

        let result = service
            .transcribe(&[1, 2, 3], AudioFormat::Opus)
            .await
            .unwrap();
        assert_eq!(result.text, "Hello world");
    }

    #[tokio::test]
    async fn synthesize_delegates_to_port() {
        use crate::ports::SynthesisResult;

        let mut mock_speech = MockSpeechPort::new();
        mock_speech.expect_synthesize().returning(|_, _| {
            Ok(SynthesisResult {
                audio_data: vec![1, 2, 3, 4],
                format: AudioFormat::Opus,
                duration_ms: Some(1500),
            })
        });

        let service = VoiceMessageService::new(Arc::new(mock_speech), create_mock_chat_service());

        let result = service.synthesize("Hello").await.unwrap();
        assert_eq!(result.audio_data.len(), 4);
    }

    #[tokio::test]
    async fn process_voice_message_full_workflow() {
        let mut mock_speech = MockSpeechPort::new();

        // Expect transcribe to be called
        mock_speech.expect_transcribe().returning(|_, _, _| {
            Ok(TranscriptionResult {
                text: "What's the weather like?".to_string(),
                detected_language: Some("en".to_string()),
                confidence: Some(0.98),
                duration_ms: Some(3000),
            })
        });

        // Expect synthesize to be called (mirror_response_format is true by default)
        mock_speech.expect_synthesize().returning(|_, _| {
            Ok(SynthesisResult {
                audio_data: vec![1, 2, 3, 4, 5],
                format: AudioFormat::Opus,
                duration_ms: Some(2000),
            })
        });

        let service = VoiceMessageService::new(Arc::new(mock_speech), create_mock_chat_service());

        let result = service
            .process_voice_message(
                vec![0, 1, 2, 3],
                AudioFormat::Opus,
                ConversationId::new(),
                Some("wamid.123".to_string()),
            )
            .await
            .unwrap();

        // Verify result
        assert_eq!(result.transcription, "What's the weather like?");
        assert!(result.response_text.contains("voice message"));
        assert!(result.response_audio.is_some());
        assert_eq!(
            result.voice_message.status,
            VoiceMessageStatus::ResponseReady
        );
        assert_eq!(
            result.voice_message.external_id,
            Some("wamid.123".to_string())
        );
    }

    #[tokio::test]
    async fn process_voice_message_without_audio_response() {
        let mut mock_speech = MockSpeechPort::new();

        mock_speech.expect_transcribe().returning(|_, _, _| {
            Ok(TranscriptionResult {
                text: "Hello".to_string(),
                detected_language: None,
                confidence: None,
                duration_ms: None,
            })
        });

        // synthesize should NOT be called
        // (we don't set an expectation for it)

        let config = VoiceMessageConfig {
            mirror_response_format: false,
            ..Default::default()
        };

        let service = VoiceMessageService::with_config(
            Arc::new(mock_speech),
            create_mock_chat_service(),
            config,
        );

        let result = service
            .process_voice_message(
                vec![0, 1, 2, 3],
                AudioFormat::Opus,
                ConversationId::new(),
                None,
            )
            .await
            .unwrap();

        assert!(result.response_audio.is_none());
    }

    #[tokio::test]
    async fn process_voice_message_transcription_failure() {
        let mut mock_speech = MockSpeechPort::new();

        mock_speech.expect_transcribe().returning(|_, _, _| {
            Err(ApplicationError::ExternalService(
                "Transcription failed".to_string(),
            ))
        });

        let service = VoiceMessageService::new(Arc::new(mock_speech), create_mock_chat_service());

        let result = service
            .process_voice_message(
                vec![0, 1, 2, 3],
                AudioFormat::Opus,
                ConversationId::new(),
                None,
            )
            .await;

        assert!(result.is_err());
    }
}
