//! Persistence module
//!
//! SQLite-based storage using sqlx for async database operations.
//! All stores use `SqlitePool` from the shared `AsyncDatabase` connection.

pub mod approval_queue;
pub mod async_connection;
pub mod async_conversation_store;
pub mod audit_log;
pub mod database_health;
pub mod draft_store;
pub mod error;
pub mod memory_store;
pub mod reminder_store;
pub mod retry_queue;
pub mod user_profile_store;

pub use approval_queue::SqliteApprovalQueue;
pub use async_connection::{AsyncDatabase, AsyncDatabaseConfig, AsyncDatabaseError};
pub use async_conversation_store::AsyncConversationStore;
pub use audit_log::SqliteAuditLog;
pub use database_health::SqliteDatabaseHealth;
pub use draft_store::SqliteDraftStore;
pub use memory_store::SqliteMemoryStore;
pub use reminder_store::SqliteReminderStore;
pub use retry_queue::{
    DeadLetterItem, QueueStats, RetryItem, RetryQueueError, RetryQueueStore, RetryStatus,
};
pub use user_profile_store::SqliteUserProfileStore;
