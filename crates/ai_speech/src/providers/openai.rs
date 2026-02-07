//! OpenAI Speech Provider
//!
//! Implements `SpeechToText` using OpenAI Whisper and `TextToSpeech` using OpenAI TTS.
//!
//! # Supported Audio Formats
//!
//! ## STT (Whisper)
//! - mp3, mp4, mpeg, mpga, m4a, wav, webm
//! - Note: OGG/Opus (WhatsApp format) is NOT directly supported - use converter
//!
//! ## TTS
//! - mp3, opus, aac, flac, wav, pcm

use std::time::Duration;

use async_trait::async_trait;
use bytes::Bytes;
use reqwest::Client;
use reqwest::multipart::{Form, Part};
use serde::{Deserialize, Serialize};
use tracing::{debug, instrument, warn};

use crate::config::SpeechConfig;
use crate::error::SpeechError;
use crate::ports::{SpeechToText, TextToSpeech};
use crate::types::{AudioData, AudioFormat, Transcription, VoiceGender, VoiceInfo};

/// OpenAI speech provider implementing both STT and TTS
#[derive(Debug, Clone)]
pub struct OpenAISpeechProvider {
    client: Client,
    config: SpeechConfig,
}

impl OpenAISpeechProvider {
    /// Create a new OpenAI speech provider
    ///
    /// # Arguments
    ///
    /// * `config` - Speech configuration
    ///
    /// # Returns
    ///
    /// Returns the provider instance.
    ///
    /// # Errors
    ///
    /// Returns `SpeechError::Configuration` if the configuration is invalid.
    pub fn new(config: SpeechConfig) -> Result<Self, SpeechError> {
        config.validate().map_err(SpeechError::Configuration)?;

        let client = Client::builder()
            .timeout(Duration::from_millis(config.timeout_ms))
            .build()
            .map_err(|e| {
                SpeechError::Configuration(format!("Failed to create HTTP client: {e}"))
            })?;

        Ok(Self { client, config })
    }

    /// Get the API key
    fn api_key(&self) -> &str {
        self.config.openai_api_key.as_deref().unwrap_or_default()
    }

    /// Build the STT endpoint URL
    fn stt_url(&self) -> String {
        format!("{}/audio/transcriptions", self.config.openai_base_url)
    }

    /// Build the TTS endpoint URL
    fn tts_url(&self) -> String {
        format!("{}/audio/speech", self.config.openai_base_url)
    }

    /// Convert OpenAI response format string to AudioFormat
    fn response_format_to_audio_format(format: &str) -> AudioFormat {
        match format {
            "opus" => AudioFormat::Opus,
            "aac" => AudioFormat::M4a,
            "flac" => AudioFormat::Flac,
            "wav" | "pcm" => AudioFormat::Wav,
            // Default to MP3 for unknown formats
            _ => AudioFormat::Mp3,
        }
    }

    /// Convert AudioFormat to OpenAI TTS response format string
    const fn audio_format_to_response_format(format: AudioFormat) -> &'static str {
        match format {
            AudioFormat::Mp3 => "mp3",
            // Opus, OGG, and WebM all use opus codec
            AudioFormat::Opus | AudioFormat::Ogg | AudioFormat::Webm => "opus",
            AudioFormat::M4a => "aac",
            AudioFormat::Flac => "flac",
            AudioFormat::Wav => "wav",
        }
    }
}

/// OpenAI Whisper transcription response
#[derive(Debug, Deserialize)]
struct WhisperResponse {
    text: String,
    #[serde(default)]
    language: Option<String>,
    #[serde(default)]
    duration: Option<f64>,
}

/// OpenAI TTS request body
#[derive(Debug, Serialize)]
struct TtsRequest<'a> {
    model: &'a str,
    input: &'a str,
    voice: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    response_format: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    speed: Option<f32>,
}

