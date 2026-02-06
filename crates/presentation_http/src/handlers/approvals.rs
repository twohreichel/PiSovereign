//! Approval workflow handlers
//!
//! REST API endpoints for managing approval requests.

use application::RequestContext;
use axum::{
    Extension, Json,
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
};
use domain::{AgentCommand, ApprovalId, ApprovalStatus};
use serde::{Deserialize, Serialize};
use tracing::{debug, info, instrument, warn};
use utoipa::{IntoParams, ToSchema};

use crate::{error::ApiError, state::AppState};

/// Approval request summary for API responses
#[derive(Debug, Serialize, ToSchema)]
#[schema(example = json!({
    "id": "550e8400-e29b-41d4-a716-446655440000",
    "status": "pending",
    "description": "Send email to john@example.com",
    "command_type": "send_email",
    "created_at": "2026-02-06T10:30:00Z",
    "expires_at": "2026-02-06T11:30:00Z",
    "reason": null
}))]
pub struct ApprovalResponse {
    /// Unique identifier
    pub id: String,
    /// Current status
    #[schema(value_type = crate::openapi::ApprovalStatusSchema)]
    pub status: ApprovalStatus,
    /// Human-readable description
    pub description: String,
    /// The command type requiring approval
    pub command_type: String,
    /// When the request was created (ISO 8601)
    pub created_at: String,
    /// When the request expires (ISO 8601)
    pub expires_at: String,
    /// Optional reason for denial/cancellation
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

/// List approvals query parameters
#[derive(Debug, Deserialize, IntoParams, ToSchema)]
pub struct ListApprovalsQuery {
    /// Filter by status (optional)
    pub status: Option<String>,
    /// Maximum number of results (default: 50)
    pub limit: Option<u32>,
}

/// Deny request body
#[derive(Debug, Deserialize, ToSchema)]
#[schema(example = json!({"reason": "Not authorized for this action"}))]
pub struct DenyRequest {
    /// Optional reason for denial
    pub reason: Option<String>,
}

/// List pending approvals for the current user
///
/// GET /v1/approvals
#[utoipa::path(
    get,
    path = "/v1/approvals",
    tag = "approvals",
    params(ListApprovalsQuery),
    responses(
        (status = 200, description = "List of approval requests", body = Vec<ApprovalResponse>),
        (status = 503, description = "Service unavailable", body = crate::error::ErrorResponse)
    ),
    security(("api_key" = []))
)]
#[instrument(skip(state, ctx))]
pub async fn list_approvals(
    State(state): State<AppState>,
    ctx: Option<Extension<RequestContext>>,
    axum::extract::Query(query): axum::extract::Query<ListApprovalsQuery>,
) -> Result<Json<Vec<ApprovalResponse>>, ApiError> {
    let Some(approval_service) = &state.approval_service else {
        return Err(ApiError::ServiceUnavailable(
            "Approval service not configured".to_string(),
        ));
    };

    // Extract user ID from request context, fall back to default for backward compatibility
    let user_id = ctx.map(|Extension(c)| c.user_id()).unwrap_or_default();
    let limit = query.limit.unwrap_or(50);

    // Currently only pending requests are supported via the API
    // Other statuses would require database queries that aren't exposed yet
    if query.status.is_some() && query.status.as_deref() != Some("pending") {
        warn!(
            status = ?query.status,
            "Non-pending status filter requested but not yet supported"
        );
    }

    let requests = approval_service.get_pending_for_user(&user_id).await?;

    let responses: Vec<ApprovalResponse> = requests
        .into_iter()
        .take(limit as usize)
        .map(|req| ApprovalResponse {
            id: req.id.to_string(),
            status: req.status,
            description: req.description,
            command_type: command_type_name(&req.command),
            created_at: req.created_at.to_rfc3339(),
            expires_at: req.expires_at.to_rfc3339(),
            reason: req.reason,
        })
        .collect();

    debug!(count = responses.len(), "Listed approval requests");
    Ok(Json(responses))
}

