//! IMAP client implementation for Proton Bridge
//!
//! Provides async IMAP operations for reading, managing, and organizing emails.
//! Uses the synchronous `imap` crate wrapped in `spawn_blocking` for async compatibility.

use std::{fs, net::TcpStream};

use imap::Session;
use native_tls::{Certificate, TlsConnector};
use tracing::{debug, error, instrument, warn};

use crate::{EmailSummary, ProtonConfig, ProtonError, TlsConfig};

/// Type alias for IMAP session over TLS
type ImapSession = Session<native_tls::TlsStream<TcpStream>>;

/// IMAP client for Proton Bridge
///
/// Manages IMAP connections and provides methods for mailbox operations.
/// Uses synchronous operations wrapped in `spawn_blocking` for async interface.
#[derive(Debug, Clone)]
pub struct ProtonImapClient {
    config: ProtonConfig,
}

impl ProtonImapClient {
    /// Creates a new IMAP client with the given configuration
    pub const fn new(config: ProtonConfig) -> Self {
        Self { config }
    }

    /// Builds a TLS connector based on the TLS configuration
    fn build_tls_connector(tls_config: &TlsConfig) -> Result<TlsConnector, ProtonError> {
        let mut builder = native_tls::TlsConnector::builder();

        // Configure certificate verification
        if !tls_config.verify_certificates {
            warn!(
                "⚠️ TLS certificate verification disabled - only recommended for local Proton Bridge"
            );
            builder.danger_accept_invalid_certs(true);
        } else if let Some(ca_cert_path) = &tls_config.ca_cert_path {
            // Load custom CA certificate
            debug!(path = %ca_cert_path.display(), "Loading custom CA certificate");
            let cert_data = fs::read(ca_cert_path).map_err(|e| {
                ProtonError::ConnectionFailed(format!(
                    "Failed to read CA certificate at {}: {e}",
                    ca_cert_path.display()
                ))
            })?;
            let cert = Certificate::from_pem(&cert_data).map_err(|e| {
                ProtonError::ConnectionFailed(format!("Failed to parse CA certificate: {e}"))
            })?;
            builder.add_root_certificate(cert);
        }

        // Configure minimum TLS version
        let min_protocol = match tls_config.min_tls_version.as_str() {
            "1.0" => native_tls::Protocol::Tlsv10,
            "1.1" => native_tls::Protocol::Tlsv11,
            _ => native_tls::Protocol::Tlsv12,
        };
        builder.min_protocol_version(Some(min_protocol));

        builder
            .build()
            .map_err(|e| ProtonError::ConnectionFailed(format!("TLS builder failed: {e}")))
    }

    /// Establishes a new IMAP connection
    fn connect_sync(config: &ProtonConfig) -> Result<ImapSession, ProtonError> {
        let addr = format!("{}:{}", config.imap_host, config.imap_port);
        debug!(addr = %addr, "Connecting to IMAP server");

        // Connect to the IMAP server
        let tcp_stream = TcpStream::connect(&addr).map_err(|e| {
            error!(error = %e, "Failed to connect to IMAP server");
            ProtonError::ConnectionFailed(format!("TCP connection failed: {e}"))
        })?;

        // Create TLS connector with config-based settings
        let tls = Self::build_tls_connector(&config.tls)?;

        // Wrap with TLS
        let tls_stream = tls.connect(&config.imap_host, tcp_stream).map_err(|e| {
            error!(error = %e, "TLS handshake failed");
            ProtonError::ConnectionFailed(format!("TLS handshake failed: {e}"))
        })?;

        // Create IMAP client and login
        let client = imap::Client::new(tls_stream);
        let session = client.login(&config.email, &config.password).map_err(|e| {
            error!(error = ?e.0, "IMAP login failed");
            ProtonError::AuthenticationFailed
        })?;

        debug!("IMAP login successful");
        Ok(session)
    }

    /// Fetches emails from a mailbox
    #[instrument(skip(self))]
    pub async fn fetch_mailbox(
        &self,
        mailbox: &str,
        count: u32,
    ) -> Result<Vec<EmailSummary>, ProtonError> {
        let config = self.config.clone();
        let mailbox = mailbox.to_string();

        tokio::task::spawn_blocking(move || Self::fetch_mailbox_sync(&config, &mailbox, count))
            .await
            .map_err(|e| ProtonError::RequestFailed(format!("Task join error: {e}")))?
    }

