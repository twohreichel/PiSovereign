//! WhatsApp client for sending messages
//!
//! Uses the Meta Graph API to send WhatsApp messages, including text and audio.

use reqwest::Client;
use reqwest::multipart::{Form, Part};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tracing::{debug, instrument, warn};

/// WhatsApp API errors
#[derive(Debug, Error)]
pub enum WhatsAppError {
    #[error("HTTP request failed: {0}")]
    Request(#[from] reqwest::Error),

    #[error("API error: {code} - {message}")]
    Api { code: i32, message: String },

    #[error("Missing configuration: {0}")]
    Configuration(String),

    #[error("Invalid phone number: {0}")]
    InvalidPhoneNumber(String),

    #[error("Invalid signature")]
    InvalidSignature,

    #[error("Sender not whitelisted: {0}")]
    NotWhitelisted(String),

    #[error("Media not found: {0}")]
    MediaNotFound(String),

    #[error("Media download failed: {0}")]
    MediaDownloadFailed(String),

    #[error("Media upload failed: {0}")]
    MediaUploadFailed(String),
}

/// WhatsApp client configuration
#[derive(Debug, Clone)]
pub struct WhatsAppClientConfig {
    /// Meta Graph API access token
    pub access_token: String,
    /// Phone number ID from WhatsApp Business
    pub phone_number_id: String,
    /// App secret for webhook signature verification
    pub app_secret: String,
    /// Verify token for webhook setup
    pub verify_token: String,
    /// Whether signature verification is required
    pub signature_required: bool,
    /// API version (default: v18.0)
    pub api_version: String,
}

impl Default for WhatsAppClientConfig {
    fn default() -> Self {
        Self {
            access_token: String::new(),
            phone_number_id: String::new(),
            app_secret: String::new(),
            verify_token: String::new(),
            signature_required: true,
            api_version: "v18.0".to_string(),
        }
    }
}

/// WhatsApp client for the Meta Graph API
#[derive(Debug, Clone)]
pub struct WhatsAppClient {
    client: Client,
    config: WhatsAppClientConfig,
    base_url: String,
}

/// Message send request
#[derive(Debug, Serialize)]
struct SendMessageRequest {
    messaging_product: &'static str,
    to: String,
    #[serde(rename = "type")]
    msg_type: &'static str,
    text: TextContent,
}

#[derive(Debug, Serialize)]
struct TextContent {
    body: String,
}

/// API response for sent message
#[derive(Debug, Deserialize)]
pub struct SendMessageResponse {
    pub messaging_product: String,
    pub contacts: Vec<ContactInfo>,
    pub messages: Vec<MessageInfo>,
}

#[derive(Debug, Deserialize)]
pub struct ContactInfo {
    pub input: String,
    pub wa_id: String,
}

#[derive(Debug, Deserialize)]
pub struct MessageInfo {
    pub id: String,
}

/// API error response
#[derive(Debug, Deserialize)]
struct ApiErrorResponse {
    error: ApiErrorDetail,
}

#[derive(Debug, Deserialize)]
struct ApiErrorDetail {
    code: i32,
    message: String,
}

/// Media URL response from Meta Graph API
#[derive(Debug, Deserialize)]
struct MediaUrlResponse {
    url: String,
    #[serde(default)]
    mime_type: Option<String>,
    #[serde(default)]
    file_size: Option<u64>,
}

/// Media upload response
#[derive(Debug, Deserialize)]
pub struct MediaUploadResponse {
    /// The uploaded media ID
    pub id: String,
}

/// Downloaded media with metadata
#[derive(Debug)]
pub struct DownloadedMedia {
    /// Raw audio bytes
    pub data: Vec<u8>,
    /// MIME type (e.g., "audio/ogg")
    pub mime_type: String,
}

/// Audio message send request
#[derive(Debug, Serialize)]
struct SendAudioMessageRequest {
    messaging_product: &'static str,
    to: String,
    #[serde(rename = "type")]
    msg_type: &'static str,
    audio: AudioContent,
}

#[derive(Debug, Serialize)]
struct AudioContent {
    id: String,
}

impl WhatsAppClient {
    /// Create a new WhatsApp client
    pub fn new(config: WhatsAppClientConfig) -> Result<Self, WhatsAppError> {
        if config.access_token.is_empty() {
            return Err(WhatsAppError::Configuration(
                "access_token is required".to_string(),
            ));
        }
        if config.phone_number_id.is_empty() {
            return Err(WhatsAppError::Configuration(
                "phone_number_id is required".to_string(),
            ));
        }

        let base_url = format!(
            "https://graph.facebook.com/{}/{}",
            config.api_version, config.phone_number_id
        );

        Ok(Self {
            client: Client::new(),
            config,
            base_url,
        })
    }

