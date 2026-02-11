//! Signal messenger adapter
//!
//! Implements the `MessengerPort` trait using the Signal integration crate.

use std::path::Path;

use application::error::ApplicationError;
use application::ports::{
    DownloadedAudio, MessengerPort, OutgoingAudioMessage, OutgoingTextMessage,
};
use async_trait::async_trait;
use domain::{MessengerSource, PhoneNumber};
use integration_signal::{SignalClient, SignalClientConfig, SignalError};
use tokio::fs;
use tracing::{debug, instrument, warn};

/// Adapter that implements `MessengerPort` using `SignalClient`
pub struct SignalMessengerAdapter {
    /// The underlying Signal client
    client: SignalClient,
    /// Temporary directory for audio files
    temp_dir: String,
}

impl SignalMessengerAdapter {
    /// Create a new Signal messenger adapter
    #[must_use]
    pub fn new(config: SignalClientConfig) -> Self {
        Self {
            client: SignalClient::new(config),
            temp_dir: std::env::temp_dir()
                .join("pisovereign-signal")
                .to_string_lossy()
                .to_string(),
        }
    }

    /// Create an adapter with a phone number whitelist
    #[must_use]
    pub fn with_whitelist(config: SignalClientConfig, whitelist: Vec<String>) -> Self {
        Self {
            client: SignalClient::with_whitelist(config, whitelist),
            temp_dir: std::env::temp_dir()
                .join("pisovereign-signal")
                .to_string_lossy()
                .to_string(),
        }
    }

    /// Set the temporary directory for audio files
    #[must_use]
    pub fn with_temp_dir(mut self, temp_dir: impl Into<String>) -> Self {
        self.temp_dir = temp_dir.into();
        self
    }

    /// Get a reference to the underlying client
    #[must_use]
    pub const fn client(&self) -> &SignalClient {
        &self.client
    }

    /// Ensure the temp directory exists
    async fn ensure_temp_dir(&self) -> Result<(), ApplicationError> {
        fs::create_dir_all(&self.temp_dir)
            .await
            .map_err(|e| ApplicationError::Internal(format!("Failed to create temp dir: {e}")))
    }

    /// Write audio data to a temp file and return the path
    async fn write_temp_audio(
        &self,
        data: &[u8],
        mime_type: &str,
    ) -> Result<String, ApplicationError> {
        self.ensure_temp_dir().await?;

        let ext = mime_to_extension(mime_type);
        let filename = format!("{}.{}", uuid::Uuid::new_v4(), ext);
        let path = Path::new(&self.temp_dir).join(&filename);

        fs::write(&path, data)
            .await
            .map_err(|e| ApplicationError::Internal(format!("Failed to write temp file: {e}")))?;

        Ok(path.to_string_lossy().to_string())
    }

    /// Clean up a temp file
    async fn cleanup_temp_file(&self, path: &str) {
        if let Err(e) = fs::remove_file(path).await {
            warn!(path = %path, error = %e, "Failed to cleanup temp file");
        }
    }
}

impl std::fmt::Debug for SignalMessengerAdapter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SignalMessengerAdapter")
            .field("phone_number", &self.client.phone_number())
            .field("temp_dir", &self.temp_dir)
            .finish_non_exhaustive()
    }
}

#[async_trait]
impl MessengerPort for SignalMessengerAdapter {
    fn source(&self) -> MessengerSource {
        MessengerSource::Signal
    }

    #[instrument(skip(self))]
    async fn is_available(&self) -> bool {
        self.client.is_available().await
    }

    async fn is_whitelisted(&self, phone: &PhoneNumber) -> bool {
        self.client.is_whitelisted(phone.as_str())
    }

    #[instrument(skip(self, message), fields(recipient = %message.recipient))]
    async fn send_text(&self, message: OutgoingTextMessage) -> Result<String, ApplicationError> {
        let result = if let Some(ref reply_to) = message.reply_to {
            // Try to parse the reply_to as a timestamp (Signal uses timestamps as message IDs)
            if let Ok(timestamp) = reply_to.parse::<i64>() {
                self.client
                    .send_text_reply(
                        message.recipient.as_str(),
                        &message.text,
                        timestamp,
                        message.recipient.as_str(), // Reply author is the recipient
                    )
                    .await
            } else {
                // If not a valid timestamp, send without reply
                warn!(reply_to = %reply_to, "Invalid reply_to timestamp, sending without reply");
                self.client
                    .send_text(message.recipient.as_str(), &message.text)
                    .await
            }
        } else {
            self.client
                .send_text(message.recipient.as_str(), &message.text)
                .await
        };

        let send_result = result.map_err(|e| match e {
            SignalError::NotRegistered(account) => {
                ApplicationError::Configuration(format!("Signal account not registered: {account}"))
            },
            SignalError::SendFailed(msg) => {
                ApplicationError::ExternalService(format!("Signal send failed: {msg}"))
            },
            SignalError::Connection(msg) => {
                ApplicationError::ExternalService(format!("Signal connection failed: {msg}"))
            },
            e => ApplicationError::ExternalService(format!("Signal error: {e}")),
        })?;

        // Signal uses timestamp as message ID
        let message_id = send_result.timestamp.to_string();
        debug!(message_id = %message_id, "Signal text message sent");
        Ok(message_id)
    }

