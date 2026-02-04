//! Proton Email adapter - Implements EmailPort using integration_proton

use application::ports::{EmailDraft, EmailError, EmailPort, EmailSummary};
use async_trait::async_trait;
use integration_proton::{
    EmailComposition, EmailSummary as ProtonEmailSummary, ProtonBridgeClient, ProtonClient,
    ProtonConfig, ProtonError,
};
use tracing::{debug, instrument};

/// Adapter for Proton Mail via Proton Bridge
#[derive(Debug)]
pub struct ProtonEmailAdapter {
    client: ProtonBridgeClient,
}

impl ProtonEmailAdapter {
    /// Create a new adapter with the given configuration
    pub fn new(config: ProtonConfig) -> Self {
        let client = ProtonBridgeClient::new(config);
        Self { client }
    }

    /// Create with specific IMAP/SMTP ports
    pub fn with_ports(
        email: &str,
        password: &str,
        imap_host: &str,
        imap_port: u16,
        smtp_host: &str,
        smtp_port: u16,
    ) -> Self {
        let config = ProtonConfig::with_credentials(email, password)
            .with_imap(imap_host, imap_port)
            .with_smtp(smtp_host, smtp_port);
        Self::new(config)
    }

    /// Map ProtonError to EmailError
    fn map_error(e: ProtonError) -> EmailError {
        match e {
            ProtonError::AuthenticationFailed => EmailError::AuthenticationFailed,
            ProtonError::BridgeUnavailable(_) | ProtonError::ConnectionFailed(_) => {
                EmailError::ServiceUnavailable
            },
            ProtonError::MailboxNotFound(name) => EmailError::NotFound(name),
            ProtonError::MessageNotFound(id) => EmailError::NotFound(id),
            ProtonError::InvalidAddress(addr) => EmailError::InvalidAddress(addr),
            other => EmailError::OperationFailed(other.to_string()),
        }
    }

    /// Convert ProtonEmailSummary to port EmailSummary
    fn convert_summary(summary: &ProtonEmailSummary) -> EmailSummary {
        EmailSummary::new(&summary.id, &summary.from, &summary.subject)
            .with_snippet(&summary.snippet)
            .with_received_at(&summary.received_at)
            .with_is_read(summary.is_read)
    }
}

#[async_trait]
impl EmailPort for ProtonEmailAdapter {
    #[instrument(skip(self))]
    async fn get_inbox(&self, count: u32) -> Result<Vec<EmailSummary>, EmailError> {
        debug!(count, "Getting inbox from Proton");

        let emails = self
            .client
            .get_inbox(count)
            .await
            .map_err(Self::map_error)?;

        Ok(emails.iter().map(Self::convert_summary).collect())
    }

    #[instrument(skip(self))]
    async fn get_mailbox(
        &self,
        mailbox: &str,
        count: u32,
    ) -> Result<Vec<EmailSummary>, EmailError> {
        debug!(mailbox, count, "Getting mailbox from Proton");

        let emails = self
            .client
            .get_mailbox(mailbox, count)
            .await
            .map_err(Self::map_error)?;

        Ok(emails.iter().map(Self::convert_summary).collect())
    }

    #[instrument(skip(self))]
    async fn get_unread_count(&self) -> Result<u32, EmailError> {
        self.client
            .get_unread_count()
            .await
            .map_err(Self::map_error)
    }

    #[instrument(skip(self))]
    async fn mark_read(&self, email_id: &str) -> Result<(), EmailError> {
        self.client
            .mark_read(email_id)
            .await
            .map_err(Self::map_error)
    }

    #[instrument(skip(self))]
    async fn mark_unread(&self, email_id: &str) -> Result<(), EmailError> {
        self.client
            .mark_unread(email_id)
            .await
            .map_err(Self::map_error)
    }

    #[instrument(skip(self))]
    async fn delete(&self, email_id: &str) -> Result<(), EmailError> {
        self.client.delete(email_id).await.map_err(Self::map_error)
    }

