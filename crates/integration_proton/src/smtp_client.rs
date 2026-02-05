//! SMTP client implementation for Proton Bridge
//!
//! Provides async SMTP operations for sending emails via Proton Bridge.
//! This is a lightweight implementation using tokio and tokio-native-tls.

use std::fs;

use base64::Engine;
use native_tls::Certificate;
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    net::TcpStream,
};
use tokio_native_tls::TlsConnector;
use tracing::{debug, error, instrument, trace, warn};

use crate::{EmailComposition, ProtonConfig, ProtonError, TlsConfig};

/// SMTP client for Proton Bridge
///
/// Manages SMTP connections for sending emails through Proton Bridge.
/// Supports STARTTLS and PLAIN authentication.
#[derive(Debug, Clone)]
pub struct ProtonSmtpClient {
    config: ProtonConfig,
}

impl ProtonSmtpClient {
    /// Creates a new SMTP client with the given configuration
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

        let native_connector = builder
            .build()
            .map_err(|e| ProtonError::ConnectionFailed(format!("TLS builder failed: {e}")))?;

        Ok(TlsConnector::from(native_connector))
    }

    /// Sends an email
    #[instrument(skip(self, email))]
    pub async fn send_email(&self, email: &EmailComposition) -> Result<String, ProtonError> {
        debug!(to = %email.to, subject = %email.subject, "Sending email");

        // Generate message ID
        let message_id = format!(
            "<{}.{}@{}>",
            chrono::Utc::now().timestamp_millis(),
            uuid::Uuid::new_v4(),
            Self::extract_domain(&self.config.email)
        );

        // Build the email message
        let email_content = self.build_email_content(email, &message_id);

        // Send via SMTP
        self.send_smtp(&email.to, &email.cc, &email_content).await?;

        debug!(message_id = %message_id, "Email sent successfully");
        Ok(message_id)
    }

    /// Builds the email content in RFC 5322 format
    fn build_email_content(&self, email: &EmailComposition, message_id: &str) -> String {
        let date = chrono::Utc::now().format("%a, %d %b %Y %H:%M:%S +0000");

        let mut headers = format!(
            "From: {}\r\n\
             To: {}\r\n\
             Subject: {}\r\n\
             Date: {}\r\n\
             Message-ID: {}\r\n\
             MIME-Version: 1.0\r\n\
             Content-Type: text/plain; charset=utf-8\r\n\
             Content-Transfer-Encoding: 8bit\r\n",
            self.config.email, email.to, email.subject, date, message_id
        );

        // Add CC headers
        if !email.cc.is_empty() {
            headers.push_str(&format!("Cc: {}\r\n", email.cc.join(", ")));
        }

        // Add blank line separator and body
        format!("{headers}\r\n{}", email.body)
    }

    /// Sends the email via SMTP
    async fn send_smtp(&self, to: &str, cc: &[String], content: &str) -> Result<(), ProtonError> {
        let addr = format!("{}:{}", self.config.smtp_host, self.config.smtp_port);

        // Connect to SMTP server
        let stream = TcpStream::connect(&addr).await.map_err(|e| {
            error!(error = %e, "Failed to connect to SMTP server");
            ProtonError::ConnectionFailed(format!("SMTP connection failed: {e}"))
        })?;

        // Build TLS connector with config-based settings
        let tls = Self::build_tls_connector(&self.config.tls)?;

        // For port 465, use implicit TLS
        if self.config.smtp_port == 465 {
            let tls_stream = tls
                .connect(&self.config.smtp_host, stream)
                .await
                .map_err(|e| ProtonError::ConnectionFailed(format!("TLS handshake failed: {e}")))?;

            self.smtp_session(tls_stream, to, cc, content).await
        } else {
            // STARTTLS flow
            self.smtp_starttls_session(stream, to, cc, content).await
        }
    }

    /// Handles SMTP session with STARTTLS
    async fn smtp_starttls_session(
        &self,
        stream: TcpStream,
        to: &str,
        cc: &[String],
        content: &str,
    ) -> Result<(), ProtonError> {
        let (reader, mut writer) = tokio::io::split(stream);
        let mut reader = BufReader::new(reader);

        // Read greeting
        self.read_response(&mut reader).await?;

        // Send EHLO
        let hostname = hostname::get().map_or_else(
            |_| "localhost".to_string(),
            |h| h.to_string_lossy().to_string(),
        );

        self.send_command(&mut writer, &format!("EHLO {hostname}"))
            .await?;
        self.read_response(&mut reader).await?;

        // Send STARTTLS
        self.send_command(&mut writer, "STARTTLS").await?;
        self.read_response(&mut reader).await?;

        // Upgrade to TLS with config-based settings
        let stream = reader.into_inner().unsplit(writer);

        let tls = Self::build_tls_connector(&self.config.tls)?;
        let tls_stream = tls
            .connect(&self.config.smtp_host, stream)
            .await
            .map_err(|e| ProtonError::ConnectionFailed(format!("STARTTLS upgrade failed: {e}")))?;

        // Continue with TLS session
        self.smtp_session(tls_stream, to, cc, content).await
    }

    /// Handles SMTP session over TLS
    async fn smtp_session<S>(
        &self,
        stream: S,
        to: &str,
        cc: &[String],
        content: &str,
    ) -> Result<(), ProtonError>
    where
        S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
    {
        let (reader, mut writer) = tokio::io::split(stream);
        let mut reader = BufReader::new(reader);

        // Read greeting (for implicit TLS) or re-send EHLO after STARTTLS
        self.read_response(&mut reader).await?;

        // Send EHLO
        let hostname = hostname::get().map_or_else(
            |_| "localhost".to_string(),
            |h| h.to_string_lossy().to_string(),
        );

        self.send_command(&mut writer, &format!("EHLO {hostname}"))
            .await?;
        self.read_response(&mut reader).await?;

        // Authenticate using AUTH PLAIN
        let auth_string = format!("\0{}\0{}", self.config.email, self.config.password);
        let auth_b64 = base64::engine::general_purpose::STANDARD.encode(auth_string);

        self.send_command(&mut writer, &format!("AUTH PLAIN {auth_b64}"))
            .await?;
        let auth_response = self.read_response(&mut reader).await?;
        if !auth_response.starts_with("235") {
            return Err(ProtonError::AuthenticationFailed);
        }

        // MAIL FROM
        self.send_command(&mut writer, &format!("MAIL FROM:<{}>", self.config.email))
            .await?;
        self.expect_response(&mut reader, "250").await?;

        // RCPT TO (primary recipient)
        self.send_command(&mut writer, &format!("RCPT TO:<{to}>"))
            .await?;
        self.expect_response(&mut reader, "250").await?;

        // RCPT TO (CC recipients)
        for cc_addr in cc {
            self.send_command(&mut writer, &format!("RCPT TO:<{cc_addr}>"))
                .await?;
            self.expect_response(&mut reader, "250").await?;
        }

        // DATA
        self.send_command(&mut writer, "DATA").await?;
        self.expect_response(&mut reader, "354").await?;

        // Send email content (escape dots at start of lines)
        let escaped_content = content.replace("\r\n.", "\r\n..");
        writer
            .write_all(escaped_content.as_bytes())
            .await
            .map_err(|e| ProtonError::SmtpError(format!("Failed to send content: {e}")))?;

        // End DATA with <CRLF>.<CRLF>
        writer
            .write_all(b"\r\n.\r\n")
            .await
            .map_err(|e| ProtonError::SmtpError(format!("Failed to end DATA: {e}")))?;
        writer.flush().await.ok();

        self.expect_response(&mut reader, "250").await?;

        // QUIT
        self.send_command(&mut writer, "QUIT").await?;
        // Don't wait for QUIT response, server may close connection

        Ok(())
    }

    /// Sends an SMTP command
    async fn send_command<W>(&self, writer: &mut W, command: &str) -> Result<(), ProtonError>
    where
        W: tokio::io::AsyncWrite + Unpin,
    {
        trace!(command = %command.split(' ').next().unwrap_or(command), "Sending SMTP command");
        writer
            .write_all(format!("{command}\r\n").as_bytes())
            .await
            .map_err(|e| ProtonError::SmtpError(format!("Failed to send command: {e}")))?;
        writer.flush().await.ok();
        Ok(())
    }

    /// Reads an SMTP response
    async fn read_response<R>(&self, reader: &mut BufReader<R>) -> Result<String, ProtonError>
    where
        R: tokio::io::AsyncRead + Unpin,
    {
        let mut response = String::new();
        loop {
            let mut line = String::new();
            reader
                .read_line(&mut line)
                .await
                .map_err(|e| ProtonError::SmtpError(format!("Failed to read response: {e}")))?;

            trace!(line = %line.trim(), "SMTP response");
            response.push_str(&line);

            // Check if this is the last line (no hyphen after code)
            if line.len() >= 4 && line.chars().nth(3) != Some('-') {
                break;
            }
        }
        Ok(response)
    }

    /// Expects a specific response code
    async fn expect_response<R>(
        &self,
        reader: &mut BufReader<R>,
        expected_code: &str,
    ) -> Result<(), ProtonError>
    where
        R: tokio::io::AsyncRead + Unpin,
    {
        let response = self.read_response(reader).await?;
        if !response.starts_with(expected_code) {
            return Err(ProtonError::SmtpError(format!(
                "Expected {expected_code}, got: {response}"
            )));
        }
        Ok(())
    }

    /// Extracts domain from an email address
    fn extract_domain(email: &str) -> String {
        email
            .split('@')
            .nth(1)
            .unwrap_or("pisovereign.local")
            .to_string()
    }

    /// Checks if the SMTP server is reachable
    #[instrument(skip(self))]
    pub async fn check_connection(&self) -> Result<bool, ProtonError> {
        let addr = format!("{}:{}", self.config.smtp_host, self.config.smtp_port);

        match TcpStream::connect(&addr).await {
            Ok(_) => {
                debug!("SMTP server is reachable");
                Ok(true)
            },
            Err(e) => {
                debug!(error = %e, "SMTP server is not reachable");
                Ok(false)
            },
        }
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
    fn smtp_client_creation() {
        let config = test_config();
        let client = ProtonSmtpClient::new(config);
        assert!(format!("{client:?}").contains("ProtonSmtpClient"));
    }

    #[test]
    fn smtp_client_has_debug() {
        let client = ProtonSmtpClient::new(test_config());
        let debug = format!("{client:?}");
        assert!(debug.contains("ProtonSmtpClient"));
        assert!(debug.contains("config"));
    }

    #[test]
    fn smtp_client_clone() {
        let client = ProtonSmtpClient::new(test_config());
        #[allow(clippy::redundant_clone)]
        let cloned = client.clone();
        assert!(format!("{cloned:?}").contains("ProtonSmtpClient"));
    }

    #[test]
    fn extract_domain_from_email() {
        assert_eq!(
            ProtonSmtpClient::extract_domain("user@proton.me"),
            "proton.me"
        );
        assert_eq!(
            ProtonSmtpClient::extract_domain("test@example.com"),
            "example.com"
        );
    }

    #[test]
    fn extract_domain_fallback() {
        assert_eq!(
            ProtonSmtpClient::extract_domain("invalid-email"),
            "pisovereign.local"
        );
    }

    #[test]
    fn build_email_content_basic() {
        let client = ProtonSmtpClient::new(test_config());
        let email = EmailComposition::new("recipient@example.com", "Test Subject", "Hello World");
        let message_id = "<123@test.local>";

        let content = client.build_email_content(&email, message_id);

        assert!(content.contains("From: test@proton.me"));
        assert!(content.contains("To: recipient@example.com"));
        assert!(content.contains("Subject: Test Subject"));
        assert!(content.contains("Message-ID: <123@test.local>"));
        assert!(content.contains("Hello World"));
    }

    #[test]
    fn build_email_content_with_cc() {
        let client = ProtonSmtpClient::new(test_config());
        let email = EmailComposition::new("to@example.com", "Test", "Body")
            .with_cc("cc1@example.com")
            .with_cc("cc2@example.com");
        let message_id = "<123@test.local>";

        let content = client.build_email_content(&email, message_id);

        assert!(content.contains("Cc: cc1@example.com, cc2@example.com"));
    }

    #[tokio::test]
    async fn check_connection_fails_for_unavailable_server() {
        let config = ProtonConfig {
            smtp_port: 19999, // Non-existent port
            ..test_config()
        };
        let client = ProtonSmtpClient::new(config);
        let result = client.check_connection().await;
        assert!(result.is_ok());
        assert!(!result.unwrap());
    }

    #[test]
    fn email_composition_for_sending() {
        let email = EmailComposition {
            to: "recipient@example.com".to_string(),
            cc: vec!["cc1@example.com".to_string()],
            subject: "Test Subject".to_string(),
            body: "Hello, World!".to_string(),
        };

        assert_eq!(email.to, "recipient@example.com");
        assert_eq!(email.cc.len(), 1);
        assert_eq!(email.subject, "Test Subject");
    }
}
