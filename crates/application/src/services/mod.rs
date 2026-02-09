//! Application services - Use case implementations

mod agent_service;
mod approval_service;
mod briefing_service;
mod calendar_service;
mod chat_service;
mod conversation_context;
mod email_service;
mod health_service;
mod memory_service;
mod prompt_sanitizer;
mod voice_message_service;

pub use agent_service::{AgentService, ApprovalStatus, CommandResult, ExecutionResult};
pub use approval_service::ApprovalService;
pub use briefing_service::{
    BriefingService, CalendarBrief, EmailBrief, EmailHighlight, EventSummary, MorningBriefing,
    TaskBrief, WeatherSummary,
};
pub use calendar_service::CalendarService;
pub use chat_service::{ChatService, MAX_CONVERSATION_MESSAGES};
pub use conversation_context::{
    ConversationCacheStats, ConversationContextConfig, ConversationContextService,
};
pub use email_service::{EmailService, InboxSummary};
pub use health_service::{HealthConfig, HealthReport, HealthService, ServiceHealth};
pub use memory_service::{MemoryService, MemoryServiceConfig};
pub use prompt_sanitizer::{PromptSanitizer, PromptSecurityConfig, SecuritySensitivity};
pub use voice_message_service::{VoiceMessageConfig, VoiceMessageResult, VoiceMessageService};
