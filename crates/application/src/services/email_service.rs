//! Email service
//!
//! Business logic for email operations including summarization and sending.

use std::{fmt, sync::Arc};

use tracing::{debug, info, instrument};

use crate::{
    error::ApplicationError,
    ports::{EmailDraft, EmailError, EmailPort, EmailSummary, InferencePort},
};

/// Email service for handling email operations
pub struct EmailService {
    email_port: Arc<dyn EmailPort>,
    inference: Arc<dyn InferencePort>,
}

impl fmt::Debug for EmailService {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("EmailService").finish_non_exhaustive()
    }
}

impl EmailService {
    /// Create a new email service
    pub fn new(email_port: Arc<dyn EmailPort>, inference: Arc<dyn InferencePort>) -> Self {
        Self {
            email_port,
            inference,
        }
    }

    /// Get inbox summary
    ///
    /// # Arguments
    /// * `count` - Maximum number of emails to retrieve
    /// * `only_important` - Filter to important/starred emails only
    #[instrument(skip(self))]
    pub async fn get_inbox_summary(
        &self,
        count: u32,
        only_important: bool,
    ) -> Result<InboxSummary, ApplicationError> {
        info!(count, only_important, "Getting inbox summary");

        let emails = self.email_port.get_inbox(count).await.map_err(map_error)?;
        let unread_count = self
            .email_port
            .get_unread_count()
            .await
            .map_err(map_error)?;

        // Filter to important emails if requested
        let emails: Vec<EmailSummary> = if only_important {
            emails.into_iter().filter(|e| e.is_starred).collect()
        } else {
            emails
        };

        debug!(email_count = emails.len(), "Retrieved emails");

        Ok(InboxSummary {
            total_count: emails.len() as u32,
            unread_count,
            emails,
        })
    }

    /// Summarize emails using AI
    ///
    /// # Arguments
    /// * `count` - Maximum number of emails to summarize
    /// * `only_important` - Only summarize important emails
    #[instrument(skip(self))]
    pub async fn summarize_inbox(
        &self,
        count: u32,
        only_important: bool,
    ) -> Result<String, ApplicationError> {
        let summary = self.get_inbox_summary(count, only_important).await?;

        if summary.emails.is_empty() {
            return Ok("📧 Keine E-Mails zum Zusammenfassen.".to_string());
        }

        // Build prompt for AI summarization
        let email_list: String = summary
            .emails
            .iter()
            .enumerate()
            .map(|(i, e)| {
                format!(
                    "{}. Von: {}\n   Betreff: {}\n   Vorschau: {}",
                    i + 1,
                    e.from,
                    e.subject,
                    e.snippet
                )
            })
            .collect::<Vec<_>>()
            .join("\n\n");

        let prompt = format!(
            "Fasse die folgenden {} E-Mails kurz und prägnant zusammen. \
             Nenne die wichtigsten Punkte und ob Aktionen erforderlich sind:\n\n{}",
            summary.emails.len(),
            email_list
        );

        let response = self.inference.generate(&prompt).await?;

        Ok(format!(
            "📧 Inbox-Zusammenfassung ({} E-Mails, {} ungelesen):\n\n{}",
            summary.total_count, summary.unread_count, response.content
        ))
    }

    /// Get unread email count
    #[instrument(skip(self))]
    pub async fn get_unread_count(&self) -> Result<u32, ApplicationError> {
        self.email_port.get_unread_count().await.map_err(map_error)
    }

    /// Get emails from a specific mailbox
    #[instrument(skip(self))]
    pub async fn get_mailbox(
        &self,
        mailbox: &str,
        count: u32,
    ) -> Result<Vec<EmailSummary>, ApplicationError> {
        self.email_port
            .get_mailbox(mailbox, count)
            .await
            .map_err(map_error)
    }