    /// Send a text message
    #[instrument(skip(self, message), fields(to = %to))]
    pub async fn send_message(
        &self,
        to: &str,
        message: &str,
    ) -> Result<SendMessageResponse, WhatsAppError> {
        // Validate phone number format (basic check)
        if !to.starts_with('+') || to.len() < 10 {
            return Err(WhatsAppError::InvalidPhoneNumber(to.to_string()));
        }

        // Remove + prefix for API
        let phone = to.trim_start_matches('+');

        let request = SendMessageRequest {
            messaging_product: "whatsapp",
            to: phone.to_string(),
            msg_type: "text",
            text: TextContent {
                body: message.to_string(),
            },
        };

        debug!(phone = %phone, message_len = message.len(), "Sending WhatsApp message");

        let response = self
            .client
            .post(format!("{}/messages", self.base_url))
            .bearer_auth(&self.config.access_token)
            .json(&request)
            .send()
            .await?;

        if response.status().is_success() {
            Ok(response.json().await?)
        } else {
            let error: ApiErrorResponse = response.json().await?;
            Err(WhatsAppError::Api {
                code: error.error.code,
                message: error.error.message,
            })
        }
    }

    /// Check if a phone number is whitelisted
    pub fn is_whitelisted(&self, phone: &str, whitelist: &[String]) -> bool {
        if whitelist.is_empty() {
            // Empty whitelist means all numbers are allowed
            return true;
        }
        whitelist.iter().any(|w| phone == w || phone.ends_with(w))
    }

    /// Verify webhook signature (wrapper around webhook::verify_signature)
    pub fn verify_signature(&self, payload: &[u8], signature: &str) -> Result<(), WhatsAppError> {
        if !self.config.signature_required {
            return Ok(());
        }

        if crate::webhook::verify_signature(payload, signature, &self.config.app_secret) {
            Ok(())
        } else {
            Err(WhatsAppError::InvalidSignature)
        }
    }

    /// Get the verify token for webhook setup
    #[must_use]
    pub fn verify_token(&self) -> &str {
        &self.config.verify_token
    }

    /// Check if the WhatsApp API is reachable
    ///
    /// Performs a lightweight check to verify the configuration is valid
    /// and the API is accessible.
    #[instrument(skip(self))]
    pub async fn is_available(&self) -> bool {
        // Try to get business profile as a health check
        // This is a read-only operation that doesn't send messages
        self.client
            .get(format!("{}/whatsapp_business_profile", self.base_url))
            .bearer_auth(&self.config.access_token)
            .query(&[("fields", "about,address,description,vertical")])
            .send()
            .await
            .is_ok_and(|res| res.status().is_success())
    }

