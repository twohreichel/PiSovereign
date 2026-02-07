//! Integration tests for ai_speech crate
//!
//! Tests the full voice message flow with mocked OpenAI APIs.

use ai_speech::{
    AudioConverter, AudioData, AudioFormat, OpenAISpeechProvider, SpeechConfig, SpeechToText,
    TextToSpeech,
};
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// Create a test configuration pointing to mock server
fn test_config(base_url: &str) -> SpeechConfig {
    SpeechConfig {
        openai_api_key: Some("test-api-key".to_string()),
        openai_base_url: base_url.to_string(),
        stt_model: "whisper-1".to_string(),
        tts_model: "tts-1".to_string(),
        default_voice: "nova".to_string(),
        output_format: AudioFormat::Opus,
        timeout_ms: 5000,
        max_audio_duration_ms: 1_500_000,
        ..Default::default()
    }
}

/// Create mock MP3 audio data (minimal valid MP3 header)
fn mock_mp3_audio() -> Vec<u8> {
    // Minimal MP3 frame header (MPEG Audio Layer 3)
    vec![
        0xFF, 0xFB, 0x90, 0x00, // MP3 frame header
        0x00, 0x00, 0x00, 0x00, // Padding
        0x00, 0x00, 0x00, 0x00, // More padding
    ]
}

/// Create mock Opus audio data
fn mock_opus_audio() -> Vec<u8> {
    // OggS header for Opus audio
    vec![
        0x4F, 0x67, 0x67, 0x53, // OggS
        0x00, 0x02, 0x00, 0x00, // Version and flags
        0x00, 0x00, 0x00, 0x00, // Granule position
        0x00, 0x00, 0x00, 0x00, // Stream serial
        0x00, 0x00, 0x00, 0x00, // Page sequence
        0x00, 0x00, 0x00, 0x00, // Checksum (placeholder)
        0x01, 0x13,             // Page segments
    ]
}

// ============ STT (Transcription) Integration Tests ============

#[tokio::test]
async fn stt_transcription_success() {
    let mock_server = MockServer::start().await;

    // Mock successful transcription response
    Mock::given(method("POST"))
        .and(path("/audio/transcriptions"))
        .and(header("Authorization", "Bearer test-api-key"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "text": "Hello, this is a test transcription.",
            "language": "en",
            "duration": 2.5
        })))
        .expect(1)
        .mount(&mock_server)
        .await;

    let config = test_config(&mock_server.uri());
    let provider = OpenAISpeechProvider::new(config).expect("Failed to create provider");

    let audio = AudioData::new(mock_mp3_audio(), AudioFormat::Mp3);
    let result = provider.transcribe(audio).await;

    assert!(result.is_ok(), "Transcription should succeed");
    let transcription = result.unwrap();
    assert_eq!(transcription.text, "Hello, this is a test transcription.");
    assert_eq!(transcription.language, Some("en".to_string()));
}

#[tokio::test]
async fn stt_transcription_with_language_hint() {
    let mock_server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/audio/transcriptions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "text": "Hallo, das ist ein Test.",
            "language": "de",
            "duration": 1.8
        })))
        .expect(1)
        .mount(&mock_server)
        .await;

    let config = test_config(&mock_server.uri());
    let provider = OpenAISpeechProvider::new(config).expect("Failed to create provider");

    let audio = AudioData::new(mock_mp3_audio(), AudioFormat::Mp3);
    let result = provider.transcribe_with_language(audio, "de").await;

    assert!(result.is_ok());
    let transcription = result.unwrap();
    assert_eq!(transcription.text, "Hallo, das ist ein Test.");
    assert_eq!(transcription.language, Some("de".to_string()));
}

#[tokio::test]
async fn stt_transcription_api_error() {
    let mock_server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/audio/transcriptions"))
        .respond_with(ResponseTemplate::new(400).set_body_json(serde_json::json!({
            "error": {
                "message": "Invalid audio file format",
                "type": "invalid_request_error",
                "code": "invalid_file_format"
            }
        })))
        .expect(1)
        .mount(&mock_server)
        .await;

    let config = test_config(&mock_server.uri());
    let provider = OpenAISpeechProvider::new(config).expect("Failed to create provider");

    let audio = AudioData::new(vec![0x00, 0x01, 0x02], AudioFormat::Mp3);
    let result = provider.transcribe(audio).await;

    assert!(result.is_err(), "Should fail with API error");
}

#[tokio::test]
async fn stt_transcription_rate_limited() {
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

    let config = test_config(&mock_server.uri());
    let provider = OpenAISpeechProvider::new(config).expect("Failed to create provider");

    let audio = AudioData::new(mock_mp3_audio(), AudioFormat::Mp3);
    let result = provider.transcribe(audio).await;

    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(
        matches!(err, ai_speech::SpeechError::RateLimited),
        "Expected RateLimited error, got: {err:?}"
    );
}

