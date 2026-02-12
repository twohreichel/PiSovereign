//! Integration tests for persistence layer using in-memory SQLite databases
//!
//! These tests verify the actual persistence stores used by the application.

#![allow(clippy::expect_used, clippy::cast_possible_truncation, unused_imports)]

use application::ports::{
    ApprovalQueuePort, AuditLogPort, AuditQuery, ConversationStore, DraftStorePort,
    UserProfileStore,
};
use chrono::{Duration, Utc};
use domain::{
    AgentCommand, ApprovalRequest, ApprovalStatus, AuditEntry, AuditEventType, ChatMessage,
    Conversation, ConversationId, DraftId, EmailAddress, PersistedEmailDraft, UserId, UserProfile,
};
use infrastructure::persistence::{
    AsyncConversationStore, AsyncDatabase, SqliteApprovalQueue, SqliteAuditLog, SqliteDraftStore,
    SqliteUserProfileStore,
};

// ============================================================================
// Test Helpers
// ============================================================================

async fn create_test_db() -> AsyncDatabase {
    let db = AsyncDatabase::in_memory()
        .await
        .expect("Failed to create in-memory database");
    db.migrate().await.expect("Failed to run migrations");
    db
}

fn create_test_conversation() -> Conversation {
    let mut conv = Conversation::new();
    conv.title = Some("Test Conversation".to_string());
    conv.system_prompt = Some("You are a helpful assistant.".to_string());
    conv.add_message(ChatMessage::user("Hello"));
    conv.add_message(ChatMessage::assistant("Hi there!"));
    conv
}

// ============================================================================
// Conversation Store Tests
// ============================================================================

mod conversation_store_tests {
    use super::*;

    #[tokio::test]
    async fn test_save_and_get_conversation() {
        let db = create_test_db().await;
        let store = AsyncConversationStore::new(db.pool().clone());
        let conversation = create_test_conversation();
        let id = conversation.id;

        store.save(&conversation).await.expect("Failed to save");

        let retrieved = store.get(&id).await.expect("Failed to get");
        assert!(retrieved.is_some());

        let retrieved = retrieved.unwrap();
        assert_eq!(retrieved.id, id);
        assert_eq!(retrieved.title, Some("Test Conversation".to_string()));
        assert_eq!(retrieved.messages.len(), 2);
    }