    /// Download media from WhatsApp using the media ID
    ///
    /// WhatsApp media download is a two-step process:
    /// 1. Get the media URL using the media ID
    /// 2. Download the actual media from that URL
    #[instrument(skip(self), fields(media_id = %media_id))]
    pub async fn download_media(&self, media_id: &str) -> Result<DownloadedMedia, WhatsAppError> {
        // Step 1: Get the media URL
        let media_url_endpoint = format!(
            "https://graph.facebook.com/{}/{}",
            self.config.api_version, media_id
        );

        debug!("Fetching media URL for ID: {}", media_id);

        let url_response = self
            .client
            .get(&media_url_endpoint)
            .bearer_auth(&self.config.access_token)
            .send()
            .await?;

        if !url_response.status().is_success() {
            let status = url_response.status();
            let error_body = url_response.text().await.unwrap_or_default();
            warn!("Failed to get media URL: {} - {}", status, error_body);

            if let Ok(api_error) = serde_json::from_str::<ApiErrorResponse>(&error_body) {
                if api_error.error.code == 100 {
                    return Err(WhatsAppError::MediaNotFound(media_id.to_string()));
                }
                return Err(WhatsAppError::Api {
                    code: api_error.error.code,
                    message: api_error.error.message,
                });
            }

            return Err(WhatsAppError::MediaDownloadFailed(format!(
                "HTTP {status}: {error_body}"
            )));
        }

        let media_info: MediaUrlResponse = url_response.json().await.map_err(|e| {
            WhatsAppError::MediaDownloadFailed(format!("Failed to parse media URL response: {e}"))
        })?;

        debug!(
            url = %media_info.url,
            mime_type = ?media_info.mime_type,
            file_size = ?media_info.file_size,
            "Got media URL"
        );

        // Step 2: Download the actual media
        let media_response = self
            .client
            .get(&media_info.url)
            .bearer_auth(&self.config.access_token)
            .send()
            .await?;

        if !media_response.status().is_success() {
            let status = media_response.status();
            return Err(WhatsAppError::MediaDownloadFailed(format!(
                "Media download failed with HTTP {status}"
            )));
        }

        // Get content-type from response if available
        let content_type = media_response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .map(String::from)
            .or(media_info.mime_type)
            .unwrap_or_else(|| "application/octet-stream".to_string());

        let data = media_response.bytes().await?.to_vec();

        debug!(
            data_size = data.len(),
            mime_type = %content_type,
            "Media downloaded successfully"
        );

        Ok(DownloadedMedia {
            data,
            mime_type: content_type,
        })
    }

    /// Upload media to WhatsApp for sending
    ///
    /// Returns a media ID that can be used to send the media in a message.
    #[instrument(skip(self, data), fields(data_size = data.len(), mime_type = %mime_type))]
    pub async fn upload_media(
        &self,
        data: Vec<u8>,
        mime_type: &str,
        filename: &str,
    ) -> Result<MediaUploadResponse, WhatsAppError> {
        let upload_url = format!("{}/media", self.base_url);

        debug!("Uploading media: {} bytes, type: {}", data.len(), mime_type);

        let file_part = Part::bytes(data)
            .file_name(filename.to_string())
            .mime_str(mime_type)
            .map_err(|e| WhatsAppError::MediaUploadFailed(format!("Invalid MIME type: {e}")))?;

        let form = Form::new()
            .text("messaging_product", "whatsapp")
            .part("file", file_part);

        let response = self
            .client
            .post(&upload_url)
            .bearer_auth(&self.config.access_token)
            .multipart(form)
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let error_body = response.text().await.unwrap_or_default();

            if let Ok(api_error) = serde_json::from_str::<ApiErrorResponse>(&error_body) {
                return Err(WhatsAppError::Api {
                    code: api_error.error.code,
                    message: api_error.error.message,
                });
            }

            return Err(WhatsAppError::MediaUploadFailed(format!(
                "HTTP {status}: {error_body}"
            )));
        }

        let upload_response: MediaUploadResponse = response.json().await.map_err(|e| {
            WhatsAppError::MediaUploadFailed(format!("Failed to parse upload response: {e}"))
        })?;

        debug!(media_id = %upload_response.id, "Media uploaded successfully");

        Ok(upload_response)
    }