// ============ TTS (Synthesis) Integration Tests ============

#[tokio::test]
async fn tts_synthesis_success() {
    let mock_server = MockServer::start().await;

    let response_audio = mock_opus_audio();

    Mock::given(method("POST"))
        .and(path("/audio/speech"))
        .and(header("Authorization", "Bearer test-api-key"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_bytes(response_audio.clone())
                .insert_header("content-type", "audio/opus"),
        )
        .expect(1)
        .mount(&mock_server)
        .await;

    let config = test_config(&mock_server.uri());
    let provider = OpenAISpeechProvider::new(config).expect("Failed to create provider");

    let result = provider.synthesize("Hello world", None).await;

    assert!(result.is_ok(), "Synthesis should succeed");
    let audio = result.unwrap();
    assert!(!audio.data().is_empty(), "Audio data should not be empty");
}

#[tokio::test]
async fn tts_synthesis_with_custom_voice() {
    let mock_server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/audio/speech"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_bytes(mock_opus_audio())
                .insert_header("content-type", "audio/opus"),
        )
        .expect(1)
        .mount(&mock_server)
        .await;

    let config = test_config(&mock_server.uri());
    let provider = OpenAISpeechProvider::new(config).expect("Failed to create provider");

    let result = provider.synthesize("Test message", Some("alloy")).await;

    assert!(result.is_ok());
}

#[tokio::test]
async fn tts_synthesis_api_error() {
    let mock_server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/audio/speech"))
        .respond_with(ResponseTemplate::new(500).set_body_json(serde_json::json!({
            "error": {
                "message": "Internal server error",
                "type": "server_error"
            }
        })))
        .expect(1)
        .mount(&mock_server)
        .await;

    let config = test_config(&mock_server.uri());
    let provider = OpenAISpeechProvider::new(config).expect("Failed to create provider");

    let result = provider.synthesize("Test", None).await;

    assert!(result.is_err(), "Should fail with server error");
}

#[tokio::test]
async fn tts_list_voices_returns_openai_voices() {
    let config = SpeechConfig {
        openai_api_key: Some("test-key".to_string()),
        ..Default::default()
    };
    let provider = OpenAISpeechProvider::new(config).expect("Failed to create provider");

    let voices = provider.list_voices().await;

    assert!(voices.is_ok());
    let voice_list = voices.unwrap();
    assert!(!voice_list.is_empty(), "Should have voices");

    // Check for known OpenAI voices
    let voice_ids: Vec<&str> = voice_list.iter().map(|v| v.id.as_str()).collect();
    assert!(voice_ids.contains(&"nova"), "Should contain nova voice");
    assert!(voice_ids.contains(&"alloy"), "Should contain alloy voice");
    assert!(voice_ids.contains(&"echo"), "Should contain echo voice");
}

// ============ Audio Converter Integration Tests ============

#[tokio::test]
async fn converter_detects_whisper_supported_formats() {
    let mp3_audio = AudioData::new(mock_mp3_audio(), AudioFormat::Mp3);
    assert!(
        mp3_audio.format().is_whisper_supported(),
        "MP3 should be Whisper-supported"
    );

    let wav_audio = AudioData::new(vec![0x52, 0x49, 0x46, 0x46], AudioFormat::Wav);
    assert!(
        wav_audio.format().is_whisper_supported(),
        "WAV should be Whisper-supported"
    );

    let ogg_audio = AudioData::new(mock_opus_audio(), AudioFormat::Ogg);
    assert!(
        !ogg_audio.format().is_whisper_supported(),
        "OGG should NOT be Whisper-supported"
    );

    let opus_audio = AudioData::new(mock_opus_audio(), AudioFormat::Opus);
    assert!(
        !opus_audio.format().is_whisper_supported(),
        "Opus should NOT be Whisper-supported"
    );
}

#[tokio::test]
async fn converter_skips_already_supported_formats() {
    let converter = AudioConverter::new();
    let mp3_audio = AudioData::new(mock_mp3_audio(), AudioFormat::Mp3);

    // MP3 is already supported, so conversion should return same format
    let result = converter.convert_for_whisper(&mp3_audio).await;

    assert!(result.is_ok());
    let converted = result.unwrap();
    assert_eq!(
        converted.format(),
        AudioFormat::Mp3,
        "Should keep MP3 format"
    );
}

// ============ Full Flow Integration Tests ============

