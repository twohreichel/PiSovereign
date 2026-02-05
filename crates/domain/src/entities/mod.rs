//! Domain entities - Objects with identity and lifecycle

mod approval_request;
mod audit_entry;
mod briefing;
mod chat_message;
mod conversation;
mod user_profile;

pub use approval_request::{ApprovalError, ApprovalRequest, ApprovalStatus};
pub use audit_entry::{AuditBuilder, AuditEntry, AuditEventType};
pub use briefing::{
    CalendarBrief, CalendarItem, EmailBrief, MorningBriefing, TaskBrief, TaskItem, WeatherSummary,
};
pub use chat_message::{ChatMessage, MessageMetadata, MessageRole};
pub use conversation::Conversation;
pub use user_profile::UserProfile;
