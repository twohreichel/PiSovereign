//! Command handlers

use application::{ApprovalStatus, RequestContext};
use axum::{Extension, Json, extract::State};
use domain::AgentCommand;
use serde::{Deserialize, Serialize};
use tracing::instrument;
use utoipa::ToSchema;

use crate::{error::ApiError, state::AppState};

/// Command execution request
#[derive(Debug, Deserialize, ToSchema)]
#[schema(example = json!({"input": "What's on my calendar today?"}))]
pub struct ExecuteCommandRequest {
    /// Natural language input or explicit command
    pub input: String,
}

/// Command execution response
#[derive(Debug, Serialize, ToSchema)]
#[schema(example = json!({
    "success": true,
    "response": "You have 3 meetings today...",
    "command_type": "morning_briefing",
    "execution_time_ms": 245,
    "requires_approval": false
}))]
pub struct ExecuteCommandResponse {
    /// Whether the command was successful
    pub success: bool,
    /// Response message
    pub response: String,
    /// Parsed command type
    pub command_type: String,
    /// Execution time in milliseconds
    pub execution_time_ms: u64,
    /// Whether approval was required
    #[serde(skip_serializing_if = "Option::is_none")]
    pub requires_approval: Option<bool>,
}

/// Execute a command from natural language input
#[utoipa::path(
    post,
    path = "/v1/commands",
    tag = "commands",
    request_body = ExecuteCommandRequest,
    responses(
        (status = 200, description = "Command executed", body = ExecuteCommandResponse),
        (status = 400, description = "Invalid request", body = crate::error::ErrorResponse),
        (status = 429, description = "Rate limited", body = crate::error::ErrorResponse),
        (status = 503, description = "Service unavailable", body = crate::error::ErrorResponse)
    ),
    security(("api_key" = []))
)]
#[instrument(skip(state, ctx, request), fields(input_len = request.input.len()))]
pub async fn execute_command(
    State(state): State<AppState>,
    ctx: Option<Extension<RequestContext>>,
    Json(request): Json<ExecuteCommandRequest>,
) -> Result<Json<ExecuteCommandResponse>, ApiError> {
    if request.input.trim().is_empty() {
        return Err(ApiError::BadRequest("Input cannot be empty".to_string()));
    }

    // Extract user ID from request context for user-specific operations
    let user_id = ctx.map(|Extension(c)| c.user_id());

    let result = state
        .agent_service
        .handle_input_with_user(&request.input, user_id)
        .await?;

    Ok(Json(ExecuteCommandResponse {
        success: result.success,
        response: result.response,
        command_type: command_type_name(&result.command),
        execution_time_ms: result.execution_time_ms,
        requires_approval: result
            .approval_status
            .map(|s| matches!(s, ApprovalStatus::Pending)),
    }))
}

/// Parse command request
#[derive(Debug, Deserialize, ToSchema)]
#[schema(example = json!({"input": "Schedule a meeting tomorrow at 2pm"}))]
pub struct ParseCommandRequest {
    /// Natural language input to parse
    pub input: String,
}

/// Parse command response
#[derive(Debug, Serialize, ToSchema)]
pub struct ParseCommandResponse {
    /// Parsed command
    #[schema(value_type = crate::openapi::AgentCommandSchema)]
    pub command: AgentCommand,
    /// Whether this command requires approval
    pub requires_approval: bool,
    /// Human-readable description
    pub description: String,
}

