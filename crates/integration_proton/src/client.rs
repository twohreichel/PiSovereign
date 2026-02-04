//! Proton Mail client implementation
//!
//! Connects to Proton Bridge's local IMAP/SMTP server.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tracing::{debug, instrument, warn};

/// Proton integration errors
#[derive(Debug, Error)]
pub enum ProtonError {
    #[error("Bridge not available: {0}")]
    BridgeUnavailable(String),

    #[error("Authentication failed")]
    AuthenticationFailed,

    #[error("Mailbox not found: {0}")]
    MailboxNotFound(String),

    #[error("Message not found: {0}")]
    MessageNotFound(String),

    #[error("Connection failed: {0}")]
    ConnectionFailed(String),

    #[error("Request failed: {0}")]
    RequestFailed(String),

    #[error("SMTP error: {0}")]
    SmtpError(String),
}

/// Proton Bridge configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProtonConfig {
    /// IMAP host (default: 127.0.0.1)
    pub imap_host: String,
    /// IMAP port (default: 1143)
    pub imap_port: u16,
    /// SMTP host (default: 127.0.0.1)
    pub smtp_host: String,
    /// SMTP port (default: 1025)
    pub smtp_port: u16,
    /// Email address (Bridge account email)
    pub email: String,
    /// Bridge password (from Bridge UI, not Proton password)
    pub password: String,
}

impl Default for ProtonConfig {
    fn default() -> Self {
        Self {
            imap_host: "127.0.0.1".to_string(),
            imap_port: 1143,
            smtp_host: "127.0.0.1".to_string(),
            smtp_port: 1025,
            email: String::new(),
            password: String::new(),
        }
    }
}

/// Email summary for display
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EmailSummary {
    /// Message ID
    pub id: String,
    /// Sender email address
    pub from: String,
    /// Email subject
    pub subject: String,
    /// Preview snippet (first 100 chars)
    pub snippet: String,
    /// Received timestamp (ISO 8601)
    pub received_at: String,
    /// Whether the email has been read
    pub is_read: bool,
    /// Whether the email is flagged as important
    pub is_important: bool,
}

/// Email composition for sending
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmailComposition {
    /// Recipient email address
    pub to: String,
    /// Carbon copy recipients
    pub cc: Vec<String>,
    /// Email subject
    pub subject: String,
    /// Email body (plain text)
    pub body: String,
}

/// Proton Mail client trait
#[async_trait]
pub trait ProtonClient: Send + Sync {
    /// Get recent emails from inbox
    async fn get_inbox(&self, count: u32) -> Result<Vec<EmailSummary>, ProtonError>;

    /// Get emails from a specific mailbox
    async fn get_mailbox(
        &self,
        mailbox: &str,
        count: u32,
    ) -> Result<Vec<EmailSummary>, ProtonError>;

    /// Get unread count
    async fn get_unread_count(&self) -> Result<u32, ProtonError>;

    /// Mark email as read
    async fn mark_read(&self, email_id: &str) -> Result<(), ProtonError>;

    /// Mark email as unread
    async fn mark_unread(&self, email_id: &str) -> Result<(), ProtonError>;

    /// Delete an email (move to trash)
    async fn delete(&self, email_id: &str) -> Result<(), ProtonError>;

    /// Send an email
    async fn send_email(&self, email: &EmailComposition) -> Result<String, ProtonError>;

    /// Check if Bridge is available
    async fn check_connection(&self) -> Result<bool, ProtonError>;
}

/// Proton Bridge client implementation
///
/// Connects to Proton Bridge's local IMAP/SMTP server.
/// Note: Proton Bridge must be running and configured.
#[derive(Debug)]
pub struct ProtonBridgeClient {
    config: ProtonConfig,
}

impl ProtonBridgeClient {
    /// Create a new Proton Bridge client
    pub fn new(config: ProtonConfig) -> Self {
        Self { config }
    }

    /// Get IMAP connection string
    fn imap_addr(&self) -> String {
        format!("{}:{}", self.config.imap_host, self.config.imap_port)
    }

    /// Get SMTP connection string
    fn smtp_addr(&self) -> String {
        format!("{}:{}", self.config.smtp_host, self.config.smtp_port)
    }
}

#[async_trait]
impl ProtonClient for ProtonBridgeClient {
    #[instrument(skip(self))]
    async fn get_inbox(&self, count: u32) -> Result<Vec<EmailSummary>, ProtonError> {
        self.get_mailbox("INBOX", count).await
    }