    #[instrument(skip(self, draft))]
    async fn send_email(&self, draft: &EmailDraft) -> Result<String, EmailError> {
        debug!(to = %draft.to, subject = %draft.subject, "Sending email via Proton");

        let mut composition = EmailComposition::new(&draft.to, &draft.subject, &draft.body);

        for cc in &draft.cc {
            composition = composition.with_cc(cc);
        }

        self.client
            .send_email(&composition)
            .await
            .map_err(Self::map_error)
    }

    async fn is_available(&self) -> bool {
        self.client.check_connection().await.unwrap_or(false)
    }

    #[instrument(skip(self))]
    async fn list_mailboxes(&self) -> Result<Vec<String>, EmailError> {
        self.client.list_mailboxes().await.map_err(Self::map_error)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_config() -> ProtonConfig {
        ProtonConfig::with_credentials("test@proton.me", "test-password")
            .with_imap("127.0.0.1", 1143)
            .with_smtp("127.0.0.1", 1025)
    }

    #[test]
    fn adapter_creation() {
        let adapter = ProtonEmailAdapter::new(test_config());
        assert!(format!("{adapter:?}").contains("ProtonEmailAdapter"));
    }

    #[test]
    fn adapter_with_ports() {
        let adapter = ProtonEmailAdapter::with_ports(
            "test@proton.me",
            "password",
            "localhost",
            1143,
            "localhost",
            1025,
        );
        assert!(format!("{adapter:?}").contains("ProtonEmailAdapter"));
    }

    #[test]
    fn map_error_auth_failed() {
        let err = ProtonEmailAdapter::map_error(ProtonError::AuthenticationFailed);
        assert!(matches!(err, EmailError::AuthenticationFailed));
    }

    #[test]
    fn map_error_bridge_unavailable() {
        let err = ProtonEmailAdapter::map_error(ProtonError::BridgeUnavailable("test".to_string()));
        assert!(matches!(err, EmailError::ServiceUnavailable));
    }

    #[test]
    fn map_error_connection_failed() {
        let err = ProtonEmailAdapter::map_error(ProtonError::ConnectionFailed("test".to_string()));
        assert!(matches!(err, EmailError::ServiceUnavailable));
    }

    #[test]
    fn map_error_mailbox_not_found() {
        let err = ProtonEmailAdapter::map_error(ProtonError::MailboxNotFound("INBOX".to_string()));
        assert!(matches!(err, EmailError::NotFound(_)));
    }

    #[test]
    fn map_error_message_not_found() {
        let err = ProtonEmailAdapter::map_error(ProtonError::MessageNotFound("123".to_string()));
        assert!(matches!(err, EmailError::NotFound(_)));
    }

    #[test]
    fn map_error_invalid_address() {
        let err = ProtonEmailAdapter::map_error(ProtonError::InvalidAddress("bad".to_string()));
        assert!(matches!(err, EmailError::InvalidAddress(_)));
    }

    #[test]
    fn map_error_other() {
        let err = ProtonEmailAdapter::map_error(ProtonError::RequestFailed("test".to_string()));
        assert!(matches!(err, EmailError::OperationFailed(_)));
    }

    #[test]
    fn convert_summary() {
        let proton_summary = ProtonEmailSummary::new("123", "sender@example.com", "Test Subject")
            .with_snippet("Preview text")
            .with_read(true);

        let summary = ProtonEmailAdapter::convert_summary(&proton_summary);

        assert_eq!(summary.id, "123");
        assert_eq!(summary.from, "sender@example.com");
        assert_eq!(summary.subject, "Test Subject");
        assert_eq!(summary.snippet, "Preview text");
        assert!(summary.is_read);
    }

    #[tokio::test]
    async fn is_available_returns_false_when_no_bridge() {
        let config = ProtonConfig::with_credentials("test@proton.me", "password")
            .with_imap("127.0.0.1", 19999) // Non-existent port
            .with_smtp("127.0.0.1", 19998);

        let adapter = ProtonEmailAdapter::new(config);
        assert!(!adapter.is_available().await);
    }
}