/// Parse input into a command without executing
#[utoipa::path(
    post,
    path = "/v1/commands/parse",
    tag = "commands",
    request_body = ParseCommandRequest,
    responses(
        (status = 200, description = "Command parsed", body = ParseCommandResponse),
        (status = 400, description = "Invalid request", body = crate::error::ErrorResponse)
    ),
    security(("api_key" = []))
)]
#[instrument(skip(_state, request), fields(input_len = request.input.len()))]
pub async fn parse_command(
    State(_state): State<AppState>,
    Json(request): Json<ParseCommandRequest>,
) -> Result<Json<ParseCommandResponse>, ApiError> {
    if request.input.trim().is_empty() {
        return Err(ApiError::BadRequest("Input cannot be empty".to_string()));
    }

    // Use the command parser through the agent service
    // For now, we'll handle the parsing directly
    let parser = application::CommandParser::new();

    // Try quick parse first, fall back to Ask if nothing matches
    let command = parser
        .parse_quick(&request.input)
        .unwrap_or_else(|| AgentCommand::Ask {
            question: request.input.clone(),
        });

    Ok(Json(ParseCommandResponse {
        requires_approval: command.requires_approval(),
        description: command.description(),
        command,
    }))
}

/// Get the type name of a command
fn command_type_name(cmd: &AgentCommand) -> String {
    match cmd {
        AgentCommand::MorningBriefing { .. } => "morning_briefing",
        AgentCommand::CreateCalendarEvent { .. } => "create_calendar_event",
        AgentCommand::UpdateCalendarEvent { .. } => "update_calendar_event",
        AgentCommand::ListTasks { .. } => "list_tasks",
        AgentCommand::CreateTask { .. } => "create_task",
        AgentCommand::CompleteTask { .. } => "complete_task",
        AgentCommand::UpdateTask { .. } => "update_task",
        AgentCommand::DeleteTask { .. } => "delete_task",
        AgentCommand::ListTaskLists => "list_task_lists",
        AgentCommand::CreateTaskList { .. } => "create_task_list",
        AgentCommand::SummarizeInbox { .. } => "summarize_inbox",
        AgentCommand::DraftEmail { .. } => "draft_email",
        AgentCommand::SendEmail { .. } => "send_email",
        AgentCommand::Ask { .. } => "ask",
        AgentCommand::System(_) => "system",
        AgentCommand::Echo { .. } => "echo",
        AgentCommand::Help { .. } => "help",
        AgentCommand::WebSearch { .. } => "web_search",
        AgentCommand::CreateReminder { .. } => "create_reminder",
        AgentCommand::ListReminders { .. } => "list_reminders",
        AgentCommand::SnoozeReminder { .. } => "snooze_reminder",
        AgentCommand::AcknowledgeReminder { .. } => "acknowledge_reminder",
        AgentCommand::DeleteReminder { .. } => "delete_reminder",
        AgentCommand::SearchTransit { .. } => "search_transit",
        AgentCommand::Unknown { .. } => "unknown",
    }
    .to_string()
}

#[cfg(test)]
mod tests {
    use domain::SystemCommand;

    use super::*;

    #[test]
    fn execute_command_request_deserialize() {
        let json = r#"{"input": "hilfe"}"#;
        let request: ExecuteCommandRequest = serde_json::from_str(json).unwrap();
        assert_eq!(request.input, "hilfe");
    }

    #[test]
    fn execute_command_request_debug() {
        let request = ExecuteCommandRequest {
            input: "test".to_string(),
        };
        let debug = format!("{request:?}");
        assert!(debug.contains("ExecuteCommandRequest"));
    }

