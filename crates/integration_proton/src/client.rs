//! Proton Mail client implementation
//!
//! Connects to Proton Bridge's local IMAP/SMTP server for email operations.
//!
//! ## Architecture
//!
//! This module provides the main `ProtonBridgeClient` which implements the
//! `ProtonClient` trait. It internally uses:
//! - `ProtonImapClient` for reading and managing emails (IMAP)
//! - `ProtonSmtpClient` for sending emails (SMTP)
//!
//! ## Requirements
//!
//! - Proton Bridge must be running locally
//! - Default IMAP port: 1143 (STARTTLS)
//! - Default SMTP port: 1025 (STARTTLS)

use std::path::PathBuf;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tracing::{debug, instrument, warn};

use crate::{imap_client::ProtonImapClient, smtp_client::ProtonSmtpClient};

/// Proton integration errors
#[derive(Debug, Error)]
pub enum ProtonError {
    /// Bridge application is not running or not reachable
    #[error("Bridge not available: {0}")]
    BridgeUnavailable(String),

    /// Invalid credentials or authentication failure
    #[error("Authentication failed")]
    AuthenticationFailed,

    /// Requested mailbox does not exist
    #[error("Mailbox not found: {0}")]
    MailboxNotFound(String),

    /// Requested message does not exist
    #[error("Message not found: {0}")]
    MessageNotFound(String),

    /// Network connection error
    #[error("Connection failed: {0}")]
    ConnectionFailed(String),

    /// General request failure
    #[error("Request failed: {0}")]
    RequestFailed(String),

    /// SMTP-specific error
    #[error("SMTP error: {0}")]
    SmtpError(String),

    /// IMAP-specific error
    #[error("IMAP error: {0}")]
    ImapError(String),

    /// Invalid email address format
    #[error("Invalid email address: {0}")]
    InvalidAddress(String),
}

/// TLS configuration for Proton Bridge connections
///
/// Controls certificate verification behavior for IMAP and SMTP connections.
/// By default, certificate verification is enabled for security. Set to `false`
/// explicitly to disable verification for self-signed Proton Bridge certificates.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TlsConfig {
    /// Whether to verify TLS certificates
    ///
    /// - `None` (default): Verification enabled (secure default)
    /// - `Some(true)`: Verification explicitly enabled
    /// - `Some(false)`: Verification disabled (for self-signed certs)
    ///
    /// For Proton Bridge with self-signed certificates, set to `false`.
    /// Provide `ca_cert_path` for custom CA verification.
    #[serde(default)]
    pub verify_certificates: Option<bool>,

    /// Path to a custom CA certificate file (PEM format)
    ///
    /// If provided and verification is enabled, this certificate
    /// will be used as the root of trust for TLS connections.
    pub ca_cert_path: Option<PathBuf>,

    /// Minimum TLS version to accept (default: "1.2")
    pub min_tls_version: String,
}

impl TlsConfig {
    /// Check if TLS certificate verification is enabled
    ///
    /// Returns `true` if verification should be performed.
    /// `None` is interpreted as `true` (secure default).
    #[must_use]
    pub fn should_verify(&self) -> bool {
        self.verify_certificates.unwrap_or(true)
    }
}

impl Default for TlsConfig {
    fn default() -> Self {
        Self {
            // None means verification enabled (secure default)
            verify_certificates: None,
            ca_cert_path: None,
            min_tls_version: "1.2".to_string(),
        }
    }
}

impl TlsConfig {
    /// Create a TLS config that accepts self-signed certificates
    ///
    /// Use this for Proton Bridge which uses self-signed certificates.
    /// **Warning:** This disables certificate verification and should only
    /// be used for local Proton Bridge connections.
    ///
    /// # Security Warning
    ///
    /// Disabling TLS certificate verification makes the connection vulnerable
    /// to man-in-the-middle attacks. Only use this for trusted local connections.
    #[must_use]
    pub fn insecure() -> Self {
        warn!(
            "⚠️ TLS certificate verification disabled - use only for local Proton Bridge. \
             This configuration is NOT suitable for production use over untrusted networks."
        );
        Self {
            verify_certificates: Some(false),
            ca_cert_path: None,
            min_tls_version: "1.2".to_string(),
        }
    }

