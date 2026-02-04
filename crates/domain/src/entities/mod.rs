//! Domain entities - Objects with identity and lifecycle

mod approval_request;
mod audit_entry;
mod chat_message;
mod conversation;

pub use approval_request::{ApprovalError, ApprovalRequest, ApprovalStatus};
pub use audit_entry::{AuditBuilder, AuditEntry, AuditEventType};
pub use chat_message::{ChatMessage, MessageMetadata, MessageRole};
pub use conversation::Conversation;