/// OpenAI API error response
#[derive(Debug, Deserialize)]
struct ApiError {
    error: ApiErrorDetail,
}

#[derive(Debug, Deserialize)]
struct ApiErrorDetail {
    message: String,
    #[serde(rename = "type")]
    #[allow(dead_code)] // Part of OpenAI API contract, kept for future use
    error_type: Option<String>,
    code: Option<String>,
}

#[async_trait]
impl SpeechToText for OpenAISpeechProvider {
    #[instrument(skip(self, audio), fields(audio_size = audio.size_bytes(), format = ?audio.format()))]
    async fn transcribe(&self, audio: AudioData) -> Result<Transcription, SpeechError> {
        debug!("Transcribing audio with OpenAI Whisper");

        // Check audio duration if known
        if let Some(duration_ms) = audio.duration_ms() {
            if duration_ms > self.config.max_audio_duration_ms {
                return Err(SpeechError::AudioTooLong {
                    duration_ms,
                    max_ms: self.config.max_audio_duration_ms,
                });
            }
        }

        // Validate audio is not empty
        if audio.is_empty() {
            return Err(SpeechError::InvalidAudio("Audio data is empty".to_string()));
        }

        // Check if format is directly supported by Whisper
        if !audio.format().is_whisper_supported() {
            return Err(SpeechError::InvalidAudio(format!(
                "Audio format {:?} is not directly supported by Whisper. Convert to MP3 first.",
                audio.format()
            )));
        }

        // Build multipart form
        let filename = audio.filename("audio");
        let mime_type = audio.mime_type();
        let data = audio.into_data();

        let file_part = Part::bytes(data)
            .file_name(filename)
            .mime_str(mime_type)
            .map_err(|e| SpeechError::InvalidAudio(format!("Invalid MIME type: {e}")))?;

        let form = Form::new()
            .part("file", file_part)
            .text("model", self.config.stt_model.clone());

        // Send request
        let response = self
            .client
            .post(self.stt_url())
            .bearer_auth(self.api_key())
            .multipart(form)
            .send()
            .await?;

        // Handle response
        let status = response.status();
        if !status.is_success() {
            let error_body = response.text().await.unwrap_or_default();

            // Try to parse API error
            if let Ok(api_error) = serde_json::from_str::<ApiError>(&error_body) {
                return match api_error.error.code.as_deref() {
                    Some("rate_limit_exceeded") => Err(SpeechError::RateLimited),
                    Some("model_not_found") => Err(SpeechError::ModelNotAvailable(
                        self.config.stt_model.clone(),
                    )),
                    _ => Err(SpeechError::TranscriptionFailed(api_error.error.message)),
                };
            }

            return Err(SpeechError::TranscriptionFailed(format!(
                "HTTP {status}: {error_body}"
            )));
        }

        let whisper_response: WhisperResponse = response
            .json()
            .await
            .map_err(|e| SpeechError::InvalidResponse(format!("Failed to parse response: {e}")))?;

        debug!(
            text_len = whisper_response.text.len(),
            language = ?whisper_response.language,
            "Transcription complete"
        );

        let mut transcription = Transcription::new(whisper_response.text);

        if let Some(lang) = whisper_response.language {
            transcription = transcription.with_language(lang);
        }

        if let Some(duration) = whisper_response.duration {
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let duration_ms = (duration * 1000.0) as u64;
            transcription = transcription.with_duration(duration_ms);
        }

        Ok(transcription)
    }

