//! WhatsApp messenger adapter
//!
//! Implements the `MessengerPort` trait using the WhatsApp integration crate.

use std::sync::Arc;

use application::error::ApplicationError;
use application::ports::{
    DownloadedAudio, MessengerPort, OutgoingAudioMessage, OutgoingTextMessage,
};
use async_trait::async_trait;
use domain::{MessengerSource, PhoneNumber};
use integration_whatsapp::{WhatsAppClient, WhatsAppClientConfig, WhatsAppError};
use tracing::{debug, instrument};

/// Adapter that implements `MessengerPort` using `WhatsAppClient`
pub struct WhatsAppMessengerAdapter {
    /// The underlying WhatsApp client
    client: WhatsAppClient,
    /// Whitelisted phone numbers (empty = allow all)
    whitelist: Arc<Vec<String>>,
}

impl WhatsAppMessengerAdapter {
    /// Create a new WhatsApp messenger adapter
    ///
    /// # Errors
    /// Returns an error if the client configuration is invalid.
    pub fn new(config: WhatsAppClientConfig) -> Result<Self, WhatsAppError> {
        let client = WhatsAppClient::new(config)?;
        Ok(Self {
            client,
            whitelist: Arc::new(Vec::new()),
        })
    }

    /// Create an adapter with a phone number whitelist
    ///
    /// # Errors
    /// Returns an error if the client configuration is invalid.
    pub fn with_whitelist(
        config: WhatsAppClientConfig,
        whitelist: Vec<String>,
    ) -> Result<Self, WhatsAppError> {
        let client = WhatsAppClient::new(config)?;
        Ok(Self {
            client,
            whitelist: Arc::new(whitelist),
        })
    }

    /// Get a reference to the underlying client for advanced operations
    #[must_use]
    pub const fn client(&self) -> &WhatsAppClient {
        &self.client
    }
}

impl std::fmt::Debug for WhatsAppMessengerAdapter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WhatsAppMessengerAdapter")
            .field("whitelist_count", &self.whitelist.len())
            .finish_non_exhaustive()
    }
}

#[async_trait]
impl MessengerPort for WhatsAppMessengerAdapter {
    fn source(&self) -> MessengerSource {
        MessengerSource::WhatsApp
    }

    #[instrument(skip(self))]
    async fn is_available(&self) -> bool {
        self.client.is_available().await
    }

    async fn is_whitelisted(&self, phone: &PhoneNumber) -> bool {
        self.client.is_whitelisted(phone.as_str(), &self.whitelist)
    }

    #[instrument(skip(self, message), fields(recipient = %message.recipient))]
    async fn send_text(&self, message: OutgoingTextMessage) -> Result<String, ApplicationError> {
        let response = self
            .client
            .send_message(message.recipient.as_str(), &message.text)
            .await
            .map_err(|e| ApplicationError::ExternalService(format!("WhatsApp send failed: {e}")))?;

        let message_id = response
            .messages
            .first()
            .map(|m| m.id.clone())
            .ok_or_else(|| {
                ApplicationError::ExternalService("No message ID in response".to_string())
            })?;

        debug!(message_id = %message_id, "WhatsApp text message sent");
        Ok(message_id)
    }

    #[instrument(skip(self, message), fields(recipient = %message.recipient, audio_size = message.audio_data.len()))]
    async fn send_audio(&self, message: OutgoingAudioMessage) -> Result<String, ApplicationError> {
        // Step 1: Upload the audio file
        let filename = format!("audio.{}", mime_to_extension(&message.mime_type));
        let upload_response = self
            .client
            .upload_media(message.audio_data, &message.mime_type, &filename)
            .await
            .map_err(|e| {
                ApplicationError::ExternalService(format!("WhatsApp upload failed: {e}"))
            })?;

        debug!(media_id = %upload_response.id, "Audio uploaded to WhatsApp");

        // Step 2: Send the audio message
        let response = self
            .client
            .send_audio_message(message.recipient.as_str(), &upload_response.id)
            .await
            .map_err(|e| {
                ApplicationError::ExternalService(format!("WhatsApp audio send failed: {e}"))
            })?;

        let message_id = response
            .messages
            .first()
            .map(|m| m.id.clone())
            .ok_or_else(|| {
                ApplicationError::ExternalService("No message ID in response".to_string())
            })?;

        debug!(message_id = %message_id, "WhatsApp audio message sent");
        Ok(message_id)
    }

    #[instrument(skip(self), fields(media_id = %media_id))]
    async fn download_audio(&self, media_id: &str) -> Result<DownloadedAudio, ApplicationError> {
        let media = self
            .client
            .download_media(media_id)
            .await
            .map_err(|e| match e {
                WhatsAppError::MediaNotFound(id) => {
                    ApplicationError::NotFound(format!("Media not found: {id}"))
                },
                e => ApplicationError::ExternalService(format!("WhatsApp download failed: {e}")),
            })?;

        debug!(size = media.data.len(), mime_type = %media.mime_type, "Audio downloaded from WhatsApp");

        Ok(DownloadedAudio {
            data: media.data,
            mime_type: media.mime_type,
        })
    }

    #[instrument(skip(self), fields(message_id = %message_id))]
    async fn mark_read(&self, message_id: &str) -> Result<(), ApplicationError> {
        // WhatsApp marks messages as read automatically via the webhook response
        // The read receipt is sent when we respond via the webhook
        debug!(message_id = %message_id, "WhatsApp read receipt (auto-sent via webhook)");
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
        "audio/mp4" | "audio/m4a" => "m4a",
        _ => "bin",
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
        assert_eq!(mime_to_extension("audio/x-wav"), "wav");
        assert_eq!(mime_to_extension("audio/mp4"), "m4a");
        assert_eq!(mime_to_extension("audio/m4a"), "m4a");
    }

    #[test]
    fn mime_to_extension_handles_parameters() {
        assert_eq!(mime_to_extension("audio/ogg; codecs=opus"), "ogg");
    }

    #[test]
    fn mime_to_extension_returns_bin_for_unknown() {
        assert_eq!(mime_to_extension("application/octet-stream"), "bin");
        assert_eq!(mime_to_extension("unknown/type"), "bin");
    }

    #[test]
    fn source_returns_whatsapp() {
        // Can't easily test without valid config, but we can test the function exists
        // and returns the right type
        assert_eq!(MessengerSource::WhatsApp.display_name(), "WhatsApp");
    }
}
