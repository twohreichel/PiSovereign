//! Application services - Use case implementations

mod agent_service;
mod approval_service;
mod briefing_service;
mod chat_service;

pub use agent_service::{AgentService, ApprovalStatus, CommandResult, ExecutionResult};
pub use approval_service::ApprovalService;
pub use briefing_service::{
    BriefingService, CalendarBrief, EmailBrief, EmailHighlight, EventSummary, MorningBriefing,
    TaskBrief, WeatherSummary,
};
pub use chat_service::ChatService;