    #[instrument(skip(self, audio), fields(audio_size = audio.size_bytes(), language = %language))]
    async fn transcribe_with_language(
        &self,
        audio: AudioData,
        language: &str,
    ) -> Result<Transcription, SpeechError> {
        debug!("Transcribing audio with language hint: {}", language);

        // Check audio duration if known
        if let Some(duration_ms) = audio.duration_ms() {
            if duration_ms > self.config.max_audio_duration_ms {
                return Err(SpeechError::AudioTooLong {
                    duration_ms,
                    max_ms: self.config.max_audio_duration_ms,
                });
            }
        }

        if audio.is_empty() {
            return Err(SpeechError::InvalidAudio("Audio data is empty".to_string()));
        }

        if !audio.format().is_whisper_supported() {
            return Err(SpeechError::InvalidAudio(format!(
                "Audio format {:?} is not directly supported by Whisper. Convert to MP3 first.",
                audio.format()
            )));
        }

        let filename = audio.filename("audio");
        let mime_type = audio.mime_type();
        let data = audio.into_data();

        let file_part = Part::bytes(data)
            .file_name(filename)
            .mime_str(mime_type)
            .map_err(|e| SpeechError::InvalidAudio(format!("Invalid MIME type: {e}")))?;

        let form = Form::new()
            .part("file", file_part)
            .text("model", self.config.stt_model.clone())
            .text("language", language.to_string());

        let response = self
            .client
            .post(self.stt_url())
            .bearer_auth(self.api_key())
            .multipart(form)
            .send()
            .await?;

        let status = response.status();
        if !status.is_success() {
            let error_body = response.text().await.unwrap_or_default();

            if let Ok(api_error) = serde_json::from_str::<ApiError>(&error_body) {
                return match api_error.error.code.as_deref() {
                    Some("rate_limit_exceeded") => Err(SpeechError::RateLimited),
                    Some("model_not_found") => Err(SpeechError::ModelNotAvailable(
                        self.config.stt_model.clone(),
                    )),
                    _ => Err(SpeechError::TranscriptionFailed(api_error.error.message)),
                };
            }

            return Err(SpeechError::TranscriptionFailed(format!(
                "HTTP {status}: {error_body}"
            )));
        }

        let whisper_response: WhisperResponse = response
            .json()
            .await
            .map_err(|e| SpeechError::InvalidResponse(format!("Failed to parse response: {e}")))?;

        let mut transcription = Transcription::new(whisper_response.text).with_language(language);

        if let Some(duration) = whisper_response.duration {
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let duration_ms = (duration * 1000.0) as u64;
            transcription = transcription.with_duration(duration_ms);
        }

        Ok(transcription)
    }

    async fn is_available(&self) -> bool {
        // Try a simple models endpoint to check connectivity
        let models_url = format!("{}/models", self.config.openai_base_url);

        match self
            .client
            .get(&models_url)
            .bearer_auth(self.api_key())
            .timeout(Duration::from_secs(5))
            .send()
            .await
        {
            Ok(response) => response.status().is_success(),
            Err(e) => {
                warn!("OpenAI STT availability check failed: {}", e);
                false
            },
        }
    }

    fn model_name(&self) -> &str {
        &self.config.stt_model
    }
}

#[async_trait]
impl TextToSpeech for OpenAISpeechProvider {
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
        debug!("Synthesizing speech with OpenAI TTS");

        if text.is_empty() {
            return Err(SpeechError::SynthesisFailed(
                "Text cannot be empty".to_string(),
            ));
        }

        // OpenAI TTS has a 4096 character limit
        if text.len() > 4096 {
            return Err(SpeechError::SynthesisFailed(format!(
                "Text too long: {} characters exceeds 4096 limit",
                text.len()
            )));
        }

        let voice = voice.unwrap_or(&self.config.default_voice);
        let response_format = Self::audio_format_to_response_format(format);

        let request = TtsRequest {
            model: &self.config.tts_model,
            input: text,
            voice,
            response_format: Some(response_format),
            speed: if (self.config.speed - 1.0).abs() < f32::EPSILON {
                None
            } else {
                Some(self.config.speed)
            },
        };

        let response = self
            .client
            .post(self.tts_url())
            .bearer_auth(self.api_key())
            .json(&request)
            .send()
            .await?;