    #[instrument(skip(self))]
    async fn get_mailbox(
        &self,
        mailbox: &str,
        count: u32,
    ) -> Result<Vec<EmailSummary>, ProtonError> {
        // Note: This is a placeholder implementation
        // Full IMAP implementation would require async-imap or similar crate
        debug!(
            mailbox = %mailbox,
            count = %count,
            addr = %self.imap_addr(),
            "Fetching emails from mailbox"
        );

        // Return empty for now - actual implementation needs IMAP library
        warn!("IMAP implementation pending - returning empty mailbox");
        Ok(Vec::new())
    }

    #[instrument(skip(self))]
    async fn get_unread_count(&self) -> Result<u32, ProtonError> {
        debug!(addr = %self.imap_addr(), "Getting unread count");

        // Placeholder - needs IMAP STATUS command
        warn!("IMAP implementation pending - returning 0 unread");
        Ok(0)
    }

    #[instrument(skip(self))]
    async fn mark_read(&self, email_id: &str) -> Result<(), ProtonError> {
        debug!(email_id = %email_id, "Marking email as read");

        // Placeholder - needs IMAP STORE +FLAGS \Seen
        warn!("IMAP implementation pending - mark_read not implemented");
        Ok(())
    }

    #[instrument(skip(self))]
    async fn mark_unread(&self, email_id: &str) -> Result<(), ProtonError> {
        debug!(email_id = %email_id, "Marking email as unread");

        // Placeholder - needs IMAP STORE -FLAGS \Seen
        warn!("IMAP implementation pending - mark_unread not implemented");
        Ok(())
    }

    #[instrument(skip(self))]
    async fn delete(&self, email_id: &str) -> Result<(), ProtonError> {
        debug!(email_id = %email_id, "Deleting email");

        // Placeholder - needs IMAP COPY to Trash + EXPUNGE
        warn!("IMAP implementation pending - delete not implemented");
        Ok(())
    }

    #[instrument(skip(self, email))]
    async fn send_email(&self, email: &EmailComposition) -> Result<String, ProtonError> {
        debug!(
            to = %email.to,
            subject = %email.subject,
            addr = %self.smtp_addr(),
            "Sending email"
        );

        // Placeholder - needs SMTP library (lettre or similar)
        warn!("SMTP implementation pending - send_email not implemented");

        // Generate a fake message ID for now
        let message_id = format!(
            "<{}.{}@pisovereign.local>",
            chrono::Utc::now().timestamp(),
            uuid::Uuid::new_v4()
        );
        Ok(message_id)
    }