    #[test]
    fn execute_command_response_serialize() {
        let response = ExecuteCommandResponse {
            success: true,
            response: "Done".to_string(),
            command_type: "echo".to_string(),
            execution_time_ms: 50,
            requires_approval: None,
        };
        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("Done"));
        assert!(json.contains("echo"));
        assert!(!json.contains("requires_approval"));
    }

    #[test]
    fn execute_command_response_with_approval() {
        let response = ExecuteCommandResponse {
            success: false,
            response: "Pending".to_string(),
            command_type: "create_calendar_event".to_string(),
            execution_time_ms: 10,
            requires_approval: Some(true),
        };
        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("requires_approval"));
    }

    #[test]
    fn execute_command_response_debug() {
        let response = ExecuteCommandResponse {
            success: true,
            response: "OK".to_string(),
            command_type: "help".to_string(),
            execution_time_ms: 5,
            requires_approval: None,
        };
        let debug = format!("{response:?}");
        assert!(debug.contains("ExecuteCommandResponse"));
    }

    #[test]
    fn parse_command_request_deserialize() {
        let json = r#"{"input": "echo test"}"#;
        let request: ParseCommandRequest = serde_json::from_str(json).unwrap();
        assert_eq!(request.input, "echo test");
    }

    #[test]
    fn parse_command_request_debug() {
        let request = ParseCommandRequest {
            input: "status".to_string(),
        };
        let debug = format!("{request:?}");
        assert!(debug.contains("ParseCommandRequest"));
    }

    #[test]
    fn parse_command_response_serialize() {
        let response = ParseCommandResponse {
            command: AgentCommand::Echo {
                message: "hi".to_string(),
            },
            requires_approval: false,
            description: "Echo command".to_string(),
        };
        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("Echo"));
    }

    #[test]
    fn parse_command_response_debug() {
        let response = ParseCommandResponse {
            command: AgentCommand::Help { command: None },
            requires_approval: false,
            description: "Help".to_string(),
        };
        let debug = format!("{response:?}");
        assert!(debug.contains("ParseCommandResponse"));
    }

    #[test]
    fn command_type_name_morning_briefing() {
        let cmd = AgentCommand::MorningBriefing { date: None };
        assert_eq!(command_type_name(&cmd), "morning_briefing");
    }

    #[test]
    fn command_type_name_create_calendar_event() {
        let cmd = AgentCommand::CreateCalendarEvent {
            title: "Meeting".to_string(),
            date: chrono::NaiveDate::from_ymd_opt(2025, 1, 15).unwrap(),
            time: chrono::NaiveTime::from_hms_opt(10, 0, 0).unwrap(),
            duration_minutes: None,
            attendees: None,
            location: None,
        };
        assert_eq!(command_type_name(&cmd), "create_calendar_event");
    }

    #[test]
    fn command_type_name_summarize_inbox() {
        let cmd = AgentCommand::SummarizeInbox {
            count: None,
            only_important: None,
        };
        assert_eq!(command_type_name(&cmd), "summarize_inbox");
    }

    #[test]
    fn command_type_name_draft_email() {
        let cmd = AgentCommand::DraftEmail {
            to: domain::EmailAddress::new("test@test.com").unwrap(),
            subject: Some("Test".to_string()),
            body: "Body content".to_string(),
        };
        assert_eq!(command_type_name(&cmd), "draft_email");
    }

    #[test]
    fn command_type_name_send_email() {
        let cmd = AgentCommand::SendEmail {
            draft_id: "draft-123".to_string(),
        };
        assert_eq!(command_type_name(&cmd), "send_email");
    }

    #[test]
    fn command_type_name_ask() {
        let cmd = AgentCommand::Ask {
            question: "What?".to_string(),
        };
        assert_eq!(command_type_name(&cmd), "ask");
    }

    #[test]
    fn command_type_name_system() {
        let cmd = AgentCommand::System(SystemCommand::Status);
        assert_eq!(command_type_name(&cmd), "system");
    }

    #[test]
    fn command_type_name_echo() {
        let cmd = AgentCommand::Echo {
            message: "hi".to_string(),
        };
        assert_eq!(command_type_name(&cmd), "echo");
    }

    #[test]
    fn command_type_name_help() {
        let cmd = AgentCommand::Help { command: None };
        assert_eq!(command_type_name(&cmd), "help");
    }

    #[test]
    fn command_type_name_unknown() {
        let cmd = AgentCommand::Unknown {
            original_input: "???".to_string(),
        };
        assert_eq!(command_type_name(&cmd), "unknown");
    }

    #[test]
    fn empty_input_validation() {
        let request = ExecuteCommandRequest {
            input: "   ".to_string(),
        };
        assert!(request.input.trim().is_empty());
    }

    #[test]
    fn non_empty_input_validation() {
        let request = ExecuteCommandRequest {
            input: "  hilfe  ".to_string(),
        };
        assert!(!request.input.trim().is_empty());
    }
}