        let status = response.status();
        if !status.is_success() {
            let error_body = response.text().await.unwrap_or_default();

            if let Ok(api_error) = serde_json::from_str::<ApiError>(&error_body) {
                return match api_error.error.code.as_deref() {
                    Some("rate_limit_exceeded") => Err(SpeechError::RateLimited),
                    Some("model_not_found") => Err(SpeechError::ModelNotAvailable(
                        self.config.tts_model.clone(),
                    )),
                    Some("invalid_voice") => Err(SpeechError::VoiceNotFound(voice.to_string())),
                    _ => Err(SpeechError::SynthesisFailed(api_error.error.message)),
                };
            }

            return Err(SpeechError::SynthesisFailed(format!(
                "HTTP {status}: {error_body}"
            )));
        }

        let audio_bytes: Bytes = response
            .bytes()
            .await
            .map_err(|e| SpeechError::InvalidResponse(format!("Failed to read audio: {e}")))?;

        debug!(audio_size = audio_bytes.len(), "Speech synthesis complete");

        let output_format = Self::response_format_to_audio_format(response_format);
        Ok(AudioData::new(audio_bytes.to_vec(), output_format))
    }

    async fn list_voices(&self) -> Result<Vec<VoiceInfo>, SpeechError> {
        // OpenAI doesn't have a voices endpoint, return static list
        Ok(vec![
            VoiceInfo {
                id: "alloy".to_string(),
                name: "Alloy".to_string(),
                description: Some("Neutral and balanced voice".to_string()),
                languages: vec!["en".to_string(), "de".to_string(), "es".to_string()],
                gender: Some(VoiceGender::Neutral),
                preview_url: None,
            },
            VoiceInfo {
                id: "echo".to_string(),
                name: "Echo".to_string(),
                description: Some("Warm and conversational voice".to_string()),
                languages: vec!["en".to_string(), "de".to_string(), "es".to_string()],
                gender: Some(VoiceGender::Male),
                preview_url: None,
            },
            VoiceInfo {
                id: "fable".to_string(),
                name: "Fable".to_string(),
                description: Some("British-accented storyteller voice".to_string()),
                languages: vec!["en".to_string(), "de".to_string(), "es".to_string()],
                gender: Some(VoiceGender::Male),
                preview_url: None,
            },
            VoiceInfo {
                id: "onyx".to_string(),
                name: "Onyx".to_string(),
                description: Some("Deep and authoritative voice".to_string()),
                languages: vec!["en".to_string(), "de".to_string(), "es".to_string()],
                gender: Some(VoiceGender::Male),
                preview_url: None,
            },
            VoiceInfo {
                id: "nova".to_string(),
                name: "Nova".to_string(),
                description: Some("Friendly and upbeat voice".to_string()),
                languages: vec!["en".to_string(), "de".to_string(), "es".to_string()],
                gender: Some(VoiceGender::Female),
                preview_url: None,
            },
            VoiceInfo {
                id: "shimmer".to_string(),
                name: "Shimmer".to_string(),
                description: Some("Clear and expressive voice".to_string()),
                languages: vec!["en".to_string(), "de".to_string(), "es".to_string()],
                gender: Some(VoiceGender::Female),
                preview_url: None,
            },
        ])
    }

    async fn is_available(&self) -> bool {
        // Reuse the same check as STT
        let models_url = format!("{}/models", self.config.openai_base_url);

        match self
            .client
            .get(&models_url)
            .bearer_auth(self.api_key())
            .timeout(Duration::from_secs(5))
            .send()
            .await
        {
            Ok(response) => response.status().is_success(),
            Err(e) => {
                warn!("OpenAI TTS availability check failed: {}", e);
                false
            },
        }
    }

    fn model_name(&self) -> &str {
        &self.config.tts_model
    }

    fn default_voice(&self) -> &str {
        &self.config.default_voice
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn create_test_provider(mock_server: &MockServer) -> OpenAISpeechProvider {
        let config = SpeechConfig {
            openai_api_key: Some("test-api-key".to_string()),
            openai_base_url: mock_server.uri(),
            ..Default::default()
        };
        OpenAISpeechProvider::new(config).unwrap()
    }

    mod stt_tests {
        use super::*;

        #[tokio::test]
        async fn transcribe_success() {
            let mock_server = MockServer::start().await;

            Mock::given(method("POST"))
                .and(path("/audio/transcriptions"))
                .and(header("authorization", "Bearer test-api-key"))
                .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                    "text": "Hello, world!",
                    "language": "en",
                    "duration": 2.5
                })))
                .expect(1)
                .mount(&mock_server)
                .await;

            let provider = create_test_provider(&mock_server);
            let audio = AudioData::new(vec![0, 1, 2, 3], AudioFormat::Mp3);

            let result = provider.transcribe(audio).await;

            assert!(result.is_ok());
            let transcription = result.unwrap();
            assert_eq!(transcription.text, "Hello, world!");
            assert_eq!(transcription.language, Some("en".to_string()));
            assert_eq!(transcription.duration_ms, Some(2500));
        }

        #[tokio::test]
        async fn transcribe_with_language_success() {
            let mock_server = MockServer::start().await;

            Mock::given(method("POST"))
                .and(path("/audio/transcriptions"))
                .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                    "text": "Hallo Welt!",
                    "duration": 1.5
                })))
                .expect(1)
                .mount(&mock_server)
                .await;

            let provider = create_test_provider(&mock_server);
            let audio = AudioData::new(vec![0, 1, 2, 3], AudioFormat::Mp3);

            let result = provider.transcribe_with_language(audio, "de").await;

            assert!(result.is_ok());
            let transcription = result.unwrap();
            assert_eq!(transcription.text, "Hallo Welt!");
            assert_eq!(transcription.language, Some("de".to_string()));
        }

        #[tokio::test]
        async fn transcribe_empty_audio_fails() {
            let mock_server = MockServer::start().await;
            let provider = create_test_provider(&mock_server);
            let audio = AudioData::new(vec![], AudioFormat::Mp3);

            let result = provider.transcribe(audio).await;

            assert!(matches!(result, Err(SpeechError::InvalidAudio(_))));
        }

        #[tokio::test]
        async fn transcribe_unsupported_format_fails() {
            let mock_server = MockServer::start().await;
            let provider = create_test_provider(&mock_server);
            // Opus is not directly supported by Whisper
            let audio = AudioData::new(vec![1, 2, 3], AudioFormat::Opus);

            let result = provider.transcribe(audio).await;

            assert!(matches!(result, Err(SpeechError::InvalidAudio(_))));
        }

        #[tokio::test]
        async fn transcribe_audio_too_long() {
            let mock_server = MockServer::start().await;
            let provider = create_test_provider(&mock_server);
            // Audio with duration exceeding max
            let audio = AudioData::new(vec![1, 2, 3], AudioFormat::Mp3).with_duration(200_000);

            let result = provider.transcribe(audio).await;

            assert!(matches!(
                result,
                Err(SpeechError::AudioTooLong {
                    duration_ms: 200_000,
                    max_ms: 120_000
                })
            ));
        }

        #[tokio::test]
        async fn transcribe_rate_limited() {
            let mock_server = MockServer::start().await;

            Mock::given(method("POST"))
                .and(path("/audio/transcriptions"))
                .respond_with(ResponseTemplate::new(429).set_body_json(serde_json::json!({
                    "error": {
                        "message": "Rate limit exceeded",
                        "type": "rate_limit_error",
                        "code": "rate_limit_exceeded"
                    }
                })))
                .expect(1)
                .mount(&mock_server)
                .await;

            let provider = create_test_provider(&mock_server);
            let audio = AudioData::new(vec![1, 2, 3], AudioFormat::Mp3);

            let result = provider.transcribe(audio).await;

            assert!(matches!(result, Err(SpeechError::RateLimited)));
        }
    }

    mod tts_tests {
        use super::*;

        #[tokio::test]
        async fn synthesize_success() {
            let mock_server = MockServer::start().await;

            let audio_bytes = vec![0u8; 1024]; // Fake audio data

            Mock::given(method("POST"))
                .and(path("/audio/speech"))
                .and(header("authorization", "Bearer test-api-key"))
                .respond_with(ResponseTemplate::new(200).set_body_bytes(audio_bytes.clone()))
                .expect(1)
                .mount(&mock_server)
                .await;

            let provider = create_test_provider(&mock_server);

            let result = provider.synthesize("Hello, world!", None).await;

            assert!(result.is_ok());
            let audio = result.unwrap();
            assert_eq!(audio.size_bytes(), 1024);
        }

        #[tokio::test]
        async fn synthesize_with_voice() {
            let mock_server = MockServer::start().await;

            Mock::given(method("POST"))
                .and(path("/audio/speech"))
                .respond_with(ResponseTemplate::new(200).set_body_bytes(vec![0u8; 512]))
                .expect(1)
                .mount(&mock_server)
                .await;

            let provider = create_test_provider(&mock_server);

            let result = provider.synthesize("Test", Some("alloy")).await;

            assert!(result.is_ok());
        }

        #[tokio::test]
        async fn synthesize_with_format() {
            let mock_server = MockServer::start().await;

            Mock::given(method("POST"))
                .and(path("/audio/speech"))
                .respond_with(ResponseTemplate::new(200).set_body_bytes(vec![0u8; 256]))
                .expect(1)
                .mount(&mock_server)
                .await;

            let provider = create_test_provider(&mock_server);

            let result = provider
                .synthesize_with_format("Test", None, AudioFormat::Mp3)
                .await;

            assert!(result.is_ok());
            assert_eq!(result.unwrap().format(), AudioFormat::Mp3);
        }

        #[tokio::test]
        async fn synthesize_empty_text_fails() {
            let mock_server = MockServer::start().await;
            let provider = create_test_provider(&mock_server);

            let result = provider.synthesize("", None).await;

            assert!(matches!(result, Err(SpeechError::SynthesisFailed(_))));
        }

        #[tokio::test]
        async fn synthesize_text_too_long_fails() {
            let mock_server = MockServer::start().await;
            let provider = create_test_provider(&mock_server);

            let long_text = "a".repeat(5000);
            let result = provider.synthesize(&long_text, None).await;

            assert!(matches!(result, Err(SpeechError::SynthesisFailed(_))));
        }

        #[tokio::test]
        async fn synthesize_rate_limited() {
            let mock_server = MockServer::start().await;

            Mock::given(method("POST"))
                .and(path("/audio/speech"))
                .respond_with(ResponseTemplate::new(429).set_body_json(serde_json::json!({
                    "error": {
                        "message": "Rate limit exceeded",
                        "type": "rate_limit_error",
                        "code": "rate_limit_exceeded"
                    }
                })))
                .expect(1)
                .mount(&mock_server)
                .await;

            let provider = create_test_provider(&mock_server);

            let result = provider.synthesize("Test", None).await;

            assert!(matches!(result, Err(SpeechError::RateLimited)));
        }
    }

    mod availability_tests {
        use super::*;

        #[tokio::test]
        async fn is_available_when_api_responds() {
            let mock_server = MockServer::start().await;

            Mock::given(method("GET"))
                .and(path("/models"))
                .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                    "data": []
                })))
                .mount(&mock_server)
                .await;

            let provider = create_test_provider(&mock_server);

            assert!(SpeechToText::is_available(&provider).await);
        }

        #[tokio::test]
        async fn is_not_available_when_api_fails() {
            let mock_server = MockServer::start().await;

            Mock::given(method("GET"))
                .and(path("/models"))
                .respond_with(ResponseTemplate::new(500))
                .mount(&mock_server)
                .await;

            let provider = create_test_provider(&mock_server);

            assert!(!SpeechToText::is_available(&provider).await);
        }
    }

    mod voice_tests {
        use super::*;

        #[tokio::test]
        async fn list_voices_returns_all_openai_voices() {
            let mock_server = MockServer::start().await;
            let provider = create_test_provider(&mock_server);

            let voices = provider.list_voices().await.unwrap();

            assert_eq!(voices.len(), 6);

            let voice_ids: Vec<&str> = voices.iter().map(|v| v.id.as_str()).collect();
            assert!(voice_ids.contains(&"alloy"));
            assert!(voice_ids.contains(&"echo"));
            assert!(voice_ids.contains(&"fable"));
            assert!(voice_ids.contains(&"onyx"));
            assert!(voice_ids.contains(&"nova"));
            assert!(voice_ids.contains(&"shimmer"));
        }

        #[test]
        fn default_voice_is_nova() {
            let config = SpeechConfig::test();
            let provider = OpenAISpeechProvider::new(config).unwrap();

            assert_eq!(provider.default_voice(), "nova");
        }

        #[test]
        fn model_names_are_correct() {
            let config = SpeechConfig::test();
            let provider = OpenAISpeechProvider::new(config).unwrap();

            assert_eq!(SpeechToText::model_name(&provider), "whisper-1");
            assert_eq!(TextToSpeech::model_name(&provider), "tts-1");
        }
    }

    mod config_tests {
        use super::*;

        #[test]
        fn new_fails_without_api_key() {
            let config = SpeechConfig::default(); // No API key

            let result = OpenAISpeechProvider::new(config);

            assert!(matches!(result, Err(SpeechError::Configuration(_))));
        }

        #[test]
        fn new_succeeds_with_valid_config() {
            let config = SpeechConfig::test();

            let result = OpenAISpeechProvider::new(config);

            assert!(result.is_ok());
        }
    }

    mod format_conversion_tests {
        use super::*;

        #[test]
        fn audio_format_to_response_format() {
            assert_eq!(
                OpenAISpeechProvider::audio_format_to_response_format(AudioFormat::Mp3),
                "mp3"
            );
            assert_eq!(
                OpenAISpeechProvider::audio_format_to_response_format(AudioFormat::Opus),
                "opus"
            );
            assert_eq!(
                OpenAISpeechProvider::audio_format_to_response_format(AudioFormat::Ogg),
                "opus"
            );
            assert_eq!(
                OpenAISpeechProvider::audio_format_to_response_format(AudioFormat::Wav),
                "wav"
            );
            assert_eq!(
                OpenAISpeechProvider::audio_format_to_response_format(AudioFormat::Flac),
                "flac"
            );
            assert_eq!(
                OpenAISpeechProvider::audio_format_to_response_format(AudioFormat::M4a),
                "aac"
            );
        }

        #[test]
        fn response_format_to_audio_format() {
            assert_eq!(
                OpenAISpeechProvider::response_format_to_audio_format("mp3"),
                AudioFormat::Mp3
            );
            assert_eq!(
                OpenAISpeechProvider::response_format_to_audio_format("opus"),
                AudioFormat::Opus
            );
            assert_eq!(
                OpenAISpeechProvider::response_format_to_audio_format("wav"),
                AudioFormat::Wav
            );
            assert_eq!(
                OpenAISpeechProvider::response_format_to_audio_format("flac"),
                AudioFormat::Flac
            );
            assert_eq!(
                OpenAISpeechProvider::response_format_to_audio_format("aac"),
                AudioFormat::M4a
            );
            assert_eq!(
                OpenAISpeechProvider::response_format_to_audio_format("unknown"),
                AudioFormat::Mp3
            );
        }
    }
}