    /// Create a TLS config with strict certificate verification
    ///
    /// This is the secure default - certificates will be verified.
    pub fn strict() -> Self {
        Self {
            verify_certificates: Some(true),
            ca_cert_path: None,
            min_tls_version: "1.2".to_string(),
        }
    }

    /// Create a TLS config with a custom CA certificate
    ///
    /// Enables verification using the specified CA certificate.
    pub fn with_ca_cert(ca_cert_path: impl Into<PathBuf>) -> Self {
        Self {
            verify_certificates: Some(true),
            ca_cert_path: Some(ca_cert_path.into()),
            min_tls_version: "1.2".to_string(),
        }
    }
}

/// Proton Bridge configuration
///
/// Contains connection settings for both IMAP and SMTP services.
/// Default values are configured for standard Proton Bridge setup.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProtonConfig {
    /// IMAP server host (default: 127.0.0.1)
    pub imap_host: String,
    /// IMAP server port (default: 1143 for STARTTLS)
    pub imap_port: u16,
    /// SMTP server host (default: 127.0.0.1)
    pub smtp_host: String,
    /// SMTP server port (default: 1025 for STARTTLS)
    pub smtp_port: u16,
    /// Email address (Bridge account email)
    pub email: String,
    /// Bridge password (from Bridge UI, not Proton account password)
    #[serde(skip_serializing)]
    pub password: String,
    /// TLS configuration for secure connections
    #[serde(default)]
    pub tls: TlsConfig,
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
            tls: TlsConfig::default(),
        }
    }
}

impl ProtonConfig {
    /// Creates a new configuration with the specified credentials
    pub fn with_credentials(email: impl Into<String>, password: impl Into<String>) -> Self {
        Self {
            email: email.into(),
            password: password.into(),
            ..Default::default()
        }
    }

    /// Sets the IMAP connection details
    #[must_use]
    pub fn with_imap(mut self, host: impl Into<String>, port: u16) -> Self {
        self.imap_host = host.into();
        self.imap_port = port;
        self
    }

    /// Sets the SMTP connection details
    #[must_use]
    pub fn with_smtp(mut self, host: impl Into<String>, port: u16) -> Self {
        self.smtp_host = host.into();
        self.smtp_port = port;
        self
    }

    /// Sets the TLS configuration
    #[must_use]
    pub fn with_tls(mut self, tls: TlsConfig) -> Self {
        self.tls = tls;
        self
    }

    /// Validates the configuration
    pub fn validate(&self) -> Result<(), ProtonError> {
        if self.email.is_empty() {
            return Err(ProtonError::InvalidAddress(
                "Email address is required".to_string(),
            ));
        }
        if !self.email.contains('@') {
            return Err(ProtonError::InvalidAddress(format!(
                "Invalid email format: {}",
                self.email
            )));
        }
        if self.password.is_empty() {
            return Err(ProtonError::AuthenticationFailed);
        }
        Ok(())
    }
}

/// Email summary for display
///
/// Contains essential information about an email without the full body.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EmailSummary {
    /// Unique message identifier (UID)
    pub id: String,
    /// Sender email address
    pub from: String,
    /// Email subject line
    pub subject: String,
    /// Preview snippet (first ~200 characters of body)
    pub snippet: String,
    /// Received timestamp (RFC 2822 or ISO 8601 format)
    pub received_at: String,
    /// Whether the email has been read
    pub is_read: bool,
    /// Whether the email is flagged as important
    pub is_important: bool,
}

