//! Task adapter - Implements TaskPort using integration_caldav

use application::error::ApplicationError;
use application::ports::{NewTask, Task, TaskPort, TaskQuery, TaskStatus, TaskUpdates};
use async_trait::async_trait;
use chrono::Utc;
use domain::value_objects::{Priority, UserId};
use integration_caldav::{
    CalDavError, CalDavTaskClient, CalendarTask, TaskPriority as CalDavPriority,
    TaskStatus as CalDavStatus,
};
use std::sync::Arc;
use tracing::{debug, instrument};

use super::{CircuitBreaker, CircuitBreakerConfig};

/// Adapter for task operations using CalDAV
pub struct TaskAdapter<C: CalDavTaskClient> {
    client: Arc<C>,
    default_calendar: String,
    circuit_breaker: Option<CircuitBreaker>,
}

impl<C: CalDavTaskClient> std::fmt::Debug for TaskAdapter<C> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TaskAdapter")
            .field("default_calendar", &self.default_calendar)
            .field(
                "circuit_breaker",
                &self.circuit_breaker.as_ref().map(CircuitBreaker::name),
            )
            .finish_non_exhaustive()
    }
}

impl<C: CalDavTaskClient> TaskAdapter<C> {
    /// Create a new task adapter
    pub fn new(client: Arc<C>, default_calendar: impl Into<String>) -> Self {
        Self {
            client,
            default_calendar: default_calendar.into(),
            circuit_breaker: None,
        }
    }

    /// Enable circuit breaker with default configuration
    #[must_use]
    pub fn with_circuit_breaker(mut self) -> Self {
        self.circuit_breaker = Some(CircuitBreaker::new("caldav-tasks"));
        self
    }

    /// Enable circuit breaker with custom configuration
    #[must_use]
    pub fn with_circuit_breaker_config(mut self, config: CircuitBreakerConfig) -> Self {
        self.circuit_breaker = Some(CircuitBreaker::with_config("caldav-tasks", config));
        self
    }

    /// Check circuit and return error if open
    fn check_circuit(&self) -> Result<(), ApplicationError> {
        if let Some(ref cb) = self.circuit_breaker {
            if cb.is_open() {
                return Err(ApplicationError::ExternalService(
                    "CalDAV task service circuit breaker is open".into(),
                ));
            }
        }
        Ok(())
    }

    /// Map CalDAV error to application error
    fn map_error(err: CalDavError) -> ApplicationError {
        match err {
            CalDavError::ConnectionFailed(e) | CalDavError::RequestFailed(e) => {
                ApplicationError::ExternalService(e)
            },
            CalDavError::AuthenticationFailed => {
                ApplicationError::NotAuthorized("CalDAV authentication failed".into())
            },
            CalDavError::CalendarNotFound(e) | CalDavError::EventNotFound(e) => {
                ApplicationError::NotFound(e)
            },
            CalDavError::ParseError(e) => ApplicationError::Internal(e),
        }
    }

    /// Convert CalDAV priority to domain priority
    const fn map_priority_from_caldav(priority: CalDavPriority) -> Priority {
        match priority {
            CalDavPriority::High => Priority::High,
            CalDavPriority::Medium => Priority::Medium,
            CalDavPriority::Low => Priority::Low,
        }
    }

    /// Convert domain priority to CalDAV priority
    const fn map_priority_to_caldav(priority: Priority) -> CalDavPriority {
        match priority {
            Priority::High => CalDavPriority::High,
            Priority::Medium => CalDavPriority::Medium,
            Priority::Low => CalDavPriority::Low,
        }
    }

    /// Convert CalDAV status to application status
    const fn map_status_from_caldav(status: CalDavStatus) -> TaskStatus {
        match status {
            CalDavStatus::NeedsAction => TaskStatus::NeedsAction,
            CalDavStatus::InProgress => TaskStatus::InProgress,
            CalDavStatus::Completed => TaskStatus::Completed,
            CalDavStatus::Cancelled => TaskStatus::Cancelled,
        }
    }

    /// Convert application status to CalDAV status
    const fn map_status_to_caldav(status: TaskStatus) -> CalDavStatus {
        match status {
            TaskStatus::NeedsAction => CalDavStatus::NeedsAction,
            TaskStatus::InProgress => CalDavStatus::InProgress,
            TaskStatus::Completed => CalDavStatus::Completed,
            TaskStatus::Cancelled => CalDavStatus::Cancelled,
        }
    }

    /// Convert CalendarTask to application Task
    fn map_task(task: CalendarTask) -> Task {
        Task {
            id: task.id,
            summary: task.summary,
            description: task.description,
            priority: Self::map_priority_from_caldav(task.priority),
            status: Self::map_status_from_caldav(task.status),
            due_date: task.due,
            created_at: task.created.unwrap_or_else(Utc::now),
            updated_at: task.last_modified.unwrap_or_else(Utc::now),
            completed_at: task.completed,
            calendar: None,
        }
    }

