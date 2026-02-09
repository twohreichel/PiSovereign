//! Audit log entry entity - Records system events for security and debugging

use std::net::IpAddr;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Type of audit event
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuditEventType {
    /// User authentication events
    Authentication,
    /// Authorization/permission checks
    Authorization,
    /// Command execution events
    CommandExecution,
    /// Approval workflow events
    Approval,
    /// Configuration changes
    ConfigChange,
    /// System lifecycle events
    System,
    /// Data access events
    DataAccess,
    /// External integration events
    Integration,
    /// Security-related events
    Security,
    /// Prompt injection attempt detected
    PromptInjection,
}

impl std::fmt::Display for AuditEventType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            Self::Authentication => "authentication",
            Self::Authorization => "authorization",
            Self::CommandExecution => "command_execution",
            Self::Approval => "approval",
            Self::ConfigChange => "config_change",
            Self::System => "system",
            Self::DataAccess => "data_access",
            Self::Integration => "integration",
            Self::Security => "security",
            Self::PromptInjection => "prompt_injection",
        };
        write!(f, "{s}")
    }
}

/// Audit log entry recording a system event
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEntry {
    /// Auto-incrementing ID (set by database)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<i64>,
    /// When the event occurred
    pub timestamp: DateTime<Utc>,
    /// Type of event
    pub event_type: AuditEventType,
    /// Who performed the action (user ID, system, etc.)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub actor: Option<String>,
    /// Type of resource affected (conversation, approval, etc.)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resource_type: Option<String>,
    /// ID of the affected resource
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resource_id: Option<String>,
    /// Action performed
    pub action: String,
    /// Additional details (JSON)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<String>,
    /// IP address of the request
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ip_address: Option<IpAddr>,
    /// Whether the action succeeded
    pub success: bool,
    /// Request ID for distributed tracing correlation
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request_id: Option<Uuid>,
}

impl AuditEntry {
    /// Create a new successful audit entry
    pub fn success(event_type: AuditEventType, action: impl Into<String>) -> Self {
        Self {
            id: None,
            timestamp: Utc::now(),
            event_type,
            actor: None,
            resource_type: None,
            resource_id: None,
            action: action.into(),
            details: None,
            ip_address: None,
            success: true,
            request_id: None,
        }
    }

    /// Create a new failed audit entry
    pub fn failure(event_type: AuditEventType, action: impl Into<String>) -> Self {
        Self {
            success: false,
            ..Self::success(event_type, action)
        }
    }

    /// Set the actor
    #[must_use]
    pub fn with_actor(mut self, actor: impl Into<String>) -> Self {
        self.actor = Some(actor.into());
        self
    }

    /// Set the resource
    #[must_use]
    pub fn with_resource(
        mut self,
        resource_type: impl Into<String>,
        resource_id: impl Into<String>,
    ) -> Self {
        self.resource_type = Some(resource_type.into());
        self.resource_id = Some(resource_id.into());
        self
    }

    /// Set the request ID for distributed tracing correlation
    #[must_use]
    pub const fn with_request_id(mut self, request_id: Uuid) -> Self {
        self.request_id = Some(request_id);
        self
    }

    /// Set additional details
    #[must_use]
    pub fn with_details(mut self, details: impl Into<String>) -> Self {
        self.details = Some(details.into());
        self
    }

    /// Set the IP address
    #[must_use]
    pub const fn with_ip_address(mut self, ip: IpAddr) -> Self {
        self.ip_address = Some(ip);
        self
    }

    /// Set details from a serializable value
    #[must_use]
    pub fn with_json_details<T: Serialize>(mut self, details: &T) -> Self {
        self.details = serde_json::to_string(details).ok();
        self
    }
}

/// Builder for creating common audit entries
#[derive(Debug, Clone, Copy, Default)]
pub struct AuditBuilder;

impl AuditBuilder {
    /// Log a successful authentication
    pub fn auth_success(actor: &str) -> AuditEntry {
        AuditEntry::success(AuditEventType::Authentication, "login").with_actor(actor)
    }

    /// Log a failed authentication
    pub fn auth_failure(reason: &str) -> AuditEntry {
        AuditEntry::failure(AuditEventType::Authentication, "login").with_details(reason)
    }

    /// Log command execution
    pub fn command_executed(actor: &str, command_type: &str, command_id: &str) -> AuditEntry {
        AuditEntry::success(AuditEventType::CommandExecution, "execute")
            .with_actor(actor)
            .with_resource("command", command_id)
            .with_details(command_type)
    }

