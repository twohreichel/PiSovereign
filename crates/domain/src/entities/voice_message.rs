//! Voice message entity

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::value_objects::{ConversationId, MessengerSource};

/// Source of a voice message
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VoiceMessageSource {
    /// User sent a voice message via a messenger platform
    MessengerUser(MessengerSource),
    /// AI assistant generated a voice response
    AssistantResponse,
}

impl VoiceMessageSource {
    /// Create a source for a WhatsApp user message
    #[must_use]
    pub const fn whatsapp_user() -> Self {
        Self::MessengerUser(MessengerSource::WhatsApp)
    }

    /// Create a source for a Signal user message
    #[must_use]
    pub const fn signal_user() -> Self {
        Self::MessengerUser(MessengerSource::Signal)
    }

    /// Check if this is a user message from any messenger
    #[must_use]
    pub const fn is_user_message(&self) -> bool {
        matches!(self, Self::MessengerUser(_))
    }

    /// Get the messenger source if this is a user message
    #[must_use]
    pub const fn messenger(&self) -> Option<MessengerSource> {
        match self {
            Self::MessengerUser(source) => Some(*source),
            Self::AssistantResponse => None,
        }
    }
}

/// Format of the audio data
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AudioFormat {
    /// Opus codec (WhatsApp voice messages)
    Opus,
    /// OGG container
    Ogg,
    /// MP3 format
    Mp3,
    /// WAV format
    Wav,
}

impl AudioFormat {
    /// Get the MIME type for this format
    #[must_use]
    pub const fn mime_type(&self) -> &'static str {
        match self {
            Self::Opus => "audio/opus",
            Self::Ogg => "audio/ogg",
            Self::Mp3 => "audio/mpeg",
            Self::Wav => "audio/wav",
        }
    }

    /// Get the file extension for this format
    #[must_use]
    pub const fn extension(&self) -> &'static str {
        match self {
            Self::Opus => "opus",
            Self::Ogg => "ogg",
            Self::Mp3 => "mp3",
            Self::Wav => "wav",
        }
    }

    /// Parse from MIME type
    #[must_use]
    pub fn from_mime_type(mime: &str) -> Option<Self> {
        let base = mime.split(';').next().unwrap_or(mime).trim();
        match base {
            "audio/opus" | "audio/ogg; codecs=opus" => Some(Self::Opus),
            "audio/ogg" => Some(Self::Ogg),
            "audio/mpeg" | "audio/mp3" => Some(Self::Mp3),
            "audio/wav" | "audio/x-wav" => Some(Self::Wav),
            _ => None,
        }
    }
}

impl std::fmt::Display for AudioFormat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Opus => write!(f, "opus"),
            Self::Ogg => write!(f, "ogg"),
            Self::Mp3 => write!(f, "mp3"),
            Self::Wav => write!(f, "wav"),
        }
    }
}

/// Processing status of a voice message
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VoiceMessageStatus {
    /// Voice message received, awaiting transcription
    Received,
    /// Currently being transcribed
    Transcribing,
    /// Transcription complete
    Transcribed,
    /// Processing the transcription through AI
    Processing,
    /// AI response generated, awaiting synthesis
    Processed,
    /// Synthesizing audio response
    Synthesizing,
    /// Audio response ready
    ResponseReady,
    /// Response delivered to user
    Delivered,
    /// Processing failed
    Failed,
}

impl VoiceMessageStatus {
    /// Check if the status indicates completion
    #[must_use]
    pub const fn is_terminal(&self) -> bool {
        matches!(self, Self::Delivered | Self::Failed)
    }

    /// Check if the message is still being processed
    #[must_use]
    pub const fn is_processing(&self) -> bool {
        matches!(
            self,
            Self::Received
                | Self::Transcribing
                | Self::Transcribed
                | Self::Processing
                | Self::Processed
                | Self::Synthesizing
                | Self::ResponseReady
        )
    }
}