    /// Get the calendar name for a user (could be customized per user)
    fn get_calendar_for_user(&self, _user_id: &UserId) -> &str {
        // In a more complex implementation, this could look up user-specific calendars
        &self.default_calendar
    }

    /// Filter tasks based on query
    fn filter_tasks(tasks: Vec<CalendarTask>, query: &TaskQuery) -> Vec<CalendarTask> {
        tasks
            .into_iter()
            .filter(|task| {
                // Filter by completed status
                if !query.include_completed && task.status.is_complete() {
                    return false;
                }

                // Filter by status
                if let Some(status) = query.status {
                    if Self::map_status_from_caldav(task.status) != status {
                        return false;
                    }
                }

                // Filter by priority
                if let Some(priority) = query.priority {
                    if Self::map_priority_from_caldav(task.priority) != priority {
                        return false;
                    }
                }

                // Filter by due date
                if let Some(due_before) = query.due_before {
                    if let Some(task_due) = task.due {
                        if task_due > due_before {
                            return false;
                        }
                    }
                }

                true
            })
            .collect()
    }
}

#[async_trait]
impl<C: CalDavTaskClient + 'static> TaskPort for TaskAdapter<C> {
    #[instrument(skip(self, user_id), fields(user = %user_id))]
    async fn list_tasks(
        &self,
        user_id: &UserId,
        query: &TaskQuery,
    ) -> Result<Vec<Task>, ApplicationError> {
        self.check_circuit()?;

        let calendar = self.get_calendar_for_user(user_id);
        let tasks = self
            .client
            .list_tasks(calendar)
            .await
            .map_err(Self::map_error)?;

        let filtered = Self::filter_tasks(tasks, query);
        let mut result: Vec<Task> = filtered.into_iter().map(Self::map_task).collect();

        // Apply limit
        if let Some(limit) = query.limit {
            result.truncate(limit);
        }

        debug!(count = result.len(), "Listed tasks");
        Ok(result)
    }

    #[instrument(skip(self, user_id), fields(user = %user_id, task_id))]
    async fn get_task(
        &self,
        user_id: &UserId,
        task_id: &str,
    ) -> Result<Option<Task>, ApplicationError> {
        self.check_circuit()?;

        let calendar = self.get_calendar_for_user(user_id);
        let tasks = self
            .client
            .list_tasks(calendar)
            .await
            .map_err(Self::map_error)?;

        let task = tasks
            .into_iter()
            .find(|t| t.id == task_id)
            .map(Self::map_task);

        debug!(found = task.is_some(), "Get task by ID");
        Ok(task)
    }

    #[instrument(skip(self, user_id, task), fields(user = %user_id))]
    async fn create_task(&self, user_id: &UserId, task: NewTask) -> Result<Task, ApplicationError> {
        self.check_circuit()?;

        let calendar = self.get_calendar_for_user(user_id);
        let now = Utc::now();

        let mut cal_task = CalendarTask::new(uuid::Uuid::new_v4().to_string(), task.summary);
        cal_task.description = task.description;
        cal_task.due = task.due_date;
        cal_task.priority = Self::map_priority_to_caldav(task.priority);
        cal_task.status = CalDavStatus::NeedsAction;
        cal_task.created = Some(now);
        cal_task.last_modified = Some(now);

        let id = self
            .client
            .create_task(calendar, &cal_task)
            .await
            .map_err(Self::map_error)?;

        cal_task.id = id;

        debug!(id = %cal_task.id, "Created task");
        Ok(Self::map_task(cal_task))
    }

    #[instrument(skip(self, user_id, updates), fields(user = %user_id, task_id))]
    async fn update_task(
        &self,
        user_id: &UserId,
        task_id: &str,
        updates: TaskUpdates,
    ) -> Result<Task, ApplicationError> {
        self.check_circuit()?;

        let calendar = self.get_calendar_for_user(user_id);
        let tasks = self
            .client
            .list_tasks(calendar)
            .await
            .map_err(Self::map_error)?;

        let mut cal_task = tasks
            .into_iter()
            .find(|t| t.id == task_id)
            .ok_or_else(|| ApplicationError::NotFound(format!("Task not found: {task_id}")))?;

        // Apply updates
        if let Some(summary) = updates.summary {
            cal_task.summary = summary;
        }
        if let Some(desc) = updates.description {
            cal_task.description = desc;
        }
        if let Some(priority) = updates.priority {
            cal_task.priority = Self::map_priority_to_caldav(priority);
        }
        if let Some(status) = updates.status {
            cal_task.status = Self::map_status_to_caldav(status);
            if status == TaskStatus::Completed {
                cal_task.completed = Some(Utc::now());
            }
        }
        if let Some(due_date) = updates.due_date {
            cal_task.due = due_date;
        }

        cal_task.last_modified = Some(Utc::now());

        self.client
            .update_task(calendar, &cal_task)
            .await
            .map_err(Self::map_error)?;

        debug!(id = task_id, "Updated task");
        Ok(Self::map_task(cal_task))
    }

    #[instrument(skip(self, user_id), fields(user = %user_id, task_id))]
    async fn complete_task(
        &self,
        user_id: &UserId,
        task_id: &str,
    ) -> Result<Task, ApplicationError> {
        self.check_circuit()?;

        let calendar = self.get_calendar_for_user(user_id);
        self.client
            .complete_task(calendar, task_id)
            .await
            .map_err(Self::map_error)?;

        // Fetch the updated task
        let task = self
            .get_task(user_id, task_id)
            .await?
            .ok_or_else(|| ApplicationError::NotFound(format!("Task not found: {task_id}")))?;

        debug!(id = task_id, "Completed task");
        Ok(task)
    }

    #[instrument(skip(self, user_id), fields(user = %user_id, task_id))]
    async fn delete_task(&self, user_id: &UserId, task_id: &str) -> Result<bool, ApplicationError> {
        self.check_circuit()?;

        let calendar = self.get_calendar_for_user(user_id);
        self.client
            .delete_task(calendar, task_id)
            .await
            .map_err(Self::map_error)?;

        debug!(id = task_id, "Deleted task");
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn map_priority_roundtrip() {
        assert_eq!(
            TaskAdapter::<MockCalDavClient>::map_priority_from_caldav(CalDavPriority::High),
            Priority::High
        );
        assert_eq!(
            TaskAdapter::<MockCalDavClient>::map_priority_from_caldav(CalDavPriority::Medium),
            Priority::Medium
        );
        assert_eq!(
            TaskAdapter::<MockCalDavClient>::map_priority_from_caldav(CalDavPriority::Low),
            Priority::Low
        );
    }

    #[test]
    fn map_status_roundtrip() {
        assert_eq!(
            TaskAdapter::<MockCalDavClient>::map_status_from_caldav(CalDavStatus::NeedsAction),
            TaskStatus::NeedsAction
        );
        assert_eq!(
            TaskAdapter::<MockCalDavClient>::map_status_from_caldav(CalDavStatus::InProgress),
            TaskStatus::InProgress
        );
        assert_eq!(
            TaskAdapter::<MockCalDavClient>::map_status_from_caldav(CalDavStatus::Completed),
            TaskStatus::Completed
        );
        assert_eq!(
            TaskAdapter::<MockCalDavClient>::map_status_from_caldav(CalDavStatus::Cancelled),
            TaskStatus::Cancelled
        );
    }

    #[test]
    fn map_error_connection() {
        let err = CalDavError::ConnectionFailed("timeout".into());
        let app_err = TaskAdapter::<MockCalDavClient>::map_error(err);
        assert!(matches!(app_err, ApplicationError::ExternalService(_)));
    }

    #[test]
    fn map_error_auth() {
        let err = CalDavError::AuthenticationFailed;
        let app_err = TaskAdapter::<MockCalDavClient>::map_error(err);
        assert!(matches!(app_err, ApplicationError::NotAuthorized(_)));
    }

    #[test]
    fn map_error_not_found() {
        let err = CalDavError::CalendarNotFound("calendar not found".into());
        let app_err = TaskAdapter::<MockCalDavClient>::map_error(err);
        assert!(matches!(app_err, ApplicationError::NotFound(_)));
    }

    // Mock client for tests
    struct MockCalDavClient;

    #[async_trait]
    impl CalDavTaskClient for MockCalDavClient {
        async fn list_tasks(&self, _calendar: &str) -> Result<Vec<CalendarTask>, CalDavError> {
            Ok(vec![])
        }

        async fn get_tasks_in_range(
            &self,
            _calendar: &str,
            _start: chrono::NaiveDate,
            _end: chrono::NaiveDate,
        ) -> Result<Vec<CalendarTask>, CalDavError> {
            Ok(vec![])
        }

        async fn get_overdue_tasks(
            &self,
            _calendar: &str,
        ) -> Result<Vec<CalendarTask>, CalDavError> {
            Ok(vec![])
        }

        async fn get_tasks_due_today(
            &self,
            _calendar: &str,
        ) -> Result<Vec<CalendarTask>, CalDavError> {
            Ok(vec![])
        }

        async fn get_high_priority_tasks(
            &self,
            _calendar: &str,
        ) -> Result<Vec<CalendarTask>, CalDavError> {
            Ok(vec![])
        }

        async fn create_task(
            &self,
            _calendar: &str,
            task: &CalendarTask,
        ) -> Result<String, CalDavError> {
            Ok(task.id.clone())
        }

        async fn update_task(
            &self,
            _calendar: &str,
            _task: &CalendarTask,
        ) -> Result<(), CalDavError> {
            Ok(())
        }

        async fn complete_task(&self, _calendar: &str, _task_id: &str) -> Result<(), CalDavError> {
            Ok(())
        }

        async fn delete_task(&self, _calendar: &str, _task_id: &str) -> Result<(), CalDavError> {
            Ok(())
        }
    }

    #[test]
    fn adapter_is_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<TaskAdapter<MockCalDavClient>>();
    }
}