impl EmailSummary {
    /// Creates a new email summary
    pub fn new(id: impl Into<String>, from: impl Into<String>, subject: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            from: from.into(),
            subject: subject.into(),
            snippet: String::new(),
            received_at: chrono::Utc::now().to_rfc3339(),
            is_read: false,
            is_important: false,
        }
    }

    /// Sets the snippet
    #[must_use]
    pub fn with_snippet(mut self, snippet: impl Into<String>) -> Self {
        self.snippet = snippet.into();
        self
    }

    /// Sets the received timestamp
    #[must_use]
    pub fn with_received_at(mut self, received_at: impl Into<String>) -> Self {
        self.received_at = received_at.into();
        self
    }

    /// Sets the read status
    #[must_use]
    pub const fn with_read(mut self, is_read: bool) -> Self {
        self.is_read = is_read;
        self
    }

    /// Sets the important flag
    #[must_use]
    pub const fn with_important(mut self, is_important: bool) -> Self {
        self.is_important = is_important;
        self
    }
}

/// Email composition for sending
///
/// Contains all fields needed to compose and send an email.
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

impl EmailComposition {
    /// Creates a new email composition
    pub fn new(to: impl Into<String>, subject: impl Into<String>, body: impl Into<String>) -> Self {
        Self {
            to: to.into(),
            cc: Vec::new(),
            subject: subject.into(),
            body: body.into(),
        }
    }

    /// Adds a CC recipient
    #[must_use]
    pub fn with_cc(mut self, cc: impl Into<String>) -> Self {
        self.cc.push(cc.into());
        self
    }

    /// Adds multiple CC recipients
    #[must_use]
    pub fn with_cc_list(mut self, cc_list: Vec<String>) -> Self {
        self.cc.extend(cc_list);
        self
    }

    /// Validates the composition
    pub fn validate(&self) -> Result<(), ProtonError> {
        if self.to.is_empty() || !self.to.contains('@') {
            return Err(ProtonError::InvalidAddress(format!(
                "Invalid recipient: {}",
                self.to
            )));
        }
        for cc in &self.cc {
            if cc.is_empty() || !cc.contains('@') {
                return Err(ProtonError::InvalidAddress(format!("Invalid CC: {cc}")));
            }
        }
        if self.subject.is_empty() {
            return Err(ProtonError::RequestFailed(
                "Subject is required".to_string(),
            ));
        }
        Ok(())
    }
}

/// Proton Mail client trait
///
/// Defines the interface for interacting with Proton Mail via Bridge.
/// Implementations should handle both IMAP (read) and SMTP (send) operations.
#[async_trait]
pub trait ProtonClient: Send + Sync {
    /// Get recent emails from inbox
    ///
    /// # Arguments
    /// * `count` - Maximum number of emails to retrieve
    ///
    /// # Returns
    /// Vector of email summaries, newest first
    async fn get_inbox(&self, count: u32) -> Result<Vec<EmailSummary>, ProtonError>;

    /// Get emails from a specific mailbox
    ///
    /// # Arguments
    /// * `mailbox` - Mailbox name (e.g., "INBOX", "Sent", "Archive", "Trash")
    /// * `count` - Maximum number of emails to retrieve
    async fn get_mailbox(
        &self,
        mailbox: &str,
        count: u32,
    ) -> Result<Vec<EmailSummary>, ProtonError>;

    /// Get unread email count in inbox
    async fn get_unread_count(&self) -> Result<u32, ProtonError>;

    /// Mark an email as read
    ///
    /// # Arguments
    /// * `email_id` - Message UID to mark as read
    async fn mark_read(&self, email_id: &str) -> Result<(), ProtonError>;

    /// Mark an email as unread
    ///
    /// # Arguments
    /// * `email_id` - Message UID to mark as unread
    async fn mark_unread(&self, email_id: &str) -> Result<(), ProtonError>;

    /// Delete an email (move to trash)
    ///
    /// # Arguments
    /// * `email_id` - Message UID to delete
    async fn delete(&self, email_id: &str) -> Result<(), ProtonError>;

