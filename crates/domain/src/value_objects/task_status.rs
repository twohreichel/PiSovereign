//! Task status value object
//!
//! Represents the current state of a task in its lifecycle.

use serde::{Deserialize, Serialize};
use std::fmt;

/// Task status indicating its current state
///
/// Based on iCalendar VTODO STATUS property (RFC 5545).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum TaskStatus {
    /// Task needs action (not yet started)
    #[default]
    NeedsAction,
    /// Task is in progress
    InProgress,
    /// Task is completed
    Completed,
    /// Task is cancelled
    Cancelled,
}

impl TaskStatus {
    /// Check if the task is still active (not completed or cancelled)
    #[must_use]
    pub const fn is_active(&self) -> bool {
        matches!(self, Self::NeedsAction | Self::InProgress)
    }

    /// Check if the task is done (completed or cancelled)
    #[must_use]
    pub const fn is_done(&self) -> bool {
        matches!(self, Self::Completed | Self::Cancelled)
    }

    /// Get a human-readable label
    #[must_use]
    pub const fn label(&self) -> &'static str {
        match self {
            Self::NeedsAction => "Needs Action",
            Self::InProgress => "In Progress",
            Self::Completed => "Completed",
            Self::Cancelled => "Cancelled",
        }
    }

    /// Convert to iCalendar STATUS value
    #[must_use]
    pub const fn to_ical(&self) -> &'static str {
        match self {
            Self::NeedsAction => "NEEDS-ACTION",
            Self::InProgress => "IN-PROCESS",
            Self::Completed => "COMPLETED",
            Self::Cancelled => "CANCELLED",
        }
    }

    /// Parse from iCalendar STATUS value
    #[must_use]
    pub fn from_ical(status: &str) -> Self {
        match status.to_uppercase().as_str() {
            "IN-PROCESS" => Self::InProgress,
            "COMPLETED" => Self::Completed,
            "CANCELLED" => Self::Cancelled,
            _ => Self::NeedsAction,
        }
    }
}

impl fmt::Display for TaskStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.label())
    }
}

impl std::str::FromStr for TaskStatus {
    type Err = &'static str;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "needs_action" | "needs-action" | "pending" | "todo" => Ok(Self::NeedsAction),
            "in_progress" | "in-progress" | "started" => Ok(Self::InProgress),
            "completed" | "done" | "finished" => Ok(Self::Completed),
            "cancelled" | "canceled" => Ok(Self::Cancelled),
            _ => Err("Invalid task status"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_needs_action() {
        assert_eq!(TaskStatus::default(), TaskStatus::NeedsAction);
    }

    #[test]
    fn is_active_works() {
        assert!(TaskStatus::NeedsAction.is_active());
        assert!(TaskStatus::InProgress.is_active());
        assert!(!TaskStatus::Completed.is_active());
        assert!(!TaskStatus::Cancelled.is_active());
    }

    #[test]
    fn is_done_works() {
        assert!(!TaskStatus::NeedsAction.is_done());
        assert!(!TaskStatus::InProgress.is_done());
        assert!(TaskStatus::Completed.is_done());
        assert!(TaskStatus::Cancelled.is_done());
    }

    #[test]
    fn ical_roundtrip() {
        for status in [
            TaskStatus::NeedsAction,
            TaskStatus::InProgress,
            TaskStatus::Completed,
            TaskStatus::Cancelled,
        ] {
            let ical = status.to_ical();
            let parsed = TaskStatus::from_ical(ical);
            assert_eq!(parsed, status);
        }
    }

    #[test]
    fn from_str_variants() {
        assert_eq!(
            "needs_action".parse::<TaskStatus>().unwrap(),
            TaskStatus::NeedsAction
        );
        assert_eq!(
            "in_progress".parse::<TaskStatus>().unwrap(),
            TaskStatus::InProgress
        );
        assert_eq!(
            "completed".parse::<TaskStatus>().unwrap(),
            TaskStatus::Completed
        );
        assert_eq!(
            "cancelled".parse::<TaskStatus>().unwrap(),
            TaskStatus::Cancelled
        );
        assert_eq!("done".parse::<TaskStatus>().unwrap(), TaskStatus::Completed);
        assert_eq!(
            "pending".parse::<TaskStatus>().unwrap(),
            TaskStatus::NeedsAction
        );
    }

    #[test]
    fn serialization() {
        let status = TaskStatus::InProgress;
        let json = serde_json::to_string(&status).unwrap();
        assert_eq!(json, r#""in_progress""#);

        let parsed: TaskStatus = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, status);
    }
}
