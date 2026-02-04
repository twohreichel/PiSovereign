//! Persistence module
//!
//! SQLite-based storage for conversations, approvals, and audit logs.

pub mod connection;
pub mod conversation_store;
pub mod migrations;

pub use connection::{ConnectionPool, DatabaseError, create_pool};
pub use conversation_store::SqliteConversationStore;