    #[instrument(skip(self, message), fields(recipient = %message.recipient, audio_size = message.audio_data.len()))]
    async fn send_audio(&self, message: OutgoingAudioMessage) -> Result<String, ApplicationError> {
        // Write audio data to temp file
        let temp_path = self
            .write_temp_audio(&message.audio_data, &message.mime_type)
            .await?;
        debug!(temp_path = %temp_path, "Audio written to temp file");

        // Send the audio
        let result = self
            .client
            .send_audio(message.recipient.as_str(), &temp_path)
            .await;

        // Clean up temp file
        self.cleanup_temp_file(&temp_path).await;

        let send_result = result.map_err(|e| match e {
            SignalError::Media(msg) => {
                ApplicationError::ExternalService(format!("Signal media error: {msg}"))
            },
            e => ApplicationError::ExternalService(format!("Signal error: {e}")),
        })?;

        let message_id = send_result.timestamp.to_string();
        debug!(message_id = %message_id, "Signal audio message sent");
        Ok(message_id)
    }

    #[instrument(skip(self), fields(media_id = %media_id))]
    async fn download_audio(&self, media_id: &str) -> Result<DownloadedAudio, ApplicationError> {
        // Signal stores attachments locally. The media_id is actually a file path.
        let path = Path::new(media_id);

        if !path.exists() {
            return Err(ApplicationError::NotFound(format!(
                "Signal attachment not found: {media_id}"
            )));
        }

        let data = fs::read(path).await.map_err(|e| {
            ApplicationError::ExternalService(format!("Failed to read attachment: {e}"))
        })?;

        // Try to determine MIME type from extension
        let mime_type = path
            .extension()
            .and_then(|ext| ext.to_str())
            .map_or("application/octet-stream", extension_to_mime)
            .to_string();

        debug!(size = data.len(), mime_type = %mime_type, "Audio downloaded from Signal");

        Ok(DownloadedAudio { data, mime_type })
    }

    #[instrument(skip(self), fields(message_id = %message_id))]
    async fn mark_read(&self, message_id: &str) -> Result<(), ApplicationError> {
        // Parse the message ID as a timestamp
        let timestamp: i64 = message_id.parse().map_err(|_| {
            ApplicationError::InvalidOperation(format!("Invalid Signal message ID: {message_id}"))
        })?;

        // We need the sender's phone number to send a read receipt
        // For now, we'll just log it since we don't have the sender context here
        debug!(timestamp = timestamp, "Signal read receipt requested");

        // In a real implementation, you'd need to track the sender:
        // self.client.send_read_receipt(sender, vec![timestamp]).await

        Ok(())
    }
}

/// Convert MIME type to file extension
fn mime_to_extension(mime_type: &str) -> &str {
    let base = mime_type.split(';').next().unwrap_or(mime_type).trim();
    match base {
        "audio/ogg" | "audio/opus" => "ogg",
        "audio/mpeg" | "audio/mp3" => "mp3",
        "audio/wav" | "audio/x-wav" => "wav",
        "audio/mp4" | "audio/m4a" | "audio/aac" => "m4a",
        _ => "bin",
    }
}

/// Convert file extension to MIME type
fn extension_to_mime(ext: &str) -> &str {
    match ext.to_lowercase().as_str() {
        "ogg" | "opus" => "audio/ogg",
        "mp3" => "audio/mpeg",
        "wav" => "audio/wav",
        "m4a" | "aac" => "audio/mp4",
        _ => "application/octet-stream",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mime_to_extension_maps_correctly() {
        assert_eq!(mime_to_extension("audio/ogg"), "ogg");
        assert_eq!(mime_to_extension("audio/opus"), "ogg");
        assert_eq!(mime_to_extension("audio/mpeg"), "mp3");
        assert_eq!(mime_to_extension("audio/mp3"), "mp3");
        assert_eq!(mime_to_extension("audio/wav"), "wav");
        assert_eq!(mime_to_extension("audio/mp4"), "m4a");
        assert_eq!(mime_to_extension("audio/aac"), "m4a");
    }

    #[test]
    fn mime_to_extension_handles_parameters() {
        assert_eq!(mime_to_extension("audio/ogg; codecs=opus"), "ogg");
    }

    #[test]
    fn mime_to_extension_returns_bin_for_unknown() {
        assert_eq!(mime_to_extension("application/octet-stream"), "bin");
    }

    #[test]
    fn extension_to_mime_maps_correctly() {
        assert_eq!(extension_to_mime("ogg"), "audio/ogg");
        assert_eq!(extension_to_mime("opus"), "audio/ogg");
        assert_eq!(extension_to_mime("mp3"), "audio/mpeg");
        assert_eq!(extension_to_mime("wav"), "audio/wav");
        assert_eq!(extension_to_mime("m4a"), "audio/mp4");
    }

    #[test]
    fn extension_to_mime_is_case_insensitive() {
        assert_eq!(extension_to_mime("OGG"), "audio/ogg");
        assert_eq!(extension_to_mime("MP3"), "audio/mpeg");
    }

    #[test]
    fn extension_to_mime_returns_octet_stream_for_unknown() {
        assert_eq!(extension_to_mime("xyz"), "application/octet-stream");
    }

    #[test]
    fn source_returns_signal() {
        assert_eq!(MessengerSource::Signal.display_name(), "Signal");
    }

    #[test]
    fn new_creates_adapter() {
        let config = SignalClientConfig::new("+1234567890");
        let adapter = SignalMessengerAdapter::new(config);
        assert_eq!(adapter.client().phone_number(), "+1234567890");
    }

    #[test]
    fn with_temp_dir_sets_directory() {
        let config = SignalClientConfig::new("+1234567890");
        let adapter = SignalMessengerAdapter::new(config).with_temp_dir("/custom/temp");
        assert_eq!(adapter.temp_dir, "/custom/temp");
    }

    #[test]
    fn debug_format() {
        let config = SignalClientConfig::new("+1234567890");
        let adapter = SignalMessengerAdapter::new(config);
        let debug = format!("{adapter:?}");
        assert!(debug.contains("SignalMessengerAdapter"));
        assert!(debug.contains("+1234567890"));
    }
}
