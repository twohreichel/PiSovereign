//! Speech processing errors

use thiserror::Error;

/// Errors that can occur during speech processing
#[derive(Debug, Error)]
pub enum SpeechError {
    /// Failed to connect to speech service
    #[error("Connection failed: {0}")]
    ConnectionFailed(String),

    /// Request to speech service failed
    #[error("Request failed: {0}")]
    RequestFailed(String),

    /// Invalid audio format or corrupted data
    #[error("Invalid audio: {0}")]
    InvalidAudio(String),

    /// Audio too long for processing
    #[error("Audio too long: {duration_ms}ms exceeds maximum of {max_ms}ms")]
    AudioTooLong {
        /// Duration of the provided audio
        duration_ms: u64,
        /// Maximum allowed duration
        max_ms: u64,
    },

    /// Transcription failed
    #[error("Transcription failed: {0}")]
    TranscriptionFailed(String),

    /// Synthesis failed
    #[error("Synthesis failed: {0}")]
    SynthesisFailed(String),

    /// Invalid response from service
    #[error("Invalid response: {0}")]
    InvalidResponse(String),

    /// Timeout during processing
    #[error("Speech processing timeout after {0}ms")]
    Timeout(u64),

    /// Rate limit exceeded
    #[error("Rate limit exceeded")]
    RateLimited,

    /// Invalid configuration
    #[error("Configuration error: {0}")]
    Configuration(String),

    /// Voice not found
    #[error("Voice not found: {0}")]
    VoiceNotFound(String),

    /// Model not available
    #[error("Model not available: {0}")]
    ModelNotAvailable(String),

    /// Service unavailable
    #[error("Service unavailable: {0}")]
    ServiceUnavailable(String),

    /// Audio processing/conversion failed
    #[error("Audio processing failed: {0}")]
    AudioProcessing(String),

    /// Provider not available (not installed or configured)
    #[error("Provider not available: {0}")]
    NotAvailable(String),
}

impl From<reqwest::Error> for SpeechError {
    fn from(err: reqwest::Error) -> Self {
        if err.is_timeout() {
            Self::Timeout(30000)
        } else if err.is_connect() {
            Self::ConnectionFailed(err.to_string())
        } else {
            Self::RequestFailed(err.to_string())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn connection_failed_error_message() {
        let err = SpeechError::ConnectionFailed("refused".to_string());
        assert_eq!(err.to_string(), "Connection failed: refused");
    }

    #[test]
    fn request_failed_error_message() {
        let err = SpeechError::RequestFailed("500 error".to_string());
        assert_eq!(err.to_string(), "Request failed: 500 error");
    }

    #[test]
    fn invalid_audio_error_message() {
        let err = SpeechError::InvalidAudio("corrupt header".to_string());
        assert_eq!(err.to_string(), "Invalid audio: corrupt header");
    }

    #[test]
    fn audio_too_long_error_message() {
        let err = SpeechError::AudioTooLong {
            duration_ms: 180_000,
            max_ms: 120_000,
        };
        assert_eq!(
            err.to_string(),
            "Audio too long: 180000ms exceeds maximum of 120000ms"
        );
    }

    #[test]
    fn transcription_failed_error_message() {
        let err = SpeechError::TranscriptionFailed("no speech detected".to_string());
        assert_eq!(err.to_string(), "Transcription failed: no speech detected");
    }

    #[test]
    fn synthesis_failed_error_message() {
        let err = SpeechError::SynthesisFailed("invalid text".to_string());
        assert_eq!(err.to_string(), "Synthesis failed: invalid text");
    }

    #[test]
    fn timeout_error_message() {
        let err = SpeechError::Timeout(30000);
        assert_eq!(err.to_string(), "Speech processing timeout after 30000ms");
    }

    #[test]
    fn rate_limited_error_message() {
        let err = SpeechError::RateLimited;
        assert_eq!(err.to_string(), "Rate limit exceeded");
    }

    #[test]
    fn configuration_error_message() {
        let err = SpeechError::Configuration("missing API key".to_string());
        assert_eq!(err.to_string(), "Configuration error: missing API key");
    }

    #[test]
    fn voice_not_found_error_message() {
        let err = SpeechError::VoiceNotFound("custom-voice".to_string());
        assert_eq!(err.to_string(), "Voice not found: custom-voice");
    }

    #[test]
    fn model_not_available_error_message() {
        let err = SpeechError::ModelNotAvailable("whisper-2".to_string());
        assert_eq!(err.to_string(), "Model not available: whisper-2");
    }

    #[test]
    fn service_unavailable_error_message() {
        let err = SpeechError::ServiceUnavailable("maintenance".to_string());
        assert_eq!(err.to_string(), "Service unavailable: maintenance");
    }
}