    /// Send an audio message using a previously uploaded media ID
    #[instrument(skip(self), fields(to = %to, media_id = %media_id))]
    pub async fn send_audio_message(
        &self,
        to: &str,
        media_id: &str,
    ) -> Result<SendMessageResponse, WhatsAppError> {
        // Validate phone number format
        if !to.starts_with('+') || to.len() < 10 {
            return Err(WhatsAppError::InvalidPhoneNumber(to.to_string()));
        }

        let phone = to.trim_start_matches('+');

        let request = SendAudioMessageRequest {
            messaging_product: "whatsapp",
            to: phone.to_string(),
            msg_type: "audio",
            audio: AudioContent {
                id: media_id.to_string(),
            },
        };

        debug!(phone = %phone, media_id = %media_id, "Sending audio message");

        let response = self
            .client
            .post(format!("{}/messages", self.base_url))
            .bearer_auth(&self.config.access_token)
            .json(&request)
            .send()
            .await?;

        if response.status().is_success() {
            Ok(response.json().await?)
        } else {
            let error: ApiErrorResponse = response.json().await?;
            Err(WhatsAppError::Api {
                code: error.error.code,
                message: error.error.message,
            })
        }
    }

    /// Mark a message as read
    ///
    /// This shows the sender that the message has been read (blue checkmarks).
    #[instrument(skip(self), fields(message_id = %message_id))]
    pub async fn mark_as_read(&self, message_id: &str) -> Result<(), WhatsAppError> {
        #[derive(Serialize)]
        struct MarkReadRequest {
            messaging_product: &'static str,
            status: &'static str,
            message_id: String,
        }

        let request = MarkReadRequest {
            messaging_product: "whatsapp",
            status: "read",
            message_id: message_id.to_string(),
        };

        let response = self
            .client
            .post(format!("{}/messages", self.base_url))
            .bearer_auth(&self.config.access_token)
            .json(&request)
            .send()
            .await?;

        if response.status().is_success() {
            debug!("Marked message {} as read", message_id);
            Ok(())
        } else {
            let error: ApiErrorResponse = response.json().await?;
            Err(WhatsAppError::Api {
                code: error.error.code,
                message: error.error.message,
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_config() -> WhatsAppClientConfig {
        WhatsAppClientConfig {
            access_token: "test_token".to_string(),
            phone_number_id: "123456789".to_string(),
            app_secret: "test_secret".to_string(),
            verify_token: "verify_test".to_string(),
            signature_required: true,
            api_version: "v18.0".to_string(),
        }
    }

    mod client_creation_tests {
        use super::*;

        #[test]
        fn client_creation_requires_access_token() {
            let config = WhatsAppClientConfig {
                access_token: String::new(),
                phone_number_id: "123".to_string(),
                ..Default::default()
            };

            let result = WhatsAppClient::new(config);
            assert!(matches!(result, Err(WhatsAppError::Configuration(_))));
        }

        #[test]
        fn client_creation_requires_phone_number_id() {
            let config = WhatsAppClientConfig {
                access_token: "token".to_string(),
                phone_number_id: String::new(),
                ..Default::default()
            };

            let result = WhatsAppClient::new(config);
            assert!(matches!(result, Err(WhatsAppError::Configuration(_))));
        }

        #[test]
        fn client_creation_succeeds_with_valid_config() {
            let client = WhatsAppClient::new(test_config());
            assert!(client.is_ok());
        }

        #[test]
        fn config_default_values() {
            let config = WhatsAppClientConfig::default();
            assert!(config.signature_required);
            assert_eq!(config.api_version, "v18.0");
        }
    }

    mod whitelist_tests {
        use super::*;

        #[test]
        fn whitelist_empty_allows_all() {
            let client = WhatsAppClient::new(test_config()).unwrap();
            assert!(client.is_whitelisted("+491234567890", &[]));
        }

        #[test]
        fn whitelist_exact_match() {
            let client = WhatsAppClient::new(test_config()).unwrap();
            let whitelist = vec!["+491234567890".to_string()];
            assert!(client.is_whitelisted("+491234567890", &whitelist));
            assert!(!client.is_whitelisted("+499999999999", &whitelist));
        }

        #[test]
        fn whitelist_suffix_match() {
            let client = WhatsAppClient::new(test_config()).unwrap();
            let whitelist = vec!["1234567890".to_string()];
            assert!(client.is_whitelisted("+491234567890", &whitelist));
        }
    }

    mod signature_tests {
        use super::*;

        #[test]
        fn verify_token_getter() {
            let client = WhatsAppClient::new(test_config()).unwrap();
            assert_eq!(client.verify_token(), "verify_test");
        }

        #[test]
        fn signature_verification_skipped_when_disabled() {
            let mut config = test_config();
            config.signature_required = false;
            let client = WhatsAppClient::new(config).unwrap();

            assert!(client.verify_signature(b"test", "invalid").is_ok());
        }

        #[test]
        fn signature_verification_fails_when_required() {
            let client = WhatsAppClient::new(test_config()).unwrap();

            let result = client.verify_signature(b"test", "invalid");
            assert!(matches!(result, Err(WhatsAppError::InvalidSignature)));
        }
    }

    mod error_tests {
        use super::*;

        #[test]
        fn error_display_configuration() {
            let err = WhatsAppError::Configuration("test".to_string());
            assert!(err.to_string().contains("test"));
        }

        #[test]
        fn error_display_api() {
            let err = WhatsAppError::Api {
                code: 100,
                message: "Invalid".to_string(),
            };
            assert!(err.to_string().contains("100"));
            assert!(err.to_string().contains("Invalid"));
        }

        #[test]
        fn error_display_media_not_found() {
            let err = WhatsAppError::MediaNotFound("media-123".to_string());
            assert!(err.to_string().contains("media-123"));
        }

        #[test]
        fn error_display_media_download_failed() {
            let err = WhatsAppError::MediaDownloadFailed("timeout".to_string());
            assert!(err.to_string().contains("timeout"));
        }

        #[test]
        fn error_display_media_upload_failed() {
            let err = WhatsAppError::MediaUploadFailed("too large".to_string());
            assert!(err.to_string().contains("too large"));
        }
    }

    mod message_validation_tests {
        use super::*;

        #[tokio::test]
        async fn send_message_validates_phone_format() {
            let client = WhatsAppClient::new(test_config()).unwrap();

            // Missing + prefix
            let result = client.send_message("491234567890", "test").await;
            assert!(matches!(result, Err(WhatsAppError::InvalidPhoneNumber(_))));

            // Too short
            let result = client.send_message("+123", "test").await;
            assert!(matches!(result, Err(WhatsAppError::InvalidPhoneNumber(_))));
        }

        #[tokio::test]
        async fn send_audio_message_validates_phone_format() {
            let client = WhatsAppClient::new(test_config()).unwrap();

            // Missing + prefix
            let result = client.send_audio_message("491234567890", "media-123").await;
            assert!(matches!(result, Err(WhatsAppError::InvalidPhoneNumber(_))));

            // Too short
            let result = client.send_audio_message("+123", "media-123").await;
            assert!(matches!(result, Err(WhatsAppError::InvalidPhoneNumber(_))));
        }
    }

    mod downloaded_media_tests {
        use super::*;

        #[test]
        fn downloaded_media_has_debug() {
            let media = DownloadedMedia {
                data: vec![1, 2, 3],
                mime_type: "audio/ogg".to_string(),
            };
            let debug = format!("{media:?}");
            assert!(debug.contains("DownloadedMedia"));
            assert!(debug.contains("audio/ogg"));
        }
    }

    mod media_upload_response_tests {
        use super::*;

        #[test]
        fn deserializes_upload_response() {
            let json = r#"{"id": "media-123456"}"#;
            let response: MediaUploadResponse = serde_json::from_str(json).unwrap();
            assert_eq!(response.id, "media-123456");
        }
    }
}