/// Get a specific approval request by ID
///
/// GET /v1/approvals/:id
#[utoipa::path(
    get,
    path = "/v1/approvals/{id}",
    tag = "approvals",
    params(
        ("id" = String, Path, description = "Approval request ID")
    ),
    responses(
        (status = 200, description = "Approval request details", body = ApprovalResponse),
        (status = 400, description = "Invalid approval ID", body = crate::error::ErrorResponse),
        (status = 404, description = "Approval request not found", body = crate::error::ErrorResponse),
        (status = 503, description = "Service unavailable", body = crate::error::ErrorResponse)
    ),
    security(("api_key" = []))
)]
#[instrument(skip(state))]
pub async fn get_approval(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<ApprovalResponse>, ApiError> {
    let Some(approval_service) = &state.approval_service else {
        return Err(ApiError::ServiceUnavailable(
            "Approval service not configured".to_string(),
        ));
    };

    let approval_id = ApprovalId::parse(&id)
        .map_err(|e| ApiError::BadRequest(format!("Invalid approval ID: {e}")))?;

    let request = approval_service
        .get_request(&approval_id)
        .await?
        .ok_or_else(|| ApiError::NotFound("Approval request not found".to_string()))?;

    Ok(Json(ApprovalResponse {
        id: request.id.to_string(),
        status: request.status,
        description: request.description,
        command_type: command_type_name(&request.command),
        created_at: request.created_at.to_rfc3339(),
        expires_at: request.expires_at.to_rfc3339(),
        reason: request.reason,
    }))
}

/// Approve a pending request
///
/// POST /v1/approvals/:id/approve
#[utoipa::path(
    post,
    path = "/v1/approvals/{id}/approve",
    tag = "approvals",
    params(
        ("id" = String, Path, description = "Approval request ID")
    ),
    responses(
        (status = 200, description = "Request approved", body = ApprovalResponse),
        (status = 400, description = "Invalid approval ID", body = crate::error::ErrorResponse),
        (status = 404, description = "Approval request not found", body = crate::error::ErrorResponse),
        (status = 503, description = "Service unavailable", body = crate::error::ErrorResponse)
    ),
    security(("api_key" = []))
)]
#[instrument(skip(state, ctx))]
pub async fn approve_request(
    State(state): State<AppState>,
    ctx: Option<Extension<RequestContext>>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    let Some(approval_service) = &state.approval_service else {
        return Err(ApiError::ServiceUnavailable(
            "Approval service not configured".to_string(),
        ));
    };

    let approval_id = ApprovalId::parse(&id)
        .map_err(|e| ApiError::BadRequest(format!("Invalid approval ID: {e}")))?;

    // Extract user ID from request context, fall back to default for backward compatibility
    let user_id = ctx.map(|Extension(c)| c.user_id()).unwrap_or_default();

    let request = approval_service.approve(&approval_id, &user_id).await?;

    info!(approval_id = %id, "Approval request approved");

    Ok((
        StatusCode::OK,
        Json(ApprovalResponse {
            id: request.id.to_string(),
            status: request.status,
            description: request.description,
            command_type: command_type_name(&request.command),
            created_at: request.created_at.to_rfc3339(),
            expires_at: request.expires_at.to_rfc3339(),
            reason: request.reason,
        }),
    ))
}

/// Deny a pending request
///
/// POST /v1/approvals/:id/deny
#[utoipa::path(
    post,
    path = "/v1/approvals/{id}/deny",
    tag = "approvals",
    params(
        ("id" = String, Path, description = "Approval request ID")
    ),
    request_body = DenyRequest,
    responses(
        (status = 200, description = "Request denied", body = ApprovalResponse),
        (status = 400, description = "Invalid approval ID", body = crate::error::ErrorResponse),
        (status = 404, description = "Approval request not found", body = crate::error::ErrorResponse),
        (status = 503, description = "Service unavailable", body = crate::error::ErrorResponse)
    ),
    security(("api_key" = []))
)]
#[instrument(skip(state, ctx, body))]
pub async fn deny_request(
    State(state): State<AppState>,
    ctx: Option<Extension<RequestContext>>,
    Path(id): Path<String>,
    Json(body): Json<DenyRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let Some(approval_service) = &state.approval_service else {
        return Err(ApiError::ServiceUnavailable(
            "Approval service not configured".to_string(),
        ));
    };

    let approval_id = ApprovalId::parse(&id)
        .map_err(|e| ApiError::BadRequest(format!("Invalid approval ID: {e}")))?;

    // Extract user ID from request context, fall back to default for backward compatibility
    let user_id = ctx.map(|Extension(c)| c.user_id()).unwrap_or_default();

    let request = approval_service
        .deny(&approval_id, &user_id, body.reason)
        .await?;

    info!(approval_id = %id, "Approval request denied");

    Ok((
        StatusCode::OK,
        Json(ApprovalResponse {
            id: request.id.to_string(),
            status: request.status,
            description: request.description,
            command_type: command_type_name(&request.command),
            created_at: request.created_at.to_rfc3339(),
            expires_at: request.expires_at.to_rfc3339(),
            reason: request.reason,
        }),
    ))
}