    /// Log command failure
    pub fn command_failed(actor: &str, command_type: &str, error: &str) -> AuditEntry {
        AuditEntry::failure(AuditEventType::CommandExecution, "execute")
            .with_actor(actor)
            .with_details(format!("{command_type}: {error}"))
    }

    /// Log approval requested
    pub fn approval_requested(actor: &str, approval_id: &str, action_type: &str) -> AuditEntry {
        AuditEntry::success(AuditEventType::Approval, "request")
            .with_actor(actor)
            .with_resource("approval", approval_id)
            .with_details(action_type)
    }

    /// Log approval granted
    pub fn approval_granted(approval_id: &str) -> AuditEntry {
        AuditEntry::success(AuditEventType::Approval, "approve")
            .with_resource("approval", approval_id)
    }

    /// Log approval denied
    pub fn approval_denied(approval_id: &str, reason: Option<&str>) -> AuditEntry {
        let entry = AuditEntry::success(AuditEventType::Approval, "deny")
            .with_resource("approval", approval_id);
        match reason {
            Some(r) => entry.with_details(r),
            None => entry,
        }
    }

    /// Log rate limit exceeded
    pub fn rate_limited(ip: IpAddr) -> AuditEntry {
        AuditEntry::failure(AuditEventType::Security, "rate_limit_exceeded").with_ip_address(ip)
    }

    /// Log unauthorized access attempt
    pub fn unauthorized(ip: IpAddr, resource: &str) -> AuditEntry {
        AuditEntry::failure(AuditEventType::Authorization, "unauthorized_access")
            .with_ip_address(ip)
            .with_details(resource)
    }

    /// Log system startup
    pub fn system_startup(version: &str) -> AuditEntry {
        AuditEntry::success(AuditEventType::System, "startup").with_details(version)
    }

    /// Log system shutdown
    pub fn system_shutdown(reason: &str) -> AuditEntry {
        AuditEntry::success(AuditEventType::System, "shutdown").with_details(reason)
    }

    /// Log configuration reload
    pub fn config_reloaded(actor: &str) -> AuditEntry {
        AuditEntry::success(AuditEventType::ConfigChange, "reload").with_actor(actor)
    }

    /// Log detected prompt injection attempt
    pub fn prompt_injection_detected(
        ip: IpAddr,
        threat_category: &str,
        threat_level: &str,
        details: &str,
    ) -> AuditEntry {
        AuditEntry::failure(AuditEventType::PromptInjection, "injection_detected")
            .with_ip_address(ip)
            .with_resource("threat", threat_category)
            .with_details(format!("{threat_level}: {details}"))
    }

    /// Log blocked suspicious activity
    pub fn suspicious_activity_blocked(ip: IpAddr, reason: &str) -> AuditEntry {
        AuditEntry::failure(AuditEventType::Security, "suspicious_blocked")
            .with_ip_address(ip)
            .with_details(reason)
    }

    /// Log user rate-limited due to suspicious behavior
    pub fn user_rate_limited_suspicious(ip: IpAddr, violation_count: u32) -> AuditEntry {
        AuditEntry::failure(AuditEventType::Security, "suspicious_rate_limit")
            .with_ip_address(ip)
            .with_details(format!("Blocked after {violation_count} violations"))
    }
}

#[cfg(test)]
mod tests {
    use std::net::Ipv4Addr;

    use super::*;

    #[test]
    fn create_success_entry() {
        let entry = AuditEntry::success(AuditEventType::CommandExecution, "test_action");

        assert!(entry.success);
        assert_eq!(entry.action, "test_action");
        assert_eq!(entry.event_type, AuditEventType::CommandExecution);
    }

    #[test]
    fn create_failure_entry() {
        let entry = AuditEntry::failure(AuditEventType::Authentication, "login");

        assert!(!entry.success);
        assert_eq!(entry.action, "login");
    }

