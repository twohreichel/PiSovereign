//! Domain entities - Objects with identity and lifecycle

mod approval_request;
mod audit_entry;
mod briefing;
mod chat_message;
mod conversation;
mod email_draft;
mod memory;
mod prompt_security;
mod reminder;
mod user_profile;
mod voice_message;
mod web_search;

pub use approval_request::{ApprovalError, ApprovalRequest, ApprovalStatus};
pub use audit_entry::{AuditBuilder, AuditEntry, AuditEventType};
pub use briefing::{
    CalendarBrief, CalendarItem, EmailBrief, MorningBriefing, TaskBrief, TaskItem, WeatherSummary,
};
pub use chat_message::{ChatMessage, MessageMetadata, MessageRole};
pub use conversation::{Conversation, ConversationSource};
pub use email_draft::{DEFAULT_DRAFT_TTL_DAYS, PersistedEmailDraft};
pub use memory::{Memory, MemoryQuery, MemoryType, cosine_similarity};
pub use prompt_security::{PromptAnalysisResult, SecurityThreat, ThreatCategory, ThreatLevel};
pub use reminder::{Reminder, ReminderSource, ReminderStatus};
pub use user_profile::UserProfile;
pub use voice_message::{AudioFormat, VoiceMessage, VoiceMessageSource, VoiceMessageStatus};
pub use web_search::{SearchResult, WebSearchResponse};
