//! Persistence module
//!
//! SQLite-based storage for conversations, approvals, and audit logs.

pub mod approval_queue;
pub mod audit_log;
pub mod connection;
pub mod conversation_store;
pub mod migrations;

pub use approval_queue::SqliteApprovalQueue;
pub use audit_log::SqliteAuditLog;
pub use connection::{ConnectionPool, DatabaseError, create_pool};
pub use conversation_store::SqliteConversationStore;
