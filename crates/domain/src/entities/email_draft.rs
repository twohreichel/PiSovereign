//! Persisted email draft entity with metadata for storage

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};

use crate::value_objects::{DraftId, UserId};

/// Default TTL for email drafts (7 days)
pub const DEFAULT_DRAFT_TTL_DAYS: i64 = 7;

/// A persisted email draft with full metadata for storage
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersistedEmailDraft {
    /// Unique draft identifier
    pub id: DraftId,
    /// Owner of the draft
    pub user_id: UserId,
    /// Recipient email address
    pub to: String,
    /// CC recipients
    pub cc: Vec<String>,
    /// Email subject
    pub subject: String,
    /// Email body (plain text)
    pub body: String,
    /// When the draft was created
    pub created_at: DateTime<Utc>,
    /// When the draft expires (for automatic cleanup)
    pub expires_at: DateTime<Utc>,
}

impl PersistedEmailDraft {
    /// Create a new persisted draft with default TTL
    pub fn new(
        user_id: UserId,
        to: impl Into<String>,
        subject: impl Into<String>,
        body: impl Into<String>,
    ) -> Self {
        let now = Utc::now();
        Self {
            id: DraftId::new(),
            user_id,
            to: to.into(),
            cc: Vec::new(),
            subject: subject.into(),
            body: body.into(),
            created_at: now,
            expires_at: now + Duration::days(DEFAULT_DRAFT_TTL_DAYS),
        }
    }

    /// Create a new persisted draft with custom TTL
    pub fn with_ttl(
        user_id: UserId,
        to: impl Into<String>,
        subject: impl Into<String>,
        body: impl Into<String>,
        ttl: Duration,
    ) -> Self {
        let now = Utc::now();
        Self {
            id: DraftId::new(),
            user_id,
            to: to.into(),
            cc: Vec::new(),
            subject: subject.into(),
            body: body.into(),
            created_at: now,
            expires_at: now + ttl,
        }
    }

    /// Add a CC recipient
    #[must_use]
    pub fn with_cc(mut self, cc: impl Into<String>) -> Self {
        self.cc.push(cc.into());
        self
    }

    /// Add multiple CC recipients
    #[must_use]
    pub fn with_ccs(mut self, ccs: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.cc.extend(ccs.into_iter().map(Into::into));
        self
    }

    /// Check if the draft has expired
    pub fn is_expired(&self) -> bool {
        Utc::now() > self.expires_at
    }

    /// Get the remaining time until expiration
    pub fn time_until_expiration(&self) -> Option<Duration> {
        let remaining = self.expires_at - Utc::now();
        if remaining.num_seconds() > 0 {
            Some(remaining)
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_user_id() -> UserId {
        UserId::new()
    }

    #[test]
    fn new_draft_has_unique_id() {
        let draft1 = PersistedEmailDraft::new(test_user_id(), "a@b.com", "Subject", "Body");
        let draft2 = PersistedEmailDraft::new(test_user_id(), "a@b.com", "Subject", "Body");
        assert_ne!(draft1.id, draft2.id);
    }

    #[test]
    fn new_draft_has_default_ttl() {
        let draft = PersistedEmailDraft::new(test_user_id(), "a@b.com", "Subject", "Body");
        let expected_expires = draft.created_at + Duration::days(DEFAULT_DRAFT_TTL_DAYS);
        // Allow 1 second tolerance for test timing
        assert!((draft.expires_at - expected_expires).num_seconds().abs() <= 1);
    }

    #[test]
    fn custom_ttl_is_respected() {
        let ttl = Duration::hours(24);
        let draft =
            PersistedEmailDraft::with_ttl(test_user_id(), "a@b.com", "Subject", "Body", ttl);
        let expected_expires = draft.created_at + ttl;
        assert!((draft.expires_at - expected_expires).num_seconds().abs() <= 1);
    }

    #[test]
    fn with_cc_adds_recipient() {
        let draft = PersistedEmailDraft::new(test_user_id(), "a@b.com", "Subject", "Body")
            .with_cc("cc@b.com");
        assert_eq!(draft.cc, vec!["cc@b.com"]);
    }

    #[test]
    fn with_ccs_adds_multiple_recipients() {
        let draft = PersistedEmailDraft::new(test_user_id(), "a@b.com", "Subject", "Body")
            .with_ccs(["cc1@b.com", "cc2@b.com"]);
        assert_eq!(draft.cc, vec!["cc1@b.com", "cc2@b.com"]);
    }

    #[test]
    fn new_draft_is_not_expired() {
        let draft = PersistedEmailDraft::new(test_user_id(), "a@b.com", "Subject", "Body");
        assert!(!draft.is_expired());
    }

    #[test]
    fn expired_draft_is_detected() {
        let mut draft = PersistedEmailDraft::new(test_user_id(), "a@b.com", "Subject", "Body");
        draft.expires_at = Utc::now() - Duration::hours(1);
        assert!(draft.is_expired());
    }

    #[test]
    fn time_until_expiration_returns_some_for_valid_draft() {
        let draft = PersistedEmailDraft::new(test_user_id(), "a@b.com", "Subject", "Body");
        let remaining = draft.time_until_expiration();
        assert!(remaining.is_some());
        assert!(remaining.unwrap().num_days() >= 6); // Should be close to 7 days
    }

    #[test]
    fn time_until_expiration_returns_none_for_expired_draft() {
        let mut draft = PersistedEmailDraft::new(test_user_id(), "a@b.com", "Subject", "Body");
        draft.expires_at = Utc::now() - Duration::hours(1);
        assert!(draft.time_until_expiration().is_none());
    }

    #[test]
    fn draft_serialization_roundtrip() {
        let draft = PersistedEmailDraft::new(test_user_id(), "to@example.com", "Test", "Body")
            .with_cc("cc@example.com");
        let json = serde_json::to_string(&draft).unwrap();
        let parsed: PersistedEmailDraft = serde_json::from_str(&json).unwrap();
        assert_eq!(draft.id, parsed.id);
        assert_eq!(draft.to, parsed.to);
        assert_eq!(draft.cc, parsed.cc);
    }

    #[test]
    fn draft_debug_output() {
        let draft = PersistedEmailDraft::new(test_user_id(), "a@b.com", "Subject", "Body");
        let debug = format!("{draft:?}");
        assert!(debug.contains("PersistedEmailDraft"));
        assert!(debug.contains("a@b.com"));
    }
}