    /// Synchronous implementation of fetch_mailbox
    fn fetch_mailbox_sync(
        config: &ProtonConfig,
        mailbox: &str,
        count: u32,
    ) -> Result<Vec<EmailSummary>, ProtonError> {
        let mut session = Self::connect_sync(config)?;

        // Select the mailbox
        let mailbox_info = session.select(mailbox).map_err(|e| {
            if e.to_string().contains("NO") {
                ProtonError::MailboxNotFound(mailbox.to_string())
            } else {
                ProtonError::RequestFailed(format!("Failed to select mailbox: {e}"))
            }
        })?;

        let exists = mailbox_info.exists;
        if exists == 0 {
            debug!(mailbox = %mailbox, "Mailbox is empty");
            session.logout().ok();
            return Ok(Vec::new());
        }

        // Calculate sequence range (newest first)
        let start = exists.saturating_sub(count) + 1;
        let end = exists;
        let range = format!("{start}:{end}");

        debug!(mailbox = %mailbox, range = %range, "Fetching emails");

        // Fetch messages with envelope and flags
        let messages = session
            .fetch(&range, "(UID FLAGS ENVELOPE BODY.PEEK[TEXT]<0.200>)")
            .map_err(|e| {
                error!(error = %e, "Failed to fetch messages");
                ProtonError::RequestFailed(format!("Failed to fetch messages: {e}"))
            })?;

        let mut emails = Vec::new();
        for msg in messages.iter() {
            if let Some(email) = Self::parse_fetch_result(msg) {
                emails.push(email);
            }
        }

        // Reverse to get newest first
        emails.reverse();

        session.logout().ok();
        Ok(emails)
    }

    /// Parses a fetch result into an `EmailSummary`
    fn parse_fetch_result(fetch: &imap::types::Fetch) -> Option<EmailSummary> {
        let uid = fetch.uid?;
        let envelope = fetch.envelope()?;

        // Extract sender
        let from = envelope
            .from
            .as_ref()
            .and_then(|addrs| addrs.first())
            .map_or_else(
                || "unknown".to_string(),
                |addr| {
                    let mailbox = addr
                        .mailbox
                        .as_ref()
                        .map(|m| String::from_utf8_lossy(m).to_string());
                    let host = addr
                        .host
                        .as_ref()
                        .map(|h| String::from_utf8_lossy(h).to_string());
                    match (mailbox, host) {
                        (Some(m), Some(h)) => format!("{m}@{h}"),
                        (Some(m), None) => m,
                        _ => "unknown".to_string(),
                    }
                },
            );

        // Extract subject
        let subject = envelope
            .subject
            .as_ref()
            .map(|s| String::from_utf8_lossy(s).to_string())
            .unwrap_or_default();

        // Extract date
        let received_at = envelope.date.as_ref().map_or_else(
            || chrono::Utc::now().to_rfc3339(),
            |d| String::from_utf8_lossy(d).to_string(),
        );

        // Extract snippet from body
        let snippet = fetch
            .text()
            .map(|text| {
                let text_str = String::from_utf8_lossy(text);
                // Take first 200 chars, clean up whitespace
                text_str
                    .chars()
                    .take(200)
                    .collect::<String>()
                    .split_whitespace()
                    .collect::<Vec<_>>()
                    .join(" ")
            })
            .unwrap_or_default();

        // Check flags
        let flags = fetch.flags();
        let is_read = flags.iter().any(|f| matches!(f, imap::types::Flag::Seen));
        let is_important = flags
            .iter()
            .any(|f| matches!(f, imap::types::Flag::Flagged));

        Some(EmailSummary {
            id: uid.to_string(),
            from,
            subject,
            snippet,
            received_at,
            is_read,
            is_important,
        })
    }

    /// Gets the unread message count for INBOX
    #[instrument(skip(self))]
    pub async fn get_unread_count(&self) -> Result<u32, ProtonError> {
        let config = self.config.clone();

        tokio::task::spawn_blocking(move || {
            let mut session = Self::connect_sync(&config)?;

            // Select INBOX to get status
            let _mailbox = session
                .select("INBOX")
                .map_err(|e| ProtonError::RequestFailed(format!("Failed to select INBOX: {e}")))?;

            // Search for unseen messages
            let unseen = session
                .search("UNSEEN")
                .map_err(|e| ProtonError::RequestFailed(format!("Search UNSEEN failed: {e}")))?;

            #[allow(clippy::cast_possible_truncation)]
            let count = unseen.len() as u32;
            session.logout().ok();
            Ok(count)
        })
        .await
        .map_err(|e| ProtonError::RequestFailed(format!("Task join error: {e}")))?
    }

    /// Marks an email as read (adds \Seen flag)
    #[instrument(skip(self))]
    pub async fn mark_read(&self, uid: &str) -> Result<(), ProtonError> {
        let config = self.config.clone();
        let uid = uid.to_string();

        tokio::task::spawn_blocking(move || {
            let mut session = Self::connect_sync(&config)?;
            session
                .select("INBOX")
                .map_err(|e| ProtonError::RequestFailed(format!("Failed to select INBOX: {e}")))?;

            session.uid_store(&uid, "+FLAGS (\\Seen)").map_err(|e| {
                if e.to_string().contains("NO") {
                    ProtonError::MessageNotFound(uid.clone())
                } else {
                    ProtonError::RequestFailed(format!("Failed to mark read: {e}"))
                }
            })?;

            session.logout().ok();
            Ok(())
        })
        .await
        .map_err(|e| ProtonError::RequestFailed(format!("Task join error: {e}")))?
    }

