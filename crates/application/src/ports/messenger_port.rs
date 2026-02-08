//! Messenger port - Unified interface for messaging platforms
//!
//! This port abstracts messaging operations across different platforms
//! like WhatsApp and Signal, providing a common interface for sending
//! text and audio messages.

#[cfg(test)]
use mockall::automock;

use async_trait::async_trait;
use domain::{MessengerSource, PhoneNumber};
use serde::{Deserialize, Serialize};

use crate::error::ApplicationError;

/// An incoming text message from any messaging platform
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IncomingTextMessage {
    /// Unique message ID from the platform
    pub message_id: String,
    /// Sender's phone number
    pub sender: PhoneNumber,
    /// Text content
    pub text: String,
    /// Timestamp (Unix milliseconds)
    pub timestamp: i64,
    /// Source messenger platform
    pub source: MessengerSource,
}

/// An incoming audio/voice message from any messaging platform
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IncomingAudioMessage {
    /// Unique message ID from the platform
    pub message_id: String,
    /// Sender's phone number
    pub sender: PhoneNumber,
    /// Platform-specific media ID for downloading
    pub media_id: String,
    /// MIME type of the audio
    pub mime_type: String,
    /// Whether this is a voice note (vs regular audio file)
    pub is_voice_note: bool,
    /// Timestamp (Unix milliseconds)
    pub timestamp: i64,
    /// Source messenger platform
    pub source: MessengerSource,
}

/// An outgoing text message to any messaging platform
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutgoingTextMessage {
    /// Recipient's phone number
    pub recipient: PhoneNumber,
    /// Text content
    pub text: String,
    /// Optional: reply to a specific message ID
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reply_to: Option<String>,
}

impl OutgoingTextMessage {
    /// Create a new outgoing text message
    #[must_use]
    pub fn new(recipient: PhoneNumber, text: impl Into<String>) -> Self {
        Self {
            recipient,
            text: text.into(),
            reply_to: None,
        }
    }

    /// Create a reply to an incoming text message
    #[must_use]
    pub fn reply_to_text(incoming: &IncomingTextMessage, text: impl Into<String>) -> Self {
        Self {
            recipient: incoming.sender.clone(),
            text: text.into(),
            reply_to: Some(incoming.message_id.clone()),
        }
    }

    /// Create a reply to an incoming audio message
    #[must_use]
    pub fn reply_to_audio(incoming: &IncomingAudioMessage, text: impl Into<String>) -> Self {
        Self {
            recipient: incoming.sender.clone(),
            text: text.into(),
            reply_to: Some(incoming.message_id.clone()),
        }
    }
}

/// An outgoing audio message to any messaging platform
#[derive(Debug, Clone)]
pub struct OutgoingAudioMessage {
    /// Recipient's phone number
    pub recipient: PhoneNumber,
    /// Audio data bytes
    pub audio_data: Vec<u8>,
    /// MIME type (e.g., "audio/ogg; codecs=opus")
    pub mime_type: String,
    /// Optional: reply to a specific message ID
    pub reply_to: Option<String>,
}

impl OutgoingAudioMessage {
    /// Create a new outgoing audio message
    #[must_use]
    pub fn new(recipient: PhoneNumber, audio_data: Vec<u8>, mime_type: impl Into<String>) -> Self {
        Self {
            recipient,
            audio_data,
            mime_type: mime_type.into(),
            reply_to: None,
        }
    }

    /// Create an audio reply to an incoming audio message
    #[must_use]
    pub fn reply_to(
        incoming: &IncomingAudioMessage,
        audio_data: Vec<u8>,
        mime_type: impl Into<String>,
    ) -> Self {
        Self {
            recipient: incoming.sender.clone(),
            audio_data,
            mime_type: mime_type.into(),
            reply_to: Some(incoming.message_id.clone()),
        }
    }
}

/// Result of downloading audio from a messenger platform
#[derive(Debug, Clone)]
pub struct DownloadedAudio {
    /// Raw audio bytes
    pub data: Vec<u8>,
    /// MIME type of the audio
    pub mime_type: String,
}

/// Unified port for messaging platform operations
///
/// Implementations of this trait provide messaging capabilities for
/// specific platforms (WhatsApp, Signal, etc.). The application layer
/// uses this trait to abstract away platform-specific details.
#[cfg_attr(test, automock)]
#[async_trait]
pub trait MessengerPort: Send + Sync {
    /// Get the messenger platform this adapter handles
    fn source(&self) -> MessengerSource;

    /// Check if the messaging service is available
    async fn is_available(&self) -> bool;

    /// Check if a phone number is whitelisted for messaging
    async fn is_whitelisted(&self, phone: &PhoneNumber) -> bool;

    /// Send a text message
    ///
    /// Returns the platform's message ID for the sent message.
    async fn send_text(&self, message: OutgoingTextMessage) -> Result<String, ApplicationError>;

    /// Send an audio/voice message
    ///
    /// Returns the platform's message ID for the sent message.
    async fn send_audio(&self, message: OutgoingAudioMessage) -> Result<String, ApplicationError>;

    /// Download audio data from a media ID
    ///
    /// Platform-specific media IDs are converted to downloadable audio data.
    async fn download_audio(&self, media_id: &str) -> Result<DownloadedAudio, ApplicationError>;

    /// Mark a message as read/processed
    async fn mark_read(&self, message_id: &str) -> Result<(), ApplicationError>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use domain::PhoneNumber;

    fn test_phone() -> PhoneNumber {
        PhoneNumber::new("+491234567890").unwrap()
    }