/// Cancel a pending request
///
/// POST /v1/approvals/:id/cancel
#[utoipa::path(
    post,
    path = "/v1/approvals/{id}/cancel",
    tag = "approvals",
    params(
        ("id" = String, Path, description = "Approval request ID")
    ),
    responses(
        (status = 200, description = "Request cancelled", body = ApprovalResponse),
        (status = 400, description = "Invalid approval ID", body = crate::error::ErrorResponse),
        (status = 404, description = "Approval request not found", body = crate::error::ErrorResponse),
        (status = 503, description = "Service unavailable", body = crate::error::ErrorResponse)
    ),
    security(("api_key" = []))
)]
#[instrument(skip(state, ctx))]
pub async fn cancel_request(
    State(state): State<AppState>,
    ctx: Option<Extension<RequestContext>>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    let Some(approval_service) = &state.approval_service else {
        return Err(ApiError::ServiceUnavailable(
            "Approval service not configured".to_string(),
        ));
    };

    let approval_id = ApprovalId::parse(&id)
        .map_err(|e| ApiError::BadRequest(format!("Invalid approval ID: {e}")))?;

    // Extract user ID from request context, fall back to default for backward compatibility
    let user_id = ctx.map(|Extension(c)| c.user_id()).unwrap_or_default();

    let request = approval_service.cancel(&approval_id, &user_id).await?;

    info!(approval_id = %id, "Approval request cancelled");

    Ok((
        StatusCode::OK,
        Json(ApprovalResponse {
            id: request.id.to_string(),
            status: request.status,
            description: request.description,
            command_type: command_type_name(&request.command),
            created_at: request.created_at.to_rfc3339(),
            expires_at: request.expires_at.to_rfc3339(),
            reason: request.reason,
        }),
    ))
}

/// Get the command type name for display
fn command_type_name(command: &AgentCommand) -> String {
    match command {
        AgentCommand::MorningBriefing { .. } => "morning_briefing",
        AgentCommand::SummarizeInbox { .. } => "summarize_inbox",
        AgentCommand::Ask { .. } => "ask",
        AgentCommand::DraftEmail { .. } => "draft_email",
        AgentCommand::SendEmail { .. } => "send_email",
        AgentCommand::CreateCalendarEvent { .. } => "create_calendar_event",
        AgentCommand::Echo { .. } => "echo",
        AgentCommand::Help { .. } => "help",
        AgentCommand::System(sys) => match sys {
            domain::SystemCommand::Status => "status",
            domain::SystemCommand::Version => "version",
            domain::SystemCommand::ListModels => "list_models",
            domain::SystemCommand::ReloadConfig => "reload_config",
            domain::SystemCommand::SwitchModel { .. } => "switch_model",
        },
        AgentCommand::Unknown { .. } => "unknown",
    }
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn approval_response_serializes() {
        let response = ApprovalResponse {
            id: "apr-123".to_string(),
            status: ApprovalStatus::Pending,
            description: "Send email to test@example.com".to_string(),
            command_type: "send_email".to_string(),
            created_at: "2025-01-01T00:00:00Z".to_string(),
            expires_at: "2025-01-01T00:30:00Z".to_string(),
            reason: None,
        };

        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("apr-123"));
        assert!(json.contains("pending"));
        assert!(json.contains("send_email"));
        // reason should be skipped when None
        assert!(!json.contains("reason"));
    }

    #[test]
    fn approval_response_includes_reason_when_present() {
        let response = ApprovalResponse {
            id: "apr-456".to_string(),
            status: ApprovalStatus::Denied,
            description: "Test".to_string(),
            command_type: "test".to_string(),
            created_at: "2025-01-01T00:00:00Z".to_string(),
            expires_at: "2025-01-01T00:30:00Z".to_string(),
            reason: Some("Not authorized".to_string()),
        };

        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("Not authorized"));
    }

    #[test]
    fn command_type_name_works() {
        assert_eq!(
            command_type_name(&AgentCommand::SendEmail {
                draft_id: "123".to_string()
            }),
            "send_email"
        );
        assert_eq!(
            command_type_name(&AgentCommand::Help { command: None }),
            "help"
        );
        assert_eq!(
            command_type_name(&AgentCommand::MorningBriefing { date: None }),
            "morning_briefing"
        );
    }
}