    #[test]
    fn builder_pattern() {
        let ip = IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1));
        let entry = AuditEntry::success(AuditEventType::DataAccess, "read")
            .with_actor("user-123")
            .with_resource("conversation", "conv-456")
            .with_details("Fetched conversation history")
            .with_ip_address(ip);

        assert_eq!(entry.actor, Some("user-123".to_string()));
        assert_eq!(entry.resource_type, Some("conversation".to_string()));
        assert_eq!(entry.resource_id, Some("conv-456".to_string()));
        assert_eq!(
            entry.details,
            Some("Fetched conversation history".to_string())
        );
        assert_eq!(entry.ip_address, Some(ip));
    }

    #[test]
    fn json_details() {
        #[derive(Serialize)]
        struct Details {
            count: u32,
            model: String,
        }

        let details = Details {
            count: 42,
            model: "qwen".to_string(),
        };

        let entry = AuditEntry::success(AuditEventType::CommandExecution, "inference")
            .with_json_details(&details);

        assert!(entry.details.is_some());
        let json = entry.details.unwrap();
        assert!(json.contains("42"));
        assert!(json.contains("qwen"));
    }

    #[test]
    fn audit_builder_auth_success() {
        let entry = AuditBuilder::auth_success("user-123");

        assert!(entry.success);
        assert_eq!(entry.event_type, AuditEventType::Authentication);
        assert_eq!(entry.actor, Some("user-123".to_string()));
    }

    #[test]
    fn audit_builder_auth_failure() {
        let entry = AuditBuilder::auth_failure("Invalid API key");

        assert!(!entry.success);
        assert_eq!(entry.details, Some("Invalid API key".to_string()));
    }

    #[test]
    fn audit_builder_command() {
        let entry = AuditBuilder::command_executed("user-123", "echo", "cmd-456");

        assert!(entry.success);
        assert_eq!(entry.resource_type, Some("command".to_string()));
        assert_eq!(entry.resource_id, Some("cmd-456".to_string()));
    }

    #[test]
    fn audit_builder_approval() {
        let entry = AuditBuilder::approval_requested("user-123", "apr-456", "send_email");

        assert_eq!(entry.event_type, AuditEventType::Approval);
        assert_eq!(entry.action, "request");
    }

    #[test]
    fn audit_builder_rate_limit() {
        let ip = IpAddr::V4(Ipv4Addr::new(10, 0, 0, 1));
        let entry = AuditBuilder::rate_limited(ip);

        assert!(!entry.success);
        assert_eq!(entry.event_type, AuditEventType::Security);
        assert_eq!(entry.ip_address, Some(ip));
    }

    #[test]
    fn audit_builder_system() {
        let entry = AuditBuilder::system_startup("1.0.0");

        assert_eq!(entry.event_type, AuditEventType::System);
        assert_eq!(entry.action, "startup");
        assert_eq!(entry.details, Some("1.0.0".to_string()));
    }

    #[test]
    fn event_type_display() {
        assert_eq!(AuditEventType::Authentication.to_string(), "authentication");
        assert_eq!(
            AuditEventType::CommandExecution.to_string(),
            "command_execution"
        );
        assert_eq!(
            AuditEventType::PromptInjection.to_string(),
            "prompt_injection"
        );
    }

    #[test]
    fn audit_builder_prompt_injection() {
        let ip = IpAddr::V4(Ipv4Addr::new(192, 168, 1, 100));
        let entry = AuditBuilder::prompt_injection_detected(
            ip,
            "prompt_injection",
            "high",
            "ignore previous instructions",
        );

        assert!(!entry.success);
        assert_eq!(entry.event_type, AuditEventType::PromptInjection);
        assert_eq!(entry.action, "injection_detected");
        assert_eq!(entry.ip_address, Some(ip));
        assert_eq!(entry.resource_type, Some("threat".to_string()));
        assert_eq!(entry.resource_id, Some("prompt_injection".to_string()));
        assert!(entry.details.as_ref().unwrap().contains("high"));
    }

    #[test]
    fn audit_builder_suspicious_blocked() {
        let ip = IpAddr::V4(Ipv4Addr::new(10, 0, 0, 50));
        let entry = AuditBuilder::suspicious_activity_blocked(ip, "Multiple injection attempts");

        assert!(!entry.success);
        assert_eq!(entry.event_type, AuditEventType::Security);
        assert_eq!(entry.action, "suspicious_blocked");
        assert!(entry.details.as_ref().unwrap().contains("injection"));
    }

    #[test]
    fn audit_builder_suspicious_rate_limit() {
        let ip = IpAddr::V4(Ipv4Addr::new(172, 16, 0, 1));
        let entry = AuditBuilder::user_rate_limited_suspicious(ip, 5);

        assert!(!entry.success);
        assert_eq!(entry.event_type, AuditEventType::Security);
        assert!(entry.details.as_ref().unwrap().contains("5 violations"));
    }

    #[test]
    fn serialization() {
        let entry = AuditEntry::success(AuditEventType::System, "test").with_actor("system");

        let json = serde_json::to_string(&entry).unwrap();
        assert!(json.contains("\"event_type\":\"system\""));
        assert!(json.contains("\"actor\":\"system\""));
    }

    #[test]
    fn entry_has_timestamp() {
        let before = Utc::now();
        let entry = AuditEntry::success(AuditEventType::System, "test");
        let after = Utc::now();

        assert!(entry.timestamp >= before);
        assert!(entry.timestamp <= after);
    }

    #[test]
    fn entry_clone() {
        let entry = AuditEntry::success(AuditEventType::System, "test");
        #[allow(clippy::redundant_clone)]
        let cloned = entry.clone();

        assert_eq!(entry.action, cloned.action);
    }

    #[test]
    fn entry_debug() {
        let entry = AuditEntry::success(AuditEventType::System, "test");
        let debug = format!("{entry:?}");

        assert!(debug.contains("AuditEntry"));
        assert!(debug.contains("System"));
    }

    // === Additional AuditBuilder tests ===

    #[test]
    fn audit_builder_command_failed() {
        let entry = AuditBuilder::command_failed("user-456", "create_event", "Permission denied");

        assert!(!entry.success);
        assert_eq!(entry.event_type, AuditEventType::CommandExecution);
        assert_eq!(entry.action, "execute");
        assert!(entry.details.as_ref().unwrap().contains("create_event"));
        assert!(
            entry
                .details
                .as_ref()
                .unwrap()
                .contains("Permission denied")
        );
    }

    #[test]
    fn audit_builder_approval_granted() {
        let entry = AuditBuilder::approval_granted("apr-123");

        assert!(entry.success);
        assert_eq!(entry.event_type, AuditEventType::Approval);
        assert_eq!(entry.action, "approve");
        assert_eq!(entry.resource_id, Some("apr-123".to_string()));
    }

    #[test]
    fn audit_builder_approval_denied_with_reason() {
        let entry = AuditBuilder::approval_denied("apr-456", Some("User rejected"));

        assert!(entry.success); // denial is a successful action
        assert_eq!(entry.event_type, AuditEventType::Approval);
        assert_eq!(entry.action, "deny");
        assert_eq!(entry.details, Some("User rejected".to_string()));
    }

    #[test]
    fn audit_builder_approval_denied_without_reason() {
        let entry = AuditBuilder::approval_denied("apr-789", None);

        assert_eq!(entry.action, "deny");
        assert!(entry.details.is_none());
    }

    #[test]
    fn audit_builder_unauthorized() {
        let ip = IpAddr::V4(Ipv4Addr::new(192, 168, 1, 50));
        let entry = AuditBuilder::unauthorized(ip, "/api/admin/config");

        assert!(!entry.success);
        assert_eq!(entry.event_type, AuditEventType::Authorization);
        assert_eq!(entry.action, "unauthorized_access");
        assert_eq!(entry.ip_address, Some(ip));
        assert!(entry.details.as_ref().unwrap().contains("/api/admin"));
    }

    #[test]
    fn audit_builder_system_shutdown() {
        let entry = AuditBuilder::system_shutdown("graceful");

        assert!(entry.success);
        assert_eq!(entry.event_type, AuditEventType::System);
        assert_eq!(entry.action, "shutdown");
        assert_eq!(entry.details, Some("graceful".to_string()));
    }

    #[test]
    fn audit_builder_config_reloaded() {
        let entry = AuditBuilder::config_reloaded("admin-user");

        assert!(entry.success);
        assert_eq!(entry.event_type, AuditEventType::ConfigChange);
        assert_eq!(entry.action, "reload");
        assert_eq!(entry.actor, Some("admin-user".to_string()));
    }

    #[test]
    fn with_request_id() {
        let request_id = Uuid::new_v4();
        let entry = AuditEntry::success(AuditEventType::System, "test").with_request_id(request_id);

        assert_eq!(entry.request_id, Some(request_id));
    }

    #[test]
    fn event_type_all_variants_have_display() {
        let variants = [
            AuditEventType::Authentication,
            AuditEventType::Authorization,
            AuditEventType::CommandExecution,
            AuditEventType::Approval,
            AuditEventType::ConfigChange,
            AuditEventType::System,
            AuditEventType::DataAccess,
            AuditEventType::Integration,
            AuditEventType::Security,
            AuditEventType::PromptInjection,
        ];

        for variant in variants {
            let display = variant.to_string();
            assert!(!display.is_empty());
        }
    }

    #[test]
    fn audit_builder_default_has_debug() {
        let builder = AuditBuilder;
        let debug = format!("{builder:?}");
        assert!(debug.contains("AuditBuilder"));
    }
}