    #[tokio::test]
    async fn test_get_nonexistent_conversation() {
        let db = create_test_db().await;
        let store = AsyncConversationStore::new(db.pool().clone());

        let result = store
            .get(&ConversationId::new())
            .await
            .expect("Failed to query");
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_update_conversation() {
        let db = create_test_db().await;
        let store = AsyncConversationStore::new(db.pool().clone());
        let mut conversation = create_test_conversation();
        let id = conversation.id;

        store.save(&conversation).await.expect("Failed to save");

        conversation.title = Some("Updated Title".to_string());
        store.update(&conversation).await.expect("Failed to update");

        let retrieved = store.get(&id).await.expect("Failed to get").unwrap();
        assert_eq!(retrieved.title, Some("Updated Title".to_string()));
    }

    #[tokio::test]
    async fn test_delete_conversation() {
        let db = create_test_db().await;
        let store = AsyncConversationStore::new(db.pool().clone());
        let conversation = create_test_conversation();
        let id = conversation.id;

        store.save(&conversation).await.expect("Failed to save");
        store.delete(&id).await.expect("Failed to delete");

        let result = store.get(&id).await.expect("Failed to query");
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_list_recent_conversations() {
        let db = create_test_db().await;
        let store = AsyncConversationStore::new(db.pool().clone());

        for _ in 0..5 {
            let conv = create_test_conversation();
            store.save(&conv).await.expect("Failed to save");
        }

        let list = store.list_recent(10).await.expect("Failed to list");
        assert_eq!(list.len(), 5);
    }

    #[tokio::test]
    async fn test_list_recent_limit() {
        let db = create_test_db().await;
        let store = AsyncConversationStore::new(db.pool().clone());

        for _ in 0..10 {
            let conv = create_test_conversation();
            store.save(&conv).await.expect("Failed to save");
        }

        let list = store.list_recent(3).await.expect("Failed to list");
        assert_eq!(list.len(), 3);
    }

    #[tokio::test]
    async fn test_message_ordering() {
        let db = create_test_db().await;
        let store = AsyncConversationStore::new(db.pool().clone());

        let mut conversation = Conversation::new();

        for i in 0..5 {
            if i % 2 == 0 {
                conversation.add_message(ChatMessage::user(format!("Message {i}")));
            } else {
                conversation.add_message(ChatMessage::assistant(format!("Message {i}")));
            }
        }

        store.save(&conversation).await.expect("Failed to save");

        let retrieved = store
            .get(&conversation.id)
            .await
            .expect("Failed to get")
            .unwrap();

        for (i, msg) in retrieved.messages.iter().enumerate() {
            assert!(msg.content.contains(&i.to_string()));
        }
    }

    #[tokio::test]
    async fn test_add_messages() {
        let db = create_test_db().await;
        let store = AsyncConversationStore::new(db.pool().clone());

        let conversation = create_test_conversation();
        let id = conversation.id;
        let current_count = conversation.messages.len();

        store.save(&conversation).await.expect("Failed to save");

        let new_messages = vec![
            ChatMessage::user("New message 1").with_sequence_number((current_count + 1) as u32),
            ChatMessage::assistant("New response 1")
                .with_sequence_number((current_count + 2) as u32),
        ];

        let added = store
            .add_messages(&id, &new_messages)
            .await
            .expect("Failed to add messages");
        assert_eq!(added, 2);

        let retrieved = store.get(&id).await.expect("Failed to get").unwrap();
        assert_eq!(retrieved.messages.len(), 4);
    }
}

// ============================================================================
// Audit Log Tests
// ============================================================================

mod audit_log_tests {
    use super::*;
    use uuid::Uuid;

    fn create_test_audit_entry(event_type: AuditEventType, action: &str) -> AuditEntry {
        AuditEntry {
            id: None,
            timestamp: Utc::now(),
            event_type,
            actor: Some("test-user".to_string()),
            resource_type: Some("conversation".to_string()),
            resource_id: Some(Uuid::new_v4().to_string()),
            action: action.to_string(),
            details: Some("Test details".to_string()),
            ip_address: None,
            success: true,
            request_id: Some(Uuid::new_v4()),
        }
    }

    #[tokio::test]
    async fn test_audit_log_insert_and_query() {
        let db = create_test_db().await;
        let log = SqliteAuditLog::new(db.pool().clone());

        let entry = create_test_audit_entry(AuditEventType::DataAccess, "read");
        log.log(&entry).await.expect("Failed to log event");

        let query = AuditQuery::new().with_event_type(AuditEventType::DataAccess);
        let events = log.query(&query).await.expect("Failed to get events");
        assert!(!events.is_empty());

        let event = &events[0];
        assert_eq!(event.event_type, AuditEventType::DataAccess);
        assert_eq!(event.actor, Some("test-user".to_string()));
    }

    #[tokio::test]
    async fn test_audit_log_filter_by_type() {
        let db = create_test_db().await;
        let log = SqliteAuditLog::new(db.pool().clone());

        log.log(&create_test_audit_entry(
            AuditEventType::Authentication,
            "login",
        ))
        .await
        .unwrap();
        log.log(&create_test_audit_entry(AuditEventType::DataAccess, "read"))
            .await
            .unwrap();
        log.log(&create_test_audit_entry(
            AuditEventType::Authentication,
            "logout",
        ))
        .await
        .unwrap();
        log.log(&create_test_audit_entry(AuditEventType::System, "startup"))
            .await
            .unwrap();

        let query = AuditQuery::new().with_event_type(AuditEventType::Authentication);
        let login_events = log.query(&query).await.expect("Failed to query");
        assert_eq!(login_events.len(), 2);

        let query = AuditQuery::new().with_event_type(AuditEventType::DataAccess);
        let data_events = log.query(&query).await.expect("Failed to query");
        assert_eq!(data_events.len(), 1);
    }

    #[tokio::test]
    async fn test_audit_log_filter_by_actor() {
        let db = create_test_db().await;
        let log = SqliteAuditLog::new(db.pool().clone());

        let mut entry_alice = create_test_audit_entry(AuditEventType::DataAccess, "read");
        entry_alice.actor = Some("alice".to_string());

        let mut entry_bob = create_test_audit_entry(AuditEventType::DataAccess, "read");
        entry_bob.actor = Some("bob".to_string());

        log.log(&entry_alice).await.unwrap();
        log.log(&entry_alice).await.unwrap();
        log.log(&entry_bob).await.unwrap();

        let query = AuditQuery::new().with_actor("alice");
        let alice_events = log.query(&query).await.expect("Failed to query");
        assert_eq!(alice_events.len(), 2);
    }

    #[tokio::test]
    async fn test_audit_log_time_range() {
        let db = create_test_db().await;
        let log = SqliteAuditLog::new(db.pool().clone());

        let entry = create_test_audit_entry(AuditEventType::DataAccess, "read");
        log.log(&entry).await.unwrap();

        let query = AuditQuery::new().with_time_range(
            Utc::now() - Duration::hours(1),
            Utc::now() + Duration::hours(1),
        );

        let events = log.query(&query).await.expect("Failed to query");
        assert!(!events.is_empty());
    }

    #[tokio::test]
    async fn test_audit_log_get_recent() {
        let db = create_test_db().await;
        let log = SqliteAuditLog::new(db.pool().clone());

        for i in 0..10 {
            let entry = create_test_audit_entry(AuditEventType::DataAccess, &format!("action_{i}"));
            log.log(&entry).await.unwrap();
        }

        let events = log.get_recent(5).await.expect("Failed to get recent");
        assert_eq!(events.len(), 5);
    }

    #[tokio::test]
    async fn test_audit_log_get_for_actor() {
        let db = create_test_db().await;
        let log = SqliteAuditLog::new(db.pool().clone());

        let mut entry = create_test_audit_entry(AuditEventType::DataAccess, "read");
        entry.actor = Some("specific-user".to_string());

        for _ in 0..3 {
            log.log(&entry).await.unwrap();
        }

        let events = log
            .get_for_actor("specific-user", 10)
            .await
            .expect("Failed to query");
        assert_eq!(events.len(), 3);
    }
}

// ============================================================================
// Approval Queue Tests
// ============================================================================

mod approval_queue_tests {
    use super::*;

    fn create_test_approval() -> ApprovalRequest {
        ApprovalRequest::new(
            UserId::new(),
            AgentCommand::SendEmail {
                draft_id: "test-draft-id".to_string(),
            },
            "Test approval request",
        )
    }

    #[tokio::test]
    async fn test_approval_enqueue_and_get() {
        let db = create_test_db().await;
        let queue = SqliteApprovalQueue::new(db.pool().clone());

        let request = create_test_approval();
        let id = request.id;

        queue.enqueue(&request).await.expect("Failed to enqueue");

        let result = queue.get(&id).await.expect("Failed to get");
        assert!(result.is_some());
        let result = result.unwrap();
        assert_eq!(result.status, ApprovalStatus::Pending);
    }

    #[tokio::test]
    async fn test_get_pending_for_user() {
        let db = create_test_db().await;
        let queue = SqliteApprovalQueue::new(db.pool().clone());

        let user_id = UserId::new();

        for _ in 0..2 {
            let request = ApprovalRequest::new(
                user_id,
                AgentCommand::SendEmail {
                    draft_id: "test-draft-id".to_string(),
                },
                "Test approval",
            );
            queue.enqueue(&request).await.unwrap();
        }

        queue.enqueue(&create_test_approval()).await.unwrap();

        let user_requests = queue
            .get_pending_for_user(&user_id)
            .await
            .expect("Failed to get");
        assert_eq!(user_requests.len(), 2);
    }

    #[tokio::test]
    async fn test_count_pending_for_user() {
        let db = create_test_db().await;
        let queue = SqliteApprovalQueue::new(db.pool().clone());

        let user_id = UserId::new();

        for _ in 0..3 {
            let request = ApprovalRequest::new(
                user_id,
                AgentCommand::SendEmail {
                    draft_id: "test-draft-id".to_string(),
                },
                "Test approval",
            );
            queue.enqueue(&request).await.unwrap();
        }

        let count = queue
            .count_pending_for_user(&user_id)
            .await
            .expect("Failed to count");
        assert_eq!(count, 3);
    }

    #[tokio::test]
    async fn test_approval_delete() {
        let db = create_test_db().await;
        let queue = SqliteApprovalQueue::new(db.pool().clone());

        let request = create_test_approval();
        let id = request.id;

        queue.enqueue(&request).await.expect("Failed to enqueue");
        queue.delete(&id).await.expect("Failed to delete");

        let result = queue.get(&id).await.expect("Failed to get");
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_approval_update() {
        let db = create_test_db().await;
        let queue = SqliteApprovalQueue::new(db.pool().clone());

        let mut request = create_test_approval();
        let id = request.id;

        queue.enqueue(&request).await.expect("Failed to enqueue");

        request.approve().expect("Failed to approve");
        queue.update(&request).await.expect("Failed to update");

        let result = queue.get(&id).await.expect("Failed to get").unwrap();
        assert_eq!(result.status, ApprovalStatus::Approved);
    }
}

// ============================================================================
// Draft Store Tests
// ============================================================================

mod draft_store_tests {
    use super::*;

    fn create_test_draft() -> PersistedEmailDraft {
        PersistedEmailDraft::new(
            UserId::new(),
            EmailAddress::new("recipient@example.com").unwrap(),
            "Test Subject",
            "Test body content",
        )
        .with_cc(EmailAddress::new("cc@example.com").unwrap())
    }

    #[tokio::test]
    async fn test_draft_save_and_get() {
        let db = create_test_db().await;
        let store = SqliteDraftStore::new(db.pool().clone());

        let draft = create_test_draft();
        let id = draft.id;

        store.save(&draft).await.expect("Failed to save");

        let result = store.get(&id).await.expect("Failed to get").unwrap();
        assert_eq!(
            result.to,
            EmailAddress::new("recipient@example.com").unwrap()
        );
        assert_eq!(result.subject, "Test Subject");
        assert_eq!(result.body, "Test body content");
        assert_eq!(result.cc.len(), 1);
    }

    #[tokio::test]
    async fn test_draft_deletion() {
        let db = create_test_db().await;
        let store = SqliteDraftStore::new(db.pool().clone());

        let draft = create_test_draft();
        let id = draft.id;

        store.save(&draft).await.expect("Failed to save");
        let deleted = store.delete(&id).await.expect("Failed to delete");
        assert!(deleted);

        let result = store.get(&id).await.expect("Failed to query");
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_list_for_user() {
        let db = create_test_db().await;
        let store = SqliteDraftStore::new(db.pool().clone());

        let user_id = UserId::new();

        for _ in 0..3 {
            let draft = PersistedEmailDraft::new(
                user_id,
                EmailAddress::new("test@example.com").unwrap(),
                "Test Subject",
                "Test body",
            );
            store.save(&draft).await.unwrap();
        }

        let drafts = store
            .list_for_user(&user_id, 10)
            .await
            .expect("Failed to list");
        assert_eq!(drafts.len(), 3);
    }

    #[tokio::test]
    async fn test_expired_draft_not_returned() {
        let db = create_test_db().await;
        let store = SqliteDraftStore::new(db.pool().clone());

        let mut draft = create_test_draft();
        draft.expires_at = Utc::now() - Duration::hours(1);
        let id = draft.id;

        store.save(&draft).await.expect("Failed to save");

        let result = store.get(&id).await.expect("Failed to query");
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_get_for_user() {
        let db = create_test_db().await;
        let store = SqliteDraftStore::new(db.pool().clone());

        let user_id = UserId::new();
        let other_user_id = UserId::new();

        let draft = PersistedEmailDraft::new(
            user_id,
            EmailAddress::new("test@example.com").unwrap(),
            "Test Subject",
            "Test body",
        );
        let id = draft.id;

        store.save(&draft).await.expect("Failed to save");

        let result = store
            .get_for_user(&id, &user_id)
            .await
            .expect("Failed to get");
        assert!(result.is_some());

        let result = store
            .get_for_user(&id, &other_user_id)
            .await
            .expect("Failed to get");
        assert!(result.is_none());
    }
}

// ============================================================================
// User Profile Store Tests
// ============================================================================

mod user_profile_tests {
    use super::*;

    fn create_test_profile() -> UserProfile {
        UserProfile::new(UserId::new())
    }

    #[tokio::test]
    async fn test_user_profile_save_and_get() {
        let db = create_test_db().await;
        let store = SqliteUserProfileStore::new(db.pool().clone());

        let profile = create_test_profile();
        let id = profile.id();

        store.save(&profile).await.expect("Failed to save");

        let result = store.get(&id).await.expect("Failed to get").unwrap();
        assert_eq!(result.id(), id);
    }

    #[tokio::test]
    async fn test_user_profile_delete() {
        let db = create_test_db().await;
        let store = SqliteUserProfileStore::new(db.pool().clone());

        let profile = create_test_profile();
        let id = profile.id();

        store.save(&profile).await.expect("Failed to save");
        store.delete(&id).await.expect("Failed to delete");

        let result = store.get(&id).await.expect("Failed to query");
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_user_profile_with_location() {
        let db = create_test_db().await;
        let store = SqliteUserProfileStore::new(db.pool().clone());

        let user_id = UserId::new();
        let location = domain::GeoLocation::new(52.52, 13.405).unwrap();
        let timezone = domain::Timezone::try_new("Europe/Berlin").unwrap();
        let profile = UserProfile::with_defaults(user_id, location, timezone);

        store.save(&profile).await.expect("Failed to save");

        let result = store.get(&user_id).await.expect("Failed to get").unwrap();
        assert!(result.location().is_some());
        let loc = result.location().unwrap();
        assert!((loc.latitude() - 52.52).abs() < 0.01);
        assert!((loc.longitude() - 13.405).abs() < 0.01);
    }
}

// ============================================================================
// Concurrent Access Tests
// ============================================================================

mod concurrent_access_tests {
    use std::sync::Arc;

    use tokio::sync::Barrier;

    use super::*;

    #[tokio::test]
    async fn test_concurrent_writes() {
        let db = create_test_db().await;
        let store = Arc::new(AsyncConversationStore::new(db.pool().clone()));

        let barrier = Arc::new(Barrier::new(5));
        let mut handles = vec![];

        for i in 0..5 {
            let store = Arc::clone(&store);
            let barrier = Arc::clone(&barrier);

            handles.push(tokio::spawn(async move {
                barrier.wait().await;

                let mut conv = Conversation::new();
                conv.title = Some(format!("Conversation {i}"));

                store.save(&conv).await.expect("Failed to save");
                conv.id
            }));
        }

        let mut ids = vec![];
        for handle in handles {
            ids.push(handle.await.expect("Task panicked"));
        }

        for id in ids {
            let conv = store.get(&id).await.expect("Failed to get");
            assert!(conv.is_some());
        }
    }
}