    /// Send an email
    ///
    /// # Arguments
    /// * `email` - Email composition with recipient, subject, and body
    ///
    /// # Returns
    /// Message ID of the sent email
    async fn send_email(&self, email: &EmailComposition) -> Result<String, ProtonError>;

    /// Check if Proton Bridge is available
    ///
    /// # Returns
    /// `true` if both IMAP and SMTP servers are reachable
    async fn check_connection(&self) -> Result<bool, ProtonError>;

    /// List available mailboxes
    async fn list_mailboxes(&self) -> Result<Vec<String>, ProtonError>;
}

/// Proton Bridge client implementation
///
/// Full implementation of the `ProtonClient` trait using IMAP for reading
/// emails and SMTP for sending. Connects to locally running Proton Bridge.
///
/// # Example
///
/// ```no_run
/// use integration_proton::{ProtonBridgeClient, ProtonClient, ProtonConfig};
///
/// #[tokio::main]
/// async fn main() {
///     let config = ProtonConfig::with_credentials("user@proton.me", "bridge-password");
///     let client = ProtonBridgeClient::new(config);
///
///     if client.check_connection().await.unwrap_or(false) {
///         let emails = client.get_inbox(10).await.unwrap();
///         println!("Found {} emails", emails.len());
///     }
/// }
/// ```
#[derive(Debug)]
pub struct ProtonBridgeClient {
    imap: ProtonImapClient,
    smtp: ProtonSmtpClient,
    config: ProtonConfig,
}

impl ProtonBridgeClient {
    /// Create a new Proton Bridge client
    pub fn new(config: ProtonConfig) -> Self {
        let imap = ProtonImapClient::new(config.clone());
        let smtp = ProtonSmtpClient::new(config.clone());
        Self { imap, smtp, config }
    }

    /// Get IMAP connection string (for logging)
    fn imap_addr(&self) -> String {
        format!("{}:{}", self.config.imap_host, self.config.imap_port)
    }

    /// Get SMTP connection string (for logging)
    fn smtp_addr(&self) -> String {
        format!("{}:{}", self.config.smtp_host, self.config.smtp_port)
    }

    /// Validates configuration before operations
    fn validate_config(&self) -> Result<(), ProtonError> {
        self.config.validate()
    }
}

#[async_trait]
impl ProtonClient for ProtonBridgeClient {
    #[instrument(skip(self))]
    async fn get_inbox(&self, count: u32) -> Result<Vec<EmailSummary>, ProtonError> {
        self.validate_config()?;
        self.imap.fetch_mailbox("INBOX", count).await
    }

    #[instrument(skip(self))]
    async fn get_mailbox(
        &self,
        mailbox: &str,
        count: u32,
    ) -> Result<Vec<EmailSummary>, ProtonError> {
        self.validate_config()?;
        self.imap.fetch_mailbox(mailbox, count).await
    }

    #[instrument(skip(self))]
    async fn get_unread_count(&self) -> Result<u32, ProtonError> {
        self.validate_config()?;
        self.imap.get_unread_count().await
    }

    #[instrument(skip(self))]
    async fn mark_read(&self, email_id: &str) -> Result<(), ProtonError> {
        self.validate_config()?;
        self.imap.mark_read(email_id).await
    }

    #[instrument(skip(self))]
    async fn mark_unread(&self, email_id: &str) -> Result<(), ProtonError> {
        self.validate_config()?;
        self.imap.mark_unread(email_id).await
    }

    #[instrument(skip(self))]
    async fn delete(&self, email_id: &str) -> Result<(), ProtonError> {
        self.validate_config()?;
        self.imap.delete(email_id).await
    }

    #[instrument(skip(self, email))]
    async fn send_email(&self, email: &EmailComposition) -> Result<String, ProtonError> {
        self.validate_config()?;
        email.validate()?;
        self.smtp.send_email(email).await
    }