/// A voice message in the system
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VoiceMessage {
    /// Unique identifier
    pub id: Uuid,
    /// Associated conversation
    pub conversation_id: ConversationId,
    /// Source of the message
    pub source: VoiceMessageSource,
    /// Audio format
    pub audio_format: AudioFormat,
    /// Audio duration in milliseconds (if known)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<u64>,
    /// Audio data size in bytes
    pub size_bytes: usize,
    /// Current processing status
    pub status: VoiceMessageStatus,
    /// Transcribed text (after STT processing)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub transcription: Option<String>,
    /// Detected language code (e.g., "en", "de")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detected_language: Option<String>,
    /// Transcription confidence (0.0 - 1.0)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub transcription_confidence: Option<f32>,
    /// When the voice message was created
    pub created_at: DateTime<Utc>,
    /// When the message was last updated
    pub updated_at: DateTime<Utc>,
    /// External message ID (e.g., WhatsApp message ID)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub external_id: Option<String>,
    /// Error message if processing failed
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl VoiceMessage {
    /// Create a new incoming voice message from a specific messenger
    #[must_use]
    pub fn new_incoming_from_messenger(
        conversation_id: ConversationId,
        audio_format: AudioFormat,
        size_bytes: usize,
        source: MessengerSource,
    ) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            conversation_id,
            source: VoiceMessageSource::MessengerUser(source),
            audio_format,
            duration_ms: None,
            size_bytes,
            status: VoiceMessageStatus::Received,
            transcription: None,
            detected_language: None,
            transcription_confidence: None,
            created_at: now,
            updated_at: now,
            external_id: None,
            error: None,
        }
    }

    /// Create a new incoming voice message (defaults to WhatsApp for backward compatibility)
    #[must_use]
    pub fn new_incoming(
        conversation_id: ConversationId,
        audio_format: AudioFormat,
        size_bytes: usize,
    ) -> Self {
        Self::new_incoming_from_messenger(
            conversation_id,
            audio_format,
            size_bytes,
            MessengerSource::WhatsApp,
        )
    }

    /// Create a new assistant voice response
    #[must_use]
    pub fn new_response(
        conversation_id: ConversationId,
        audio_format: AudioFormat,
        size_bytes: usize,
    ) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            conversation_id,
            source: VoiceMessageSource::AssistantResponse,
            audio_format,
            duration_ms: None,
            size_bytes,
            status: VoiceMessageStatus::ResponseReady,
            transcription: None,
            detected_language: None,
            transcription_confidence: None,
            created_at: now,
            updated_at: now,
            external_id: None,
            error: None,
        }
    }

    /// Set the external message ID (e.g., WhatsApp message ID)
    #[must_use]
    pub fn with_external_id(mut self, external_id: impl Into<String>) -> Self {
        self.external_id = Some(external_id.into());
        self
    }

    /// Set the audio duration
    #[must_use]
    pub const fn with_duration_ms(mut self, duration_ms: u64) -> Self {
        self.duration_ms = Some(duration_ms);
        self
    }

    /// Mark as transcribing
    pub fn start_transcription(&mut self) {
        self.status = VoiceMessageStatus::Transcribing;
        self.updated_at = Utc::now();
    }

    /// Complete transcription with result
    pub fn complete_transcription(
        &mut self,
        text: String,
        language: Option<String>,
        confidence: Option<f32>,
    ) {
        self.transcription = Some(text);
        self.detected_language = language;
        self.transcription_confidence = confidence;
        self.status = VoiceMessageStatus::Transcribed;
        self.updated_at = Utc::now();
    }

    /// Mark as processing through AI
    pub fn start_processing(&mut self) {
        self.status = VoiceMessageStatus::Processing;
        self.updated_at = Utc::now();
    }

    /// Mark AI processing as complete
    pub fn complete_processing(&mut self) {
        self.status = VoiceMessageStatus::Processed;
        self.updated_at = Utc::now();
    }

    /// Mark as synthesizing audio
    pub fn start_synthesis(&mut self) {
        self.status = VoiceMessageStatus::Synthesizing;
        self.updated_at = Utc::now();
    }

    /// Mark synthesis as complete
    pub fn complete_synthesis(&mut self) {
        self.status = VoiceMessageStatus::ResponseReady;
        self.updated_at = Utc::now();
    }

    /// Mark as delivered to user
    pub fn mark_delivered(&mut self) {
        self.status = VoiceMessageStatus::Delivered;
        self.updated_at = Utc::now();
    }

    /// Mark as failed with error
    pub fn mark_failed(&mut self, error: impl Into<String>) {
        self.status = VoiceMessageStatus::Failed;
        self.error = Some(error.into());
        self.updated_at = Utc::now();
    }

    /// Get the transcription text if available
    #[must_use]
    pub fn text(&self) -> Option<&str> {
        self.transcription.as_deref()
    }

    /// Check if this is an incoming user message
    #[must_use]
    pub const fn is_user_message(&self) -> bool {
        self.source.is_user_message()
    }

    /// Get the messenger source if this is a user message
    #[must_use]
    pub const fn messenger(&self) -> Option<MessengerSource> {
        self.source.messenger()
    }

    /// Check if this is an assistant response
    #[must_use]
    pub const fn is_assistant_response(&self) -> bool {
        matches!(self.source, VoiceMessageSource::AssistantResponse)
    }

    /// Check if processing is complete
    #[must_use]
    pub const fn is_complete(&self) -> bool {
        self.status.is_terminal()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_conversation_id() -> ConversationId {
        ConversationId::new()
    }

    mod audio_format_tests {
        use super::*;

        #[test]
        fn mime_types_are_correct() {
            assert_eq!(AudioFormat::Opus.mime_type(), "audio/opus");
            assert_eq!(AudioFormat::Ogg.mime_type(), "audio/ogg");
            assert_eq!(AudioFormat::Mp3.mime_type(), "audio/mpeg");
            assert_eq!(AudioFormat::Wav.mime_type(), "audio/wav");
        }

        #[test]
        fn extensions_are_correct() {
            assert_eq!(AudioFormat::Opus.extension(), "opus");
            assert_eq!(AudioFormat::Ogg.extension(), "ogg");
            assert_eq!(AudioFormat::Mp3.extension(), "mp3");
            assert_eq!(AudioFormat::Wav.extension(), "wav");
        }

        #[test]
        fn from_mime_type_parses_correctly() {
            assert_eq!(
                AudioFormat::from_mime_type("audio/opus"),
                Some(AudioFormat::Opus)
            );
            assert_eq!(
                AudioFormat::from_mime_type("audio/ogg"),
                Some(AudioFormat::Ogg)
            );
            assert_eq!(
                AudioFormat::from_mime_type("audio/mpeg"),
                Some(AudioFormat::Mp3)
            );
            assert_eq!(
                AudioFormat::from_mime_type("audio/wav"),
                Some(AudioFormat::Wav)
            );
            assert_eq!(AudioFormat::from_mime_type("audio/unknown"), None);
        }

        #[test]
        fn display_formats_correctly() {
            assert_eq!(format!("{}", AudioFormat::Opus), "opus");
            assert_eq!(format!("{}", AudioFormat::Mp3), "mp3");
        }
    }

    mod voice_message_status_tests {
        use super::*;

        #[test]
        fn terminal_states() {
            assert!(VoiceMessageStatus::Delivered.is_terminal());
            assert!(VoiceMessageStatus::Failed.is_terminal());
            assert!(!VoiceMessageStatus::Received.is_terminal());
            assert!(!VoiceMessageStatus::Transcribing.is_terminal());
        }

        #[test]
        fn processing_states() {
            assert!(VoiceMessageStatus::Received.is_processing());
            assert!(VoiceMessageStatus::Transcribing.is_processing());
            assert!(VoiceMessageStatus::Processing.is_processing());
            assert!(!VoiceMessageStatus::Delivered.is_processing());
            assert!(!VoiceMessageStatus::Failed.is_processing());
        }
    }

    mod voice_message_tests {
        use super::*;

        #[test]
        fn new_incoming_creates_correctly() {
            let conv_id = sample_conversation_id();
            let msg = VoiceMessage::new_incoming(conv_id, AudioFormat::Opus, 1024);

            assert_eq!(msg.conversation_id, conv_id);
            assert_eq!(msg.source, VoiceMessageSource::whatsapp_user());
            assert_eq!(msg.audio_format, AudioFormat::Opus);
            assert_eq!(msg.size_bytes, 1024);
            assert_eq!(msg.status, VoiceMessageStatus::Received);
            assert!(msg.transcription.is_none());
            assert!(msg.is_user_message());
            assert!(!msg.is_assistant_response());
            assert_eq!(msg.messenger(), Some(MessengerSource::WhatsApp));
        }

        #[test]
        fn new_incoming_from_signal_creates_correctly() {
            let conv_id = sample_conversation_id();
            let msg = VoiceMessage::new_incoming_from_messenger(
                conv_id,
                AudioFormat::Opus,
                1024,
                MessengerSource::Signal,
            );

            assert_eq!(msg.source, VoiceMessageSource::signal_user());
            assert!(msg.is_user_message());
            assert_eq!(msg.messenger(), Some(MessengerSource::Signal));
        }

        #[test]
        fn new_response_creates_correctly() {
            let conv_id = sample_conversation_id();
            let msg = VoiceMessage::new_response(conv_id, AudioFormat::Opus, 2048);

            assert_eq!(msg.conversation_id, conv_id);
            assert_eq!(msg.source, VoiceMessageSource::AssistantResponse);
            assert_eq!(msg.status, VoiceMessageStatus::ResponseReady);
            assert!(msg.is_assistant_response());
            assert!(!msg.is_user_message());
            assert_eq!(msg.messenger(), None);
        }

        #[test]
        fn with_external_id_sets_id() {
            let msg = VoiceMessage::new_incoming(sample_conversation_id(), AudioFormat::Opus, 1024)
                .with_external_id("wamid.1234567890");

            assert_eq!(msg.external_id, Some("wamid.1234567890".to_string()));
        }

        #[test]
        fn with_duration_sets_duration() {
            let msg = VoiceMessage::new_incoming(sample_conversation_id(), AudioFormat::Opus, 1024)
                .with_duration_ms(5000);

            assert_eq!(msg.duration_ms, Some(5000));
        }

        #[test]
        fn transcription_workflow() {
            let mut msg =
                VoiceMessage::new_incoming(sample_conversation_id(), AudioFormat::Opus, 1024);

            // Start transcription
            msg.start_transcription();
            assert_eq!(msg.status, VoiceMessageStatus::Transcribing);

            // Complete transcription
            msg.complete_transcription(
                "Hello, world!".to_string(),
                Some("en".to_string()),
                Some(0.95),
            );

            assert_eq!(msg.status, VoiceMessageStatus::Transcribed);
            assert_eq!(msg.text(), Some("Hello, world!"));
            assert_eq!(msg.detected_language, Some("en".to_string()));
            assert_eq!(msg.transcription_confidence, Some(0.95));
        }

        #[test]
        fn full_processing_workflow() {
            let mut msg =
                VoiceMessage::new_incoming(sample_conversation_id(), AudioFormat::Opus, 1024);

            msg.start_transcription();
            msg.complete_transcription("Test".to_string(), None, None);
            msg.start_processing();
            assert_eq!(msg.status, VoiceMessageStatus::Processing);

            msg.complete_processing();
            assert_eq!(msg.status, VoiceMessageStatus::Processed);

            msg.start_synthesis();
            assert_eq!(msg.status, VoiceMessageStatus::Synthesizing);

            msg.complete_synthesis();
            assert_eq!(msg.status, VoiceMessageStatus::ResponseReady);

            msg.mark_delivered();
            assert_eq!(msg.status, VoiceMessageStatus::Delivered);
            assert!(msg.is_complete());
        }

        #[test]
        fn mark_failed_sets_error() {
            let mut msg =
                VoiceMessage::new_incoming(sample_conversation_id(), AudioFormat::Opus, 1024);

            msg.mark_failed("Transcription timeout");

            assert_eq!(msg.status, VoiceMessageStatus::Failed);
            assert_eq!(msg.error, Some("Transcription timeout".to_string()));
            assert!(msg.is_complete());
        }
    }
}