    mod incoming_text_message_tests {
        use super::*;

        #[test]
        fn creation() {
            let msg = IncomingTextMessage {
                message_id: "msg123".to_string(),
                sender: test_phone(),
                text: "Hello".to_string(),
                timestamp: 1_234_567_890,
                source: MessengerSource::WhatsApp,
            };
            assert_eq!(msg.message_id, "msg123");
            assert_eq!(msg.text, "Hello");
            assert_eq!(msg.source, MessengerSource::WhatsApp);
        }

        #[test]
        fn serialization_roundtrip() {
            let msg = IncomingTextMessage {
                message_id: "msg123".to_string(),
                sender: test_phone(),
                text: "Hello".to_string(),
                timestamp: 1_234_567_890,
                source: MessengerSource::Signal,
            };
            let json = serde_json::to_string(&msg).unwrap();
            let deserialized: IncomingTextMessage = serde_json::from_str(&json).unwrap();
            assert_eq!(msg.message_id, deserialized.message_id);
            assert_eq!(msg.source, deserialized.source);
        }
    }

    mod incoming_audio_message_tests {
        use super::*;

        #[test]
        fn creation() {
            let msg = IncomingAudioMessage {
                message_id: "audio123".to_string(),
                sender: test_phone(),
                media_id: "media456".to_string(),
                mime_type: "audio/ogg".to_string(),
                is_voice_note: true,
                timestamp: 1_234_567_890,
                source: MessengerSource::WhatsApp,
            };
            assert_eq!(msg.media_id, "media456");
            assert!(msg.is_voice_note);
        }

        #[test]
        fn serialization_roundtrip() {
            let msg = IncomingAudioMessage {
                message_id: "audio123".to_string(),
                sender: test_phone(),
                media_id: "media456".to_string(),
                mime_type: "audio/ogg".to_string(),
                is_voice_note: false,
                timestamp: 1_234_567_890,
                source: MessengerSource::Signal,
            };
            let json = serde_json::to_string(&msg).unwrap();
            let deserialized: IncomingAudioMessage = serde_json::from_str(&json).unwrap();
            assert_eq!(msg.media_id, deserialized.media_id);
            assert!(!deserialized.is_voice_note);
        }
    }

    mod outgoing_text_message_tests {
        use super::*;

        #[test]
        fn new_creates_message() {
            let msg = OutgoingTextMessage::new(test_phone(), "Hello");
            assert_eq!(msg.text, "Hello");
            assert!(msg.reply_to.is_none());
        }

        #[test]
        fn reply_to_text_sets_reply_id() {
            let incoming = IncomingTextMessage {
                message_id: "orig123".to_string(),
                sender: test_phone(),
                text: "Original".to_string(),
                timestamp: 1_234_567_890,
                source: MessengerSource::WhatsApp,
            };
            let reply = OutgoingTextMessage::reply_to_text(&incoming, "Reply");
            assert_eq!(reply.text, "Reply");
            assert_eq!(reply.reply_to, Some("orig123".to_string()));
        }

        #[test]
        fn reply_to_audio_sets_reply_id() {
            let incoming = IncomingAudioMessage {
                message_id: "audio123".to_string(),
                sender: test_phone(),
                media_id: "media456".to_string(),
                mime_type: "audio/ogg".to_string(),
                is_voice_note: true,
                timestamp: 1_234_567_890,
                source: MessengerSource::Signal,
            };
            let reply = OutgoingTextMessage::reply_to_audio(&incoming, "Reply");
            assert_eq!(reply.reply_to, Some("audio123".to_string()));
        }

        #[test]
        fn serialization_skips_none_reply_to() {
            let msg = OutgoingTextMessage::new(test_phone(), "Hi");
            let json = serde_json::to_string(&msg).unwrap();
            assert!(!json.contains("reply_to"));
        }
    }

    mod outgoing_audio_message_tests {
        use super::*;

        #[test]
        fn new_creates_message() {
            let audio = vec![0u8, 1, 2, 3];
            let msg = OutgoingAudioMessage::new(test_phone(), audio.clone(), "audio/ogg");
            assert_eq!(msg.audio_data, audio);
            assert_eq!(msg.mime_type, "audio/ogg");
            assert!(msg.reply_to.is_none());
        }

        #[test]
        fn reply_to_sets_reply_id() {
            let incoming = IncomingAudioMessage {
                message_id: "audio123".to_string(),
                sender: test_phone(),
                media_id: "media456".to_string(),
                mime_type: "audio/ogg".to_string(),
                is_voice_note: true,
                timestamp: 1_234_567_890,
                source: MessengerSource::WhatsApp,
            };
            let audio = vec![0u8, 1, 2, 3];
            let reply = OutgoingAudioMessage::reply_to(&incoming, audio, "audio/ogg");
            assert_eq!(reply.reply_to, Some("audio123".to_string()));
        }
    }

    mod downloaded_audio_tests {
        use super::*;

        #[test]
        fn creation() {
            let audio = DownloadedAudio {
                data: vec![0u8, 1, 2, 3],
                mime_type: "audio/ogg".to_string(),
            };
            assert_eq!(audio.data.len(), 4);
            assert_eq!(audio.mime_type, "audio/ogg");
        }

        #[test]
        fn clone() {
            let audio = DownloadedAudio {
                data: vec![0u8, 1, 2, 3],
                mime_type: "audio/ogg".to_string(),
            };
            #[allow(clippy::redundant_clone)]
            let cloned = audio.clone();
            assert_eq!(audio.data, cloned.data);
        }
    }
}