    #[instrument(skip(self))]
    async fn check_connection(&self) -> Result<bool, ProtonError> {
        let imap_ok = self.imap.check_connection().await.unwrap_or(false);
        let smtp_ok = self.smtp.check_connection().await.unwrap_or(false);

        debug!(
            imap = %imap_ok,
            smtp = %smtp_ok,
            imap_addr = %self.imap_addr(),
            smtp_addr = %self.smtp_addr(),
            "Bridge connection check"
        );

        Ok(imap_ok && smtp_ok)
    }

    #[instrument(skip(self))]
    async fn list_mailboxes(&self) -> Result<Vec<String>, ProtonError> {
        self.validate_config()?;
        self.imap.list_mailboxes().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // TlsConfig tests

    #[test]
    fn tls_config_default_verifies_certs() {
        let config = TlsConfig::default();
        assert!(config.should_verify());
        assert!(config.verify_certificates.is_none());
    }

    #[test]
    fn tls_config_insecure_does_not_verify() {
        let config = TlsConfig::insecure();
        assert!(!config.should_verify());
        assert_eq!(config.verify_certificates, Some(false));
    }

    #[test]
    fn tls_config_strict_verifies() {
        let config = TlsConfig::strict();
        assert!(config.should_verify());
        assert_eq!(config.verify_certificates, Some(true));
    }

    #[test]
    fn tls_config_with_ca_cert_verifies() {
        let config = TlsConfig::with_ca_cert("/path/to/ca.pem");
        assert!(config.should_verify());
        assert!(config.ca_cert_path.is_some());
    }

    #[test]
    fn tls_config_none_defaults_to_true() {
        // Directly test that None results in verification
        let config = TlsConfig {
            verify_certificates: None,
            ca_cert_path: None,
            min_tls_version: "1.2".to_string(),
        };
        assert!(config.should_verify());
    }

    #[test]
    fn tls_config_explicit_false_disables() {
        let config = TlsConfig {
            verify_certificates: Some(false),
            ca_cert_path: None,
            min_tls_version: "1.2".to_string(),
        };
        assert!(!config.should_verify());
    }

    #[test]
    fn tls_config_serialization_with_none() {
        let config = TlsConfig::default();
        let json = serde_json::to_string(&config).unwrap();
        // With serde(default), None should serialize as omitted or null
        // and deserialize back correctly
        let deserialized: TlsConfig = serde_json::from_str(&json).unwrap();
        assert!(deserialized.should_verify());
    }

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
    fn proton_error_imap_error() {
        let err = ProtonError::ImapError("connection reset".to_string());
        assert_eq!(err.to_string(), "IMAP error: connection reset");
    }

    #[test]
    fn proton_error_invalid_address() {
        let err = ProtonError::InvalidAddress("not-an-email".to_string());
        assert_eq!(err.to_string(), "Invalid email address: not-an-email");
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
    fn proton_config_with_credentials() {
        let config = ProtonConfig::with_credentials("user@proton.me", "secret");
        assert_eq!(config.email, "user@proton.me");
        assert_eq!(config.password, "secret");
    }

    #[test]
    fn proton_config_builder_pattern() {
        let config = ProtonConfig::with_credentials("user@proton.me", "secret")
            .with_imap("localhost", 993)
            .with_smtp("localhost", 587);

        assert_eq!(config.imap_host, "localhost");
        assert_eq!(config.imap_port, 993);
        assert_eq!(config.smtp_host, "localhost");
        assert_eq!(config.smtp_port, 587);
    }

    #[test]
    fn proton_config_validation_empty_email() {
        let config = ProtonConfig::default();
        let result = config.validate();
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            ProtonError::InvalidAddress(_)
        ));
    }

    #[test]
    fn proton_config_validation_invalid_email() {
        let config = ProtonConfig {
            email: "not-an-email".to_string(),
            password: "secret".to_string(),
            ..Default::default()
        };
        let result = config.validate();
        assert!(result.is_err());
    }