    #[instrument(skip(self))]
    async fn check_connection(&self) -> Result<bool, ProtonError> {
        // Try TCP connection to IMAP port
        let addr = self.imap_addr();
        debug!(addr = %addr, "Checking Bridge connection");

        match tokio::net::TcpStream::connect(&addr).await {
            Ok(_) => {
                debug!("Bridge connection successful");
                Ok(true)
            },
            Err(e) => {
                debug!(error = %e, "Bridge connection failed");
                Ok(false)
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn proton_error_bridge_unavailable() {
        let err = ProtonError::BridgeUnavailable("not running".to_string());
        assert_eq!(err.to_string(), "Bridge not available: not running");
    }

    #[test]
    fn proton_error_authentication_failed() {
        let err = ProtonError::AuthenticationFailed;
        assert_eq!(err.to_string(), "Authentication failed");
    }

    #[test]
    fn proton_error_mailbox_not_found() {
        let err = ProtonError::MailboxNotFound("Archive".to_string());
        assert_eq!(err.to_string(), "Mailbox not found: Archive");
    }

    #[test]
    fn proton_error_message_not_found() {
        let err = ProtonError::MessageNotFound("12345".to_string());
        assert_eq!(err.to_string(), "Message not found: 12345");
    }

    #[test]
    fn proton_error_connection_failed() {
        let err = ProtonError::ConnectionFailed("timeout".to_string());
        assert_eq!(err.to_string(), "Connection failed: timeout");
    }

    #[test]
    fn proton_error_request_failed() {
        let err = ProtonError::RequestFailed("network error".to_string());
        assert_eq!(err.to_string(), "Request failed: network error");
    }

    #[test]
    fn proton_error_smtp_error() {
        let err = ProtonError::SmtpError("relay denied".to_string());
        assert_eq!(err.to_string(), "SMTP error: relay denied");
    }

    #[test]
    fn proton_error_has_debug() {
        let err = ProtonError::AuthenticationFailed;
        let debug = format!("{err:?}");
        assert!(debug.contains("AuthenticationFailed"));
    }

    #[test]
    fn proton_config_default() {
        let config = ProtonConfig::default();
        assert_eq!(config.imap_host, "127.0.0.1");
        assert_eq!(config.imap_port, 1143);
        assert_eq!(config.smtp_host, "127.0.0.1");
        assert_eq!(config.smtp_port, 1025);
        assert!(config.email.is_empty());
        assert!(config.password.is_empty());
    }

    #[test]
    fn proton_config_creation() {
        let config = ProtonConfig {
            imap_host: "localhost".to_string(),
            imap_port: 993,
            smtp_host: "localhost".to_string(),
            smtp_port: 587,
            email: "user@proton.me".to_string(),
            password: "bridge-password".to_string(),
        };
        assert_eq!(config.email, "user@proton.me");
    }

    #[test]
    fn proton_config_serialization() {
        let config = ProtonConfig::default();
        let json = serde_json::to_string(&config).unwrap();
        assert!(json.contains("imap_host"));
        assert!(json.contains("smtp_port"));
    }

    #[test]
    fn proton_config_deserialization() {
        let json = r#"{"imap_host":"127.0.0.1","imap_port":1143,"smtp_host":"127.0.0.1","smtp_port":1025,"email":"test@proton.me","password":"secret"}"#;
        let config: ProtonConfig = serde_json::from_str(json).unwrap();
        assert_eq!(config.email, "test@proton.me");
    }

    #[test]
    fn proton_config_clone() {
        let config = ProtonConfig {
            imap_host: "host".to_string(),
            imap_port: 993,
            smtp_host: "host".to_string(),
            smtp_port: 587,
            email: "a@b.com".to_string(),
            password: "pass".to_string(),
        };
        #[allow(clippy::redundant_clone)]
        let cloned = config.clone();
        assert_eq!(config.email, cloned.email);
        assert_eq!(config.imap_port, cloned.imap_port);
    }

    #[test]
    fn proton_config_has_debug() {
        let config = ProtonConfig::default();
        let debug = format!("{config:?}");
        assert!(debug.contains("ProtonConfig"));
    }

    #[test]
    fn email_summary_creation() {
        let email = EmailSummary {
            id: "mail123".to_string(),
            from: "sender@example.com".to_string(),
            subject: "Hello".to_string(),
            snippet: "This is a test...".to_string(),
            received_at: "2025-02-01T10:00:00Z".to_string(),
            is_read: false,
            is_important: true,
        };
        assert_eq!(email.id, "mail123");
        assert!(!email.is_read);
        assert!(email.is_important);
    }

    #[test]
    fn email_summary_serialization() {
        let email = EmailSummary {
            id: "1".to_string(),
            from: "a@b.com".to_string(),
            subject: "Test".to_string(),
            snippet: "...".to_string(),
            received_at: "2025-01-01T00:00:00Z".to_string(),
            is_read: true,
            is_important: false,
        };
        let json = serde_json::to_string(&email).unwrap();
        assert!(json.contains("subject"));
        assert!(json.contains("is_read"));
    }

    #[test]
    fn email_summary_deserialization() {
        let json = r#"{"id":"1","from":"a@b.com","subject":"Hi","snippet":"...","received_at":"2025-01-01T00:00:00Z","is_read":false,"is_important":false}"#;
        let email: EmailSummary = serde_json::from_str(json).unwrap();
        assert_eq!(email.subject, "Hi");
        assert!(!email.is_read);
    }

    #[test]
    fn email_summary_has_debug() {
        let email = EmailSummary {
            id: "1".to_string(),
            from: "a@b.com".to_string(),
            subject: "Test".to_string(),
            snippet: "...".to_string(),
            received_at: "2025-01-01T00:00:00Z".to_string(),
            is_read: false,
            is_important: false,
        };
        let debug = format!("{email:?}");
        assert!(debug.contains("EmailSummary"));
        assert!(debug.contains("subject"));
    }

    #[test]
    fn email_summary_clone() {
        let email = EmailSummary {
            id: "1".to_string(),
            from: "a@b.com".to_string(),
            subject: "Test".to_string(),
            snippet: "...".to_string(),
            received_at: "2025-01-01T00:00:00Z".to_string(),
            is_read: false,
            is_important: true,
        };
        #[allow(clippy::redundant_clone)]
        let cloned = email.clone();
        assert_eq!(email.id, cloned.id);
        assert_eq!(email.is_important, cloned.is_important);
    }

    #[test]
    fn email_summary_equality() {
        let email1 = EmailSummary {
            id: "1".to_string(),
            from: "a@b.com".to_string(),
            subject: "Test".to_string(),
            snippet: "...".to_string(),
            received_at: "2025-01-01T00:00:00Z".to_string(),
            is_read: false,
            is_important: false,
        };
        #[allow(clippy::redundant_clone)]
        let email2 = email1.clone();
        assert_eq!(email1, email2);
    }

    #[test]
    fn email_composition_creation() {
        let email = EmailComposition {
            to: "recipient@example.com".to_string(),
            cc: vec!["cc@example.com".to_string()],
            subject: "Hello".to_string(),
            body: "Body text".to_string(),
        };
        assert_eq!(email.to, "recipient@example.com");
        assert_eq!(email.cc.len(), 1);
    }

    #[test]
    fn email_composition_serialization() {
        let email = EmailComposition {
            to: "a@b.com".to_string(),
            cc: vec![],
            subject: "Hi".to_string(),
            body: "Hello".to_string(),
        };
        let json = serde_json::to_string(&email).unwrap();
        assert!(json.contains("to"));
        assert!(json.contains("subject"));
    }

    #[test]
    fn email_composition_deserialization() {
        let json = r#"{"to":"a@b.com","cc":[],"subject":"Hi","body":"Hello"}"#;
        let email: EmailComposition = serde_json::from_str(json).unwrap();
        assert_eq!(email.to, "a@b.com");
        assert_eq!(email.body, "Hello");
    }

    #[test]
    fn email_composition_has_debug() {
        let email = EmailComposition {
            to: "a@b.com".to_string(),
            cc: vec![],
            subject: "Test".to_string(),
            body: "Body".to_string(),
        };
        let debug = format!("{email:?}");
        assert!(debug.contains("EmailComposition"));
    }

    #[test]
    fn email_composition_clone() {
        let email = EmailComposition {
            to: "a@b.com".to_string(),
            cc: vec!["c@d.com".to_string()],
            subject: "Test".to_string(),
            body: "Body".to_string(),
        };
        #[allow(clippy::redundant_clone)]
        let cloned = email.clone();
        assert_eq!(email.to, cloned.to);
        assert_eq!(email.cc, cloned.cc);
    }

    #[test]
    fn proton_bridge_client_creation() {
        let config = ProtonConfig::default();
        let client = ProtonBridgeClient::new(config);
        assert!(format!("{client:?}").contains("ProtonBridgeClient"));
    }

    #[test]
    fn proton_bridge_client_imap_addr() {
        let config = ProtonConfig {
            imap_host: "localhost".to_string(),
            imap_port: 993,
            ..Default::default()
        };
        let client = ProtonBridgeClient::new(config);
        assert_eq!(client.imap_addr(), "localhost:993");
    }

    #[test]
    fn proton_bridge_client_smtp_addr() {
        let config = ProtonConfig {
            smtp_host: "localhost".to_string(),
            smtp_port: 587,
            ..Default::default()
        };
        let client = ProtonBridgeClient::new(config);
        assert_eq!(client.smtp_addr(), "localhost:587");
    }

    #[tokio::test]
    async fn proton_bridge_client_get_inbox_returns_empty() {
        let config = ProtonConfig::default();
        let client = ProtonBridgeClient::new(config);
        let result = client.get_inbox(10).await;
        assert!(result.is_ok());
        assert!(result.unwrap().is_empty());
    }

    #[tokio::test]
    async fn proton_bridge_client_get_mailbox_returns_empty() {
        let config = ProtonConfig::default();
        let client = ProtonBridgeClient::new(config);
        let result = client.get_mailbox("Sent", 5).await;
        assert!(result.is_ok());
        assert!(result.unwrap().is_empty());
    }

    #[tokio::test]
    async fn proton_bridge_client_get_unread_count() {
        let config = ProtonConfig::default();
        let client = ProtonBridgeClient::new(config);
        let result = client.get_unread_count().await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 0);
    }

    #[tokio::test]
    async fn proton_bridge_client_mark_read() {
        let config = ProtonConfig::default();
        let client = ProtonBridgeClient::new(config);
        let result = client.mark_read("12345").await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn proton_bridge_client_mark_unread() {
        let config = ProtonConfig::default();
        let client = ProtonBridgeClient::new(config);
        let result = client.mark_unread("12345").await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn proton_bridge_client_delete() {
        let config = ProtonConfig::default();
        let client = ProtonBridgeClient::new(config);
        let result = client.delete("12345").await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn proton_bridge_client_send_email() {
        let config = ProtonConfig::default();
        let client = ProtonBridgeClient::new(config);

        let email = EmailComposition {
            to: "test@example.com".to_string(),
            cc: vec![],
            subject: "Test".to_string(),
            body: "Hello".to_string(),
        };

        let result = client.send_email(&email).await;
        assert!(result.is_ok());
        let message_id = result.unwrap();
        assert!(message_id.starts_with('<'));
        assert!(message_id.ends_with('>'));
        assert!(message_id.contains("@pisovereign.local"));
    }

    #[tokio::test]
    async fn proton_bridge_client_check_connection_fails_no_bridge() {
        let config = ProtonConfig {
            imap_port: 19999, // Non-existent port
            ..Default::default()
        };
        let client = ProtonBridgeClient::new(config);
        let result = client.check_connection().await;
        assert!(result.is_ok());
        assert!(!result.unwrap()); // Should return false since no server
    }
}