    /// Draft an email using AI
    ///
    /// # Arguments
    /// * `to` - Recipient email address
    /// * `subject` - Email subject (optional, AI will generate if None)
    /// * `instructions` - Instructions for what the email should say
    #[instrument(skip(self, instructions))]
    pub async fn draft_email(
        &self,
        to: &str,
        subject: Option<&str>,
        instructions: &str,
    ) -> Result<EmailDraft, ApplicationError> {
        info!(to, subject, "Drafting email");

        let prompt = if let Some(subj) = subject {
            format!(
                "Schreibe eine E-Mail an {} mit dem Betreff '{}'.\n\
                 Anweisungen: {}\n\n\
                 Schreibe nur den E-Mail-Text, ohne Betreff-Zeile.",
                to, subj, instructions
            )
        } else {
            format!(
                "Schreibe eine E-Mail an {}.\n\
                 Anweisungen: {}\n\n\
                 Generiere einen passenden Betreff und den E-Mail-Text.\n\
                 Format: BETREFF: [Betreff]\n\nTEXT: [E-Mail-Text]",
                to, instructions
            )
        };

        let response = self.inference.generate(&prompt).await?;

        // Parse response
        let (final_subject, body) = if let Some(subj) = subject {
            (subj.to_string(), response.content)
        } else {
            parse_draft_response(&response.content)
        };

        Ok(EmailDraft::new(to, final_subject, body))
    }

    /// Send an email
    ///
    /// # Arguments
    /// * `draft` - The email to send
    ///
    /// # Returns
    /// Message ID of the sent email
    #[instrument(skip(self, draft))]
    pub async fn send_email(&self, draft: &EmailDraft) -> Result<String, ApplicationError> {
        info!(to = %draft.to, subject = %draft.subject, "Sending email");

        self.email_port.send_email(draft).await.map_err(map_error)
    }

    /// Mark an email as read
    #[instrument(skip(self))]
    pub async fn mark_read(&self, email_id: &str) -> Result<(), ApplicationError> {
        self.email_port.mark_read(email_id).await.map_err(map_error)
    }

    /// Mark an email as unread
    #[instrument(skip(self))]
    pub async fn mark_unread(&self, email_id: &str) -> Result<(), ApplicationError> {
        self.email_port
            .mark_unread(email_id)
            .await
            .map_err(map_error)
    }

    /// Delete an email
    #[instrument(skip(self))]
    pub async fn delete(&self, email_id: &str) -> Result<(), ApplicationError> {
        self.email_port.delete(email_id).await.map_err(map_error)
    }

    /// Check if email service is available
    pub async fn is_available(&self) -> bool {
        self.email_port.is_available().await
    }

    /// List available mailboxes
    pub async fn list_mailboxes(&self) -> Result<Vec<String>, ApplicationError> {
        self.email_port.list_mailboxes().await.map_err(map_error)
    }
}

/// Inbox summary result
#[derive(Debug, Clone)]
pub struct InboxSummary {
    /// Total emails retrieved
    pub total_count: u32,
    /// Unread email count
    pub unread_count: u32,
    /// Email summaries
    pub emails: Vec<EmailSummary>,
}

/// Parse AI response for draft email
fn parse_draft_response(response: &str) -> (String, String) {
    // Try to parse BETREFF: and TEXT: format
    if let Some(betreff_idx) = response.find("BETREFF:") {
        if let Some(text_idx) = response.find("TEXT:") {
            let subject = response[betreff_idx + 8..text_idx].trim().to_string();
            let body = response[text_idx + 5..].trim().to_string();
            return (subject, body);
        }
    }

    // Fallback: first line is subject, rest is body
    let lines: Vec<&str> = response.lines().collect();
    if lines.len() > 1 {
        let subject = lines[0].trim().to_string();
        let body = lines[1..].join("\n").trim().to_string();
        (subject, body)
    } else {
        ("(Kein Betreff)".to_string(), response.to_string())
    }
}