    #[test]
    fn proton_config_validation_empty_password() {
        let config = ProtonConfig {
            email: "user@proton.me".to_string(),
            password: String::new(),
            ..Default::default()
        };
        let result = config.validate();
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            ProtonError::AuthenticationFailed
        ));
    }

    #[test]
    fn proton_config_validation_success() {
        let config = ProtonConfig::with_credentials("user@proton.me", "secret");
        assert!(config.validate().is_ok());
    }

    #[test]
    fn proton_config_serialization() {
        let config = ProtonConfig::with_credentials("user@proton.me", "secret");
        let json = serde_json::to_string(&config).unwrap();
        assert!(json.contains("imap_host"));
        assert!(json.contains("smtp_port"));
        // Password should be skipped during serialization
        assert!(!json.contains("secret"));
    }

    #[test]
    fn proton_config_deserialization() {
        let json = r#"{"imap_host":"127.0.0.1","imap_port":1143,"smtp_host":"127.0.0.1","smtp_port":1025,"email":"test@proton.me","password":"secret"}"#;
        let config: ProtonConfig = serde_json::from_str(json).unwrap();
        assert_eq!(config.email, "test@proton.me");
    }

    #[test]
    fn proton_config_clone() {
        let config = ProtonConfig::with_credentials("a@b.com", "pass");
        #[allow(clippy::redundant_clone)]
        let cloned = config.clone();
        assert_eq!(config.email, cloned.email);
    }

    #[test]
    fn proton_config_has_debug() {
        let config = ProtonConfig::default();
        let debug = format!("{config:?}");
        assert!(debug.contains("ProtonConfig"));
    }

    #[test]
    fn email_summary_creation() {
        let email = EmailSummary::new("mail123", "sender@example.com", "Hello");
        assert_eq!(email.id, "mail123");
        assert_eq!(email.from, "sender@example.com");
        assert_eq!(email.subject, "Hello");
        assert!(!email.is_read);
        assert!(!email.is_important);
    }

    #[test]
    fn email_summary_builder_pattern() {
        let email = EmailSummary::new("1", "a@b.com", "Test")
            .with_snippet("Preview text...")
            .with_received_at("2026-02-01T10:00:00Z")
            .with_read(true)
            .with_important(true);

        assert_eq!(email.snippet, "Preview text...");
        assert!(email.is_read);
        assert!(email.is_important);
    }

    #[test]
    fn email_summary_serialization() {
        let email = EmailSummary::new("1", "a@b.com", "Test");
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
        let email = EmailSummary::new("1", "a@b.com", "Test");
        let debug = format!("{email:?}");
        assert!(debug.contains("EmailSummary"));
    }

    #[test]
    fn email_summary_clone() {
        let email = EmailSummary::new("1", "a@b.com", "Test").with_important(true);
        #[allow(clippy::redundant_clone)]
        let cloned = email.clone();
        assert_eq!(email.id, cloned.id);
        assert_eq!(email.is_important, cloned.is_important);
    }

    #[test]
    fn email_summary_equality() {
        let email1 = EmailSummary::new("1", "a@b.com", "Test");
        #[allow(clippy::redundant_clone)]
        let email2 = email1.clone();
        assert_eq!(email1, email2);
    }

    #[test]
    fn email_composition_creation() {
        let email = EmailComposition::new("recipient@example.com", "Hello", "Body text");
        assert_eq!(email.to, "recipient@example.com");
        assert_eq!(email.subject, "Hello");
        assert_eq!(email.body, "Body text");
        assert!(email.cc.is_empty());
    }

    #[test]
    fn email_composition_with_cc() {
        let email = EmailComposition::new("a@b.com", "Hi", "Hello")
            .with_cc("cc1@example.com")
            .with_cc("cc2@example.com");

        assert_eq!(email.cc.len(), 2);
        assert!(email.cc.contains(&"cc1@example.com".to_string()));
    }

    #[test]
    fn email_composition_with_cc_list() {
        let cc_list = vec!["cc1@example.com".to_string(), "cc2@example.com".to_string()];
        let email = EmailComposition::new("a@b.com", "Hi", "Hello").with_cc_list(cc_list);

        assert_eq!(email.cc.len(), 2);
    }

    #[test]
    fn email_composition_validation_invalid_to() {
        let email = EmailComposition::new("not-an-email", "Hi", "Hello");
        assert!(email.validate().is_err());
    }

    #[test]
    fn email_composition_validation_empty_subject() {
        let email = EmailComposition::new("a@b.com", "", "Hello");
        assert!(email.validate().is_err());
    }

    #[test]
    fn email_composition_validation_invalid_cc() {
        let email = EmailComposition::new("a@b.com", "Hi", "Hello").with_cc("not-valid");
        assert!(email.validate().is_err());
    }

    #[test]
    fn email_composition_validation_success() {
        let email = EmailComposition::new("a@b.com", "Hi", "Hello").with_cc("cc@example.com");
        assert!(email.validate().is_ok());
    }

    #[test]
    fn email_composition_serialization() {
        let email = EmailComposition::new("a@b.com", "Hi", "Hello");
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
        let email = EmailComposition::new("a@b.com", "Test", "Body");
        let debug = format!("{email:?}");
        assert!(debug.contains("EmailComposition"));
    }

    #[test]
    fn email_composition_clone() {
        let email = EmailComposition::new("a@b.com", "Test", "Body").with_cc("c@d.com");
        #[allow(clippy::redundant_clone)]
        let cloned = email.clone();
        assert_eq!(email.to, cloned.to);
        assert_eq!(email.cc, cloned.cc);
    }

    #[test]
    fn proton_bridge_client_creation() {
        let config = ProtonConfig::with_credentials("user@proton.me", "secret");
        let client = ProtonBridgeClient::new(config);
        assert!(format!("{client:?}").contains("ProtonBridgeClient"));
    }

    #[test]
    fn proton_bridge_client_imap_addr() {
        let config =
            ProtonConfig::with_credentials("user@proton.me", "secret").with_imap("localhost", 993);
        let client = ProtonBridgeClient::new(config);
        assert_eq!(client.imap_addr(), "localhost:993");
    }

    #[test]
    fn proton_bridge_client_smtp_addr() {
        let config =
            ProtonConfig::with_credentials("user@proton.me", "secret").with_smtp("localhost", 587);
        let client = ProtonBridgeClient::new(config);
        assert_eq!(client.smtp_addr(), "localhost:587");
    }

    #[test]
    fn proton_bridge_client_validate_config() {
        let config = ProtonConfig::with_credentials("user@proton.me", "secret");
        let client = ProtonBridgeClient::new(config);
        assert!(client.validate_config().is_ok());
    }

    #[test]
    fn proton_bridge_client_validate_config_fails() {
        let config = ProtonConfig::default();
        let client = ProtonBridgeClient::new(config);
        assert!(client.validate_config().is_err());
    }

    #[tokio::test]
    async fn proton_bridge_client_check_connection_fails_no_bridge() {
        let config = ProtonConfig::with_credentials("user@proton.me", "secret")
            .with_imap("127.0.0.1", 19998)
            .with_smtp("127.0.0.1", 19999);
        let client = ProtonBridgeClient::new(config);
        let result = client.check_connection().await;
        assert!(result.is_ok());
        assert!(!result.unwrap());
    }

    #[tokio::test]
    async fn proton_bridge_client_get_inbox_fails_without_valid_config() {
        let config = ProtonConfig::default();
        let client = ProtonBridgeClient::new(config);
        let result = client.get_inbox(10).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn proton_bridge_client_send_email_validates_composition() {
        let config = ProtonConfig::with_credentials("user@proton.me", "secret");
        let client = ProtonBridgeClient::new(config);
        let email = EmailComposition::new("not-valid", "Hi", "Hello");
        let result = client.send_email(&email).await;
        assert!(result.is_err());
    }
}
