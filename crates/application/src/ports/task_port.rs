//! Task service port
//!
//! Defines the interface for task/todo management.

use async_trait::async_trait;
use chrono::{DateTime, NaiveDate, Utc};
use domain::value_objects::{Priority, UserId};
#[cfg(test)]
use mockall::automock;
use serde::{Deserialize, Serialize};

use crate::error::ApplicationError;

// Re-export TaskStatus from domain for convenience
pub use domain::value_objects::TaskStatus;

/// Task/todo item
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    /// Unique task identifier
    pub id: String,
    /// Task summary/title
    pub summary: String,
    /// Detailed description
    pub description: Option<String>,
    /// Task priority
    pub priority: Priority,
    /// Current status
    pub status: TaskStatus,
    /// Due date (if any)
    pub due_date: Option<NaiveDate>,
    /// When the task was created
    pub created_at: DateTime<Utc>,
    /// When the task was last updated
    pub updated_at: DateTime<Utc>,
    /// When the task was completed (if applicable)
    pub completed_at: Option<DateTime<Utc>>,
    /// Associated calendar/list name
    pub calendar: Option<String>,
}

/// Parameters for creating a new task
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewTask {
    /// Task summary/title
    pub summary: String,
    /// Detailed description
    pub description: Option<String>,
    /// Task priority
    pub priority: Priority,
    /// Due date (if any)
    pub due_date: Option<NaiveDate>,
    /// Target calendar/list (None for default)
    pub calendar: Option<String>,
}

/// Query parameters for listing tasks
#[derive(Debug, Clone, Default)]
pub struct TaskQuery {
    /// Filter by status
    pub status: Option<TaskStatus>,
    /// Filter by priority
    pub priority: Option<Priority>,
    /// Filter by due date (tasks due on or before this date)
    pub due_before: Option<NaiveDate>,
    /// Include completed tasks
    pub include_completed: bool,
    /// Maximum number of tasks to return
    pub limit: Option<usize>,
}

/// Port for task/todo operations
#[cfg_attr(test, automock)]
#[async_trait]
pub trait TaskPort: Send + Sync {
    /// List tasks for a user
    async fn list_tasks(
        &self,
        user_id: &UserId,
        query: &TaskQuery,
    ) -> Result<Vec<Task>, ApplicationError>;

    /// Get a specific task by ID
    async fn get_task(
        &self,
        user_id: &UserId,
        task_id: &str,
    ) -> Result<Option<Task>, ApplicationError>;

    /// Create a new task
    async fn create_task(&self, user_id: &UserId, task: NewTask) -> Result<Task, ApplicationError>;

    /// Update a task's details
    async fn update_task(
        &self,
        user_id: &UserId,
        task_id: &str,
        updates: TaskUpdates,
    ) -> Result<Task, ApplicationError>;

    /// Mark a task as completed
    async fn complete_task(
        &self,
        user_id: &UserId,
        task_id: &str,
    ) -> Result<Task, ApplicationError>;

    /// Delete a task
    async fn delete_task(&self, user_id: &UserId, task_id: &str) -> Result<bool, ApplicationError>;

    /// Get tasks due today
    async fn get_tasks_due_today(&self, user_id: &UserId) -> Result<Vec<Task>, ApplicationError> {
        let today = Utc::now().date_naive();
        self.list_tasks(
            user_id,
            &TaskQuery {
                due_before: Some(today),
                include_completed: false,
                ..Default::default()
            },
        )
        .await
    }

    /// Get high priority tasks
    async fn get_high_priority_tasks(
        &self,
        user_id: &UserId,
    ) -> Result<Vec<Task>, ApplicationError> {
        self.list_tasks(
            user_id,
            &TaskQuery {
                priority: Some(Priority::High),
                include_completed: false,
                ..Default::default()
            },
        )
        .await
    }
}

/// Updates to apply to an existing task
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TaskUpdates {
    /// New summary (if Some)
    pub summary: Option<String>,
    /// New description (if Some)
    pub description: Option<Option<String>>,
    /// New priority (if Some)
    pub priority: Option<Priority>,
    /// New status (if Some)
    pub status: Option<TaskStatus>,
    /// New due date (if Some)
    pub due_date: Option<Option<NaiveDate>>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn _assert_object_safe(_: &dyn TaskPort) {}

    #[test]
    fn trait_is_send_sync() {
        fn assert_send_sync<T: Send + Sync + ?Sized>() {}
        assert_send_sync::<dyn TaskPort>();
    }

    #[test]
    fn task_status_is_active() {
        assert!(TaskStatus::NeedsAction.is_active());
        assert!(TaskStatus::InProgress.is_active());
        assert!(!TaskStatus::Completed.is_active());
        assert!(!TaskStatus::Cancelled.is_active());
    }

    #[test]
    fn task_status_is_done() {
        assert!(!TaskStatus::NeedsAction.is_done());
        assert!(!TaskStatus::InProgress.is_done());
        assert!(TaskStatus::Completed.is_done());
        assert!(TaskStatus::Cancelled.is_done());
    }

    #[test]
    fn task_status_display() {
        assert_eq!(TaskStatus::NeedsAction.to_string(), "Needs Action");
        assert_eq!(TaskStatus::InProgress.to_string(), "In Progress");
    }

    #[test]
    fn task_query_default() {
        let query = TaskQuery::default();
        assert!(query.status.is_none());
        assert!(query.priority.is_none());
        assert!(!query.include_completed);
    }
}