/// Map email error to application error
fn map_error(err: EmailError) -> ApplicationError {
    match err {
        EmailError::ServiceUnavailable => {
            ApplicationError::ExternalService("Email service unavailable".to_string())
        },
        EmailError::AuthenticationFailed => {
            ApplicationError::NotAuthorized("Email authentication failed".to_string())
        },
        EmailError::NotFound(id) => {
            ApplicationError::ExternalService(format!("Email not found: {id}"))
        },
        EmailError::OperationFailed(msg) => ApplicationError::ExternalService(msg),
        EmailError::InvalidAddress(addr) => {
            ApplicationError::CommandFailed(format!("Invalid email address: {addr}"))
        },
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicU32, Ordering};

    use async_trait::async_trait;

    use super::*;

    struct MockEmailPort {
        emails: Vec<EmailSummary>,
        unread_count: AtomicU32,
    }

    impl MockEmailPort {
        fn new(emails: Vec<EmailSummary>, unread: u32) -> Self {
            Self {
                emails,
                unread_count: AtomicU32::new(unread),
            }
        }
    }

    #[async_trait]
    impl EmailPort for MockEmailPort {
        async fn get_inbox(&self, count: u32) -> Result<Vec<EmailSummary>, EmailError> {
            Ok(self.emails.iter().take(count as usize).cloned().collect())
        }

        async fn get_mailbox(
            &self,
            _mailbox: &str,
            count: u32,
        ) -> Result<Vec<EmailSummary>, EmailError> {
            Ok(self.emails.iter().take(count as usize).cloned().collect())
        }

        async fn get_unread_count(&self) -> Result<u32, EmailError> {
            Ok(self.unread_count.load(Ordering::Relaxed))
        }

        async fn mark_read(&self, _email_id: &str) -> Result<(), EmailError> {
            Ok(())
        }

        async fn mark_unread(&self, _email_id: &str) -> Result<(), EmailError> {
            Ok(())
        }

        async fn delete(&self, _email_id: &str) -> Result<(), EmailError> {
            Ok(())
        }

        async fn send_email(&self, _draft: &EmailDraft) -> Result<String, EmailError> {
            Ok("msg-123".to_string())
        }

        async fn is_available(&self) -> bool {
            true
        }

        async fn list_mailboxes(&self) -> Result<Vec<String>, EmailError> {
            Ok(vec!["INBOX".to_string(), "Sent".to_string()])
        }
    }

    struct MockInference;

    #[async_trait]
    impl InferencePort for MockInference {
        async fn generate(
            &self,
            _prompt: &str,
        ) -> Result<crate::ports::InferenceResult, ApplicationError> {
            Ok(crate::ports::InferenceResult {
                content: "Test summary response".to_string(),
                model: "test-model".to_string(),
                tokens_used: Some(10),
                latency_ms: 100,
            })
        }

        async fn generate_with_context(
            &self,
            _conversation: &domain::Conversation,
        ) -> Result<crate::ports::InferenceResult, ApplicationError> {
            self.generate("").await
        }

        async fn generate_with_system(
            &self,
            _system_prompt: &str,
            _message: &str,
        ) -> Result<crate::ports::InferenceResult, ApplicationError> {
            self.generate("").await
        }

        async fn generate_stream(
            &self,
            _message: &str,
        ) -> Result<crate::ports::InferenceStream, ApplicationError> {
            Ok(Box::pin(futures::stream::empty()))
        }

        async fn generate_stream_with_system(
            &self,
            _system_prompt: &str,
            _message: &str,
        ) -> Result<crate::ports::InferenceStream, ApplicationError> {
            Ok(Box::pin(futures::stream::empty()))
        }

        async fn is_healthy(&self) -> bool {
            true
        }

        #[allow(clippy::unnecessary_literal_bound)]
        fn current_model(&self) -> &str {
            "test-model"
        }

        async fn list_available_models(&self) -> Result<Vec<String>, ApplicationError> {
            Ok(vec!["test-model".to_string()])
        }
    }

    #[test]
    fn email_service_creation() {
        let email_port = Arc::new(MockEmailPort::new(vec![], 0));
        let inference = Arc::new(MockInference);
        let service = EmailService::new(email_port, inference);
        assert!(format!("{service:?}").contains("EmailService"));
    }

    #[tokio::test]
    async fn get_inbox_summary_empty() {
        let email_port = Arc::new(MockEmailPort::new(vec![], 0));
        let inference = Arc::new(MockInference);
        let service = EmailService::new(email_port, inference);

        let summary = service.get_inbox_summary(10, false).await.unwrap();
        assert_eq!(summary.total_count, 0);
        assert_eq!(summary.unread_count, 0);
    }

    #[tokio::test]
    async fn get_inbox_summary_with_emails() {
        let emails = vec![
            EmailSummary::new("1", "alice@example.com", "Hello"),
            EmailSummary::new("2", "bob@example.com", "Hi there"),
        ];
        let email_port = Arc::new(MockEmailPort::new(emails, 1));
        let inference = Arc::new(MockInference);
        let service = EmailService::new(email_port, inference);

        let summary = service.get_inbox_summary(10, false).await.unwrap();
        assert_eq!(summary.total_count, 2);
        assert_eq!(summary.unread_count, 1);
    }

    #[tokio::test]
    async fn get_inbox_summary_only_important() {
        let emails = vec![
            EmailSummary::new("1", "alice@example.com", "Normal"),
            EmailSummary::new("2", "bob@example.com", "Important").with_is_starred(true),
        ];
        let email_port = Arc::new(MockEmailPort::new(emails, 1));
        let inference = Arc::new(MockInference);
        let service = EmailService::new(email_port, inference);

        let summary = service.get_inbox_summary(10, true).await.unwrap();
        assert_eq!(summary.total_count, 1);
        assert_eq!(summary.emails[0].subject, "Important");
    }

    #[tokio::test]
    async fn summarize_inbox_empty() {
        let email_port = Arc::new(MockEmailPort::new(vec![], 0));
        let inference = Arc::new(MockInference);
        let service = EmailService::new(email_port, inference);

        let result = service.summarize_inbox(10, false).await.unwrap();
        assert!(result.contains("Keine E-Mails"));
    }

    #[tokio::test]
    async fn summarize_inbox_with_emails() {
        let emails = vec![EmailSummary::new("1", "alice@example.com", "Test")];
        let email_port = Arc::new(MockEmailPort::new(emails, 1));
        let inference = Arc::new(MockInference);
        let service = EmailService::new(email_port, inference);

        let result = service.summarize_inbox(10, false).await.unwrap();
        assert!(result.contains("Inbox-Zusammenfassung"));
    }

    #[tokio::test]
    async fn send_email_returns_message_id() {
        let email_port = Arc::new(MockEmailPort::new(vec![], 0));
        let inference = Arc::new(MockInference);
        let service = EmailService::new(email_port, inference);

        let draft = EmailDraft::new("to@example.com", "Subject", "Body");
        let result = service.send_email(&draft).await.unwrap();
        assert_eq!(result, "msg-123");
    }

    #[tokio::test]
    async fn is_available_returns_true() {
        let email_port = Arc::new(MockEmailPort::new(vec![], 0));
        let inference = Arc::new(MockInference);
        let service = EmailService::new(email_port, inference);

        assert!(service.is_available().await);
    }

    #[tokio::test]
    async fn list_mailboxes_returns_list() {
        let email_port = Arc::new(MockEmailPort::new(vec![], 0));
        let inference = Arc::new(MockInference);
        let service = EmailService::new(email_port, inference);

        let mailboxes = service.list_mailboxes().await.unwrap();
        assert!(mailboxes.contains(&"INBOX".to_string()));
    }

    #[test]
    fn parse_draft_response_with_format() {
        let response = "BETREFF: Test Subject\n\nTEXT: Hello, this is the body.";
        let (subject, body) = parse_draft_response(response);
        assert_eq!(subject, "Test Subject");
        assert_eq!(body, "Hello, this is the body.");
    }

    #[test]
    fn parse_draft_response_fallback() {
        let response = "Subject Line\nBody line 1\nBody line 2";
        let (subject, body) = parse_draft_response(response);
        assert_eq!(subject, "Subject Line");
        assert!(body.contains("Body line 1"));
    }

    #[test]
    fn parse_draft_response_single_line() {
        let response = "Just some text";
        let (subject, body) = parse_draft_response(response);
        assert_eq!(subject, "(Kein Betreff)");
        assert_eq!(body, "Just some text");
    }

    #[test]
    fn inbox_summary_has_debug() {
        let summary = InboxSummary {
            total_count: 5,
            unread_count: 2,
            emails: vec![],
        };
        assert!(format!("{summary:?}").contains("InboxSummary"));
    }

    #[test]
    fn map_error_service_unavailable() {
        let err = map_error(EmailError::ServiceUnavailable);
        assert!(matches!(err, ApplicationError::ExternalService(_)));
    }

    #[test]
    fn map_error_auth_failed() {
        let err = map_error(EmailError::AuthenticationFailed);
        assert!(matches!(err, ApplicationError::NotAuthorized(_)));
    }

    #[test]
    fn map_error_not_found() {
        let err = map_error(EmailError::NotFound("123".to_string()));
        assert!(matches!(err, ApplicationError::ExternalService(_)));
    }

    #[test]
    fn map_error_invalid_address() {
        let err = map_error(EmailError::InvalidAddress("bad".to_string()));
        assert!(matches!(err, ApplicationError::CommandFailed(_)));
    }
}
