//! Email-related handlers: inbox summarization and draft creation

use domain::{EmailAddress, PersistedEmailDraft, UserId};
use tracing::{info, warn};

use super::{AgentService, ExecutionResult};
use crate::error::ApplicationError;

impl AgentService {
    /// Handle inbox summarization command
    pub(super) async fn handle_summarize_inbox(
        &self,
        count: Option<u32>,
        only_important: Option<bool>,
    ) -> Result<ExecutionResult, ApplicationError> {
        let email_count = count.unwrap_or(10);
        let important_only = only_important.unwrap_or(false);

        // Use email service if available
        if let Some(ref email_svc) = self.email_service {
            match email_svc.summarize_inbox(email_count, important_only).await {
                Ok(summary) => {
                    return Ok(ExecutionResult {
                        success: true,
                        response: summary,
                    });
                },
                Err(e) => {
                    warn!(error = %e, "Failed to summarize inbox, falling back to placeholder");
                },
            }
        }

        // Fallback when service not available
        let filter_msg = if important_only {
            ", important only"
        } else {
            ""
        };
        Ok(ExecutionResult {
            success: true,
            response: format!(
                "📧 Inbox summary (last {email_count} emails{filter_msg}):\n\n\
                 (Email integration not configured. Please set up Proton Bridge.)"
            ),
        })
    }

    /// Handle draft email command - create and store the draft
    ///
    /// For now uses a default user ID. Future versions will map API keys to users.
    pub(super) async fn handle_draft_email(
        &self,
        to: &EmailAddress,
        subject: Option<&str>,
        body: &str,
    ) -> Result<ExecutionResult, ApplicationError> {
        let user_id = UserId::default();

        // Generate subject if not provided
        let subject = subject.map_or_else(|| format!("Re: {}", to.local_part()), String::from);

        // Check if draft store is configured
        let Some(ref draft_store) = self.draft_store else {
            return Ok(ExecutionResult {
                success: false,
                response: "📧 Email draft creation failed:\n\n\
                          Draft storage is not configured. Please set up database persistence."
                    .to_string(),
            });
        };

        // Create and save the draft
        let draft =
            PersistedEmailDraft::new(user_id, to.clone(), subject.clone(), body.to_string());
        let draft_id = draft.id;

        draft_store.save(&draft).await?;

        info!(draft_id = %draft_id, to = %to, subject = %subject, "Created email draft");

        Ok(ExecutionResult {
            success: true,
            response: format!(
                "📝 Email draft created:\n\n\
                 **To:** {to}\n\
                 **Subject:** {subject}\n\n\
                 Draft ID: `{draft_id}`\n\n\
                 To send this email, say 'send email {draft_id}' or 'approve send'."
            ),
        })
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use domain::{AgentCommand, EmailAddress};

    use super::super::{AgentService, test_support::MockInferenceEngine};

    fn email(addr: &str) -> EmailAddress {
        EmailAddress::new(addr).unwrap()
    }

    #[tokio::test]
    async fn execute_summarize_inbox() {
        let mock = MockInferenceEngine::new();
        let service = AgentService::new(Arc::new(mock));

        let result = service
            .execute_command(&AgentCommand::SummarizeInbox {
                count: Some(5),
                only_important: Some(true),
            })
            .await
            .unwrap();

        assert!(result.success);
        assert!(result.response.contains("Inbox"));
        assert!(result.response.contains('5'));
    }

    #[tokio::test]
    async fn execute_summarize_inbox_defaults() {
        let mock = MockInferenceEngine::new();
        let service = AgentService::new(Arc::new(mock));

        let result = service
            .execute_command(&AgentCommand::SummarizeInbox {
                count: None,
                only_important: None,
            })
            .await
            .unwrap();

        assert!(result.success);
        assert!(result.response.contains("10"));
    }

    #[tokio::test]
    async fn execute_draft_email_without_store_fallback() {
        let mock = MockInferenceEngine::new();
        let service = AgentService::new(Arc::new(mock));

        let result = service
            .execute_command(&AgentCommand::DraftEmail {
                to: domain::EmailAddress::new("test@example.com").unwrap(),
                subject: Some("Test".to_string()),
                body: "Body content".to_string(),
            })
            .await;

        assert!(result.is_ok());
        let result = result.unwrap();
        assert!(!result.success);
        assert!(result.response.contains("not configured"));
    }

    #[tokio::test]
    async fn draft_email_without_store_returns_error() {
        let mock = MockInferenceEngine::new();
        let service = AgentService::new(Arc::new(mock));

        let result = service
            .handle_draft_email(
                &email("test@example.com"),
                Some("Test Subject"),
                "Test body",
            )
            .await
            .unwrap();

        assert!(!result.success);
        assert!(result.response.contains("not configured"));
    }

    #[tokio::test]
    async fn draft_email_with_store_creates_draft() {
        use crate::ports::MockDraftStorePort;

        let mock_inference = MockInferenceEngine::new();
        let mut mock_store = MockDraftStorePort::new();

        mock_store.expect_save().returning(|draft| Ok(draft.id));

        let service =
            AgentService::new(Arc::new(mock_inference)).with_draft_store(Arc::new(mock_store));

        let result = service
            .handle_draft_email(
                &email("recipient@example.com"),
                Some("Test Subject"),
                "Email body content",
            )
            .await
            .unwrap();

        assert!(result.success);
        assert!(result.response.contains("Draft ID:"));
        assert!(result.response.contains("recipient@example.com"));
        assert!(result.response.contains("Test Subject"));
    }

    #[tokio::test]
    async fn draft_email_generates_subject_when_not_provided() {
        use crate::ports::MockDraftStorePort;

        let mock_inference = MockInferenceEngine::new();
        let mut mock_store = MockDraftStorePort::new();

        mock_store.expect_save().returning(|draft| Ok(draft.id));

        let service =
            AgentService::new(Arc::new(mock_inference)).with_draft_store(Arc::new(mock_store));

        let result = service
            .handle_draft_email(&email("john@example.com"), None, "Hello!")
            .await
            .unwrap();

        assert!(result.success);
        assert!(result.response.contains("Re: john"));
    }

    #[tokio::test]
    async fn agent_service_has_draft_store_in_debug() {
        use crate::ports::MockDraftStorePort;

        let mock_inference = MockInferenceEngine::new();
        let mock_store = MockDraftStorePort::new();

        let service =
            AgentService::new(Arc::new(mock_inference)).with_draft_store(Arc::new(mock_store));

        let debug = format!("{service:?}");
        assert!(debug.contains("has_draft_store"));
    }
}