#[tokio::test]
async fn full_voice_message_flow_text_response() {
    let mock_server = MockServer::start().await;

    // Step 1: Mock STT endpoint
    Mock::given(method("POST"))
        .and(path("/audio/transcriptions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "text": "What's the weather like today?",
            "language": "en",
            "duration": 3.2
        })))
        .expect(1)
        .mount(&mock_server)
        .await;

    let config = test_config(&mock_server.uri());
    let provider = OpenAISpeechProvider::new(config).expect("Failed to create provider");

    // Simulate receiving voice message
    let voice_message_audio = AudioData::new(mock_mp3_audio(), AudioFormat::Mp3);

    // Step 1: Transcribe
    let transcription = provider
        .transcribe(voice_message_audio)
        .await
        .expect("Transcription failed");

    assert_eq!(transcription.text, "What's the weather like today?");
    assert_eq!(transcription.language, Some("en".to_string()));

    // Step 2: AI would process transcription.text here
    // (In real flow, this would call ChatService)
    let ai_response = "The weather is sunny with temperatures around 22°C.";

    // Step 3: Synthesize response (optional based on config)
    // This test validates the transcription part of the flow
    assert!(!ai_response.is_empty());
}

#[tokio::test]
async fn full_voice_message_flow_with_audio_response() {
    let mock_server = MockServer::start().await;

    // Mock STT endpoint
    Mock::given(method("POST"))
        .and(path("/audio/transcriptions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "text": "Send a voice reply",
            "language": "en"
        })))
        .expect(1)
        .mount(&mock_server)
        .await;

    // Mock TTS endpoint
    Mock::given(method("POST"))
        .and(path("/audio/speech"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_bytes(mock_opus_audio())
                .insert_header("content-type", "audio/opus"),
        )
        .expect(1)
        .mount(&mock_server)
        .await;

    let config = test_config(&mock_server.uri());
    let provider = OpenAISpeechProvider::new(config).expect("Failed to create provider");

    // Step 1: Transcribe incoming voice message
    let incoming_audio = AudioData::new(mock_mp3_audio(), AudioFormat::Mp3);
    let transcription = provider
        .transcribe(incoming_audio)
        .await
        .expect("Transcription failed");

    assert_eq!(transcription.text, "Send a voice reply");

    // Step 2: Generate AI response (simulated)
    let ai_response = "Here's your voice reply!";

    // Step 3: Synthesize voice response
    let response_audio = provider
        .synthesize(ai_response, None)
        .await
        .expect("Synthesis failed");

    assert!(!response_audio.data().is_empty());
    // The response should be audio we can send back via WhatsApp
}

// ============ Error Handling Tests ============

#[tokio::test]
async fn handles_network_timeout_gracefully() {
    let mock_server = MockServer::start().await;

    // Mock a slow response that will timeout
    Mock::given(method("POST"))
        .and(path("/audio/transcriptions"))
        .respond_with(ResponseTemplate::new(200).set_delay(std::time::Duration::from_secs(10)))
        .mount(&mock_server)
        .await;

    let mut config = test_config(&mock_server.uri());
    config.timeout_ms = 100; // Very short timeout

    let provider = OpenAISpeechProvider::new(config).expect("Failed to create provider");

    let audio = AudioData::new(mock_mp3_audio(), AudioFormat::Mp3);
    let result = provider.transcribe(audio).await;

    assert!(result.is_err(), "Should timeout");
}

#[tokio::test]
async fn handles_empty_transcription_response() {
    let mock_server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/audio/transcriptions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "text": "",
            "language": "en"
        })))
        .expect(1)
        .mount(&mock_server)
        .await;

    let config = test_config(&mock_server.uri());
    let provider = OpenAISpeechProvider::new(config).expect("Failed to create provider");

    let audio = AudioData::new(mock_mp3_audio(), AudioFormat::Mp3);
    let result = provider.transcribe(audio).await;

    assert!(result.is_ok());
    let transcription = result.unwrap();
    assert!(transcription.text.is_empty(), "Transcription text should be empty");
    assert!(transcription.is_empty(), "is_empty() should return true");
}

// ============ Configuration Validation Tests ============

#[test]
fn config_validates_api_key_required() {
    let config = SpeechConfig {
        openai_api_key: None,
        ..Default::default()
    };

    let result = OpenAISpeechProvider::new(config);
    assert!(result.is_err(), "Should fail without API key");
}

#[test]
fn config_allows_valid_configuration() {
    let config = SpeechConfig {
        openai_api_key: Some("sk-test-key".to_string()),
        ..Default::default()
    };

    let result = OpenAISpeechProvider::new(config);
    assert!(result.is_ok(), "Should succeed with valid config");
}

#[test]
fn config_defaults_are_sensible() {
    let config = SpeechConfig::default();

    assert_eq!(config.stt_model, "whisper-1");
    assert_eq!(config.tts_model, "tts-1");
    assert_eq!(config.default_voice, "nova");
    assert_eq!(config.timeout_ms, 30000); // 30 seconds default
    assert!(config.max_audio_duration_ms > 0);
}
