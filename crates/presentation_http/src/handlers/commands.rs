//! Command handlers

use axum::{extract::State, Json};
use serde::{Deserialize, Serialize};
use tracing::instrument;

use application::ApprovalStatus;
use domain::AgentCommand;

use crate::error::ApiError;
use crate::state::AppState;

/// Command execution request
#[derive(Debug, Deserialize)]
pub struct ExecuteCommandRequest {
    /// Natural language input or explicit command
    pub input: String,
}

/// Command execution response
#[derive(Debug, Serialize)]
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
#[instrument(skip(state, request), fields(input_len = request.input.len()))]
pub async fn execute_command(
    State(state): State<AppState>,
    Json(request): Json<ExecuteCommandRequest>,
) -> Result<Json<ExecuteCommandResponse>, ApiError> {
    if request.input.trim().is_empty() {
        return Err(ApiError::BadRequest("Input cannot be empty".to_string()));
    }

    let result = state.agent_service.handle_input(&request.input).await?;

    Ok(Json(ExecuteCommandResponse {
        success: result.success,
        response: result.response,
        command_type: command_type_name(&result.command),
        execution_time_ms: result.execution_time_ms,
        requires_approval: result.approval_status.map(|s| {
            matches!(s, ApprovalStatus::Pending)
        }),
    }))
}

/// Parse command request
#[derive(Debug, Deserialize)]
pub struct ParseCommandRequest {
    /// Natural language input to parse
    pub input: String,
}

/// Parse command response
#[derive(Debug, Serialize)]
pub struct ParseCommandResponse {
    /// Parsed command
    pub command: AgentCommand,
    /// Whether this command requires approval
    pub requires_approval: bool,
    /// Human-readable description
    pub description: String,
}

/// Parse input into a command without executing
#[instrument(skip(state, request), fields(input_len = request.input.len()))]
pub async fn parse_command(
    State(state): State<AppState>,
    Json(request): Json<ParseCommandRequest>,
) -> Result<Json<ParseCommandResponse>, ApiError> {
    if request.input.trim().is_empty() {
        return Err(ApiError::BadRequest("Input cannot be empty".to_string()));
    }

    // Use the command parser through the agent service
    // For now, we'll handle the parsing directly
    let parser = application::CommandParser::new();

    // Try quick parse first, fall back to Ask if nothing matches
    let command = parser.parse_quick(&request.input).unwrap_or_else(|| AgentCommand::Ask {
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
        AgentCommand::SummarizeInbox { .. } => "summarize_inbox",
        AgentCommand::DraftEmail { .. } => "draft_email",
        AgentCommand::SendEmail { .. } => "send_email",
        AgentCommand::Ask { .. } => "ask",
        AgentCommand::System(_) => "system",
        AgentCommand::Echo { .. } => "echo",
        AgentCommand::Help { .. } => "help",
        AgentCommand::Unknown { .. } => "unknown",
    }
    .to_string()
}
