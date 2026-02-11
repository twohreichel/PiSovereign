//! Application services - Use case implementations

mod agent_service;
mod approval_service;
mod briefing_service;
mod calendar_service;
mod chat_service;
mod conversation_context;
mod email_service;
mod health_service;
pub mod location_helper;
mod memory_enhanced_chat;
mod memory_service;
pub mod notification_service;
mod prompt_sanitizer;
pub mod reminder_formatter;
mod reminder_service;
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
pub use location_helper::{
    format_location_with_coords_link, format_location_with_link, generate_maps_link,
    generate_maps_link_coords,
};
pub use memory_enhanced_chat::{MemoryEnhancedChat, MemoryEnhancedChatConfig};
pub use memory_service::{MemoryService, MemoryServiceConfig};
pub use notification_service::{NotificationConfig, NotificationService, ReminderNotification};
pub use prompt_sanitizer::{PromptSanitizer, PromptSecurityConfig, SecuritySensitivity};
pub use reminder_formatter::{
    BriefingEvent, MorningBriefingData, format_acknowledge_confirmation,
    format_calendar_event_reminder, format_calendar_task_reminder, format_custom_reminder,
    format_morning_briefing, format_reminder, format_reminder_list, format_snooze_confirmation,
};
pub use reminder_service::{ReminderService, ReminderServiceConfig};
pub use voice_message_service::{VoiceMessageConfig, VoiceMessageResult, VoiceMessageService};