    /// Marks an email as unread (removes \Seen flag)
    #[instrument(skip(self))]
    pub async fn mark_unread(&self, uid: &str) -> Result<(), ProtonError> {
        let config = self.config.clone();
        let uid = uid.to_string();

        tokio::task::spawn_blocking(move || {
            let mut session = Self::connect_sync(&config)?;
            session
                .select("INBOX")
                .map_err(|e| ProtonError::RequestFailed(format!("Failed to select INBOX: {e}")))?;

            session.uid_store(&uid, "-FLAGS (\\Seen)").map_err(|e| {
                if e.to_string().contains("NO") {
                    ProtonError::MessageNotFound(uid.clone())
                } else {
                    ProtonError::RequestFailed(format!("Failed to mark unread: {e}"))
                }
            })?;

            session.logout().ok();
            Ok(())
        })
        .await
        .map_err(|e| ProtonError::RequestFailed(format!("Task join error: {e}")))?
    }

    /// Deletes an email (moves to Trash and expunges)
    #[instrument(skip(self))]
    pub async fn delete(&self, uid: &str) -> Result<(), ProtonError> {
        let config = self.config.clone();
        let uid = uid.to_string();

        tokio::task::spawn_blocking(move || {
            let mut session = Self::connect_sync(&config)?;
            session
                .select("INBOX")
                .map_err(|e| ProtonError::RequestFailed(format!("Failed to select INBOX: {e}")))?;

            // Copy to Trash
            session.uid_copy(&uid, "Trash").map_err(|e| {
                if e.to_string().contains("NO") {
                    ProtonError::MessageNotFound(uid.clone())
                } else {
                    ProtonError::RequestFailed(format!("Failed to copy to Trash: {e}"))
                }
            })?;

            // Mark as deleted
            session
                .uid_store(&uid, "+FLAGS (\\Deleted)")
                .map_err(|e| ProtonError::RequestFailed(format!("Failed to mark deleted: {e}")))?;

            // Expunge
            session
                .expunge()
                .map_err(|e| ProtonError::RequestFailed(format!("Expunge failed: {e}")))?;

            session.logout().ok();
            Ok(())
        })
        .await
        .map_err(|e| ProtonError::RequestFailed(format!("Task join error: {e}")))?
    }

    /// Checks if the IMAP server is reachable
    #[instrument(skip(self))]
    pub async fn check_connection(&self) -> Result<bool, ProtonError> {
        let addr = format!("{}:{}", self.config.imap_host, self.config.imap_port);

        match tokio::net::TcpStream::connect(&addr).await {
            Ok(_) => {
                debug!("IMAP server is reachable");
                Ok(true)
            },
            Err(e) => {
                debug!(error = %e, "IMAP server is not reachable");
                Ok(false)
            },
        }
    }

    /// Lists available mailboxes
    #[instrument(skip(self))]
    pub async fn list_mailboxes(&self) -> Result<Vec<String>, ProtonError> {
        let config = self.config.clone();

        tokio::task::spawn_blocking(move || {
            let mut session = Self::connect_sync(&config)?;

            let mailboxes = session
                .list(Some(""), Some("*"))
                .map_err(|e| ProtonError::RequestFailed(format!("LIST command failed: {e}")))?;

            let names: Vec<String> = mailboxes.iter().map(|mb| mb.name().to_string()).collect();

            session.logout().ok();
            Ok(names)
        })
        .await
        .map_err(|e| ProtonError::RequestFailed(format!("Task join error: {e}")))?
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::TlsConfig;

    fn test_config() -> ProtonConfig {
        ProtonConfig {
            imap_host: "127.0.0.1".to_string(),
            imap_port: 1143,
            smtp_host: "127.0.0.1".to_string(),
            smtp_port: 1025,
            email: "test@proton.me".to_string(),
            password: "bridge-password".to_string(),
            tls: TlsConfig::insecure(), // Test config uses insecure for local testing
        }
    }

    #[test]
    fn imap_client_creation() {
        let config = test_config();
        let client = ProtonImapClient::new(config);
        assert!(format!("{client:?}").contains("ProtonImapClient"));
    }

    #[test]
    fn imap_client_has_debug() {
        let client = ProtonImapClient::new(test_config());
        let debug = format!("{client:?}");
        assert!(debug.contains("ProtonImapClient"));
        assert!(debug.contains("config"));
    }

    #[test]
    fn imap_client_clone() {
        let client = ProtonImapClient::new(test_config());
        #[allow(clippy::redundant_clone)]
        let cloned = client.clone();
        assert!(format!("{cloned:?}").contains("ProtonImapClient"));
    }

    #[tokio::test]
    async fn check_connection_fails_for_unavailable_server() {
        let config = ProtonConfig {
            imap_port: 19999, // Non-existent port
            ..test_config()
        };
        let client = ProtonImapClient::new(config);
        let result = client.check_connection().await;
        assert!(result.is_ok());
        assert!(!result.unwrap());
    }
}
