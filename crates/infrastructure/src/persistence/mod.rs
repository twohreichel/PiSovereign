//! Persistence module
//!
//! SQLite-based storage for conversations, approvals, and audit logs.
//!
//! This module provides two implementations:
//! - Blocking (r2d2/rusqlite): Legacy implementation using spawn_blocking
//! - Async (sqlx): Preferred implementation with true async database operations

pub mod approval_queue;
pub mod async_connection;
pub mod async_conversation_store;
pub mod audit_log;
pub mod connection;
pub mod conversation_store;
pub mod database_health;
pub mod draft_store;
pub mod migrations;
pub mod retry_queue;
pub mod user_profile_store;

pub use approval_queue::SqliteApprovalQueue;
pub use async_connection::{AsyncDatabase, AsyncDatabaseConfig, AsyncDatabaseError};
pub use async_conversation_store::AsyncConversationStore;
pub use audit_log::SqliteAuditLog;
pub use connection::{ConnectionPool, DatabaseError, create_pool};
pub use conversation_store::SqliteConversationStore;
pub use database_health::SqliteDatabaseHealth;
pub use draft_store::SqliteDraftStore;
pub use retry_queue::{DeadLetterItem, QueueStats, RetryItem, RetryQueueError, RetryQueueStore, RetryStatus};
pub use user_profile_store::SqliteUserProfileStore;
