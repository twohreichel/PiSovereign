//! WhatsApp client for sending messages
//!
//! Uses the Meta Graph API to send WhatsApp messages.

use reqwest::Client;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tracing::{debug, instrument};

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
            .map_or(false, |res| res.status().is_success())
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

        // Should pass even with invalid signature
        assert!(client.verify_signature(b"test", "invalid").is_ok());
    }

    #[test]
    fn signature_verification_fails_when_required() {
        let client = WhatsAppClient::new(test_config()).unwrap();

        let result = client.verify_signature(b"test", "invalid");
        assert!(matches!(result, Err(WhatsAppError::InvalidSignature)));
    }

    #[test]
    fn config_default_values() {
        let config = WhatsAppClientConfig::default();
        assert!(config.signature_required);
        assert_eq!(config.api_version, "v18.0");
    }

    #[test]
    fn error_display() {
        let err = WhatsAppError::Configuration("test".to_string());
        assert!(err.to_string().contains("test"));

        let err = WhatsAppError::Api {
            code: 100,
            message: "Invalid".to_string(),
        };
        assert!(err.to_string().contains("100"));
        assert!(err.to_string().contains("Invalid"));
    }

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
}
