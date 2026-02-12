//! Task and task-list query handlers

use domain::UserId;

use super::{AgentService, ExecutionResult};
use crate::error::ApplicationError;

impl AgentService {
    /// Handle listing tasks
    ///
    /// Lists tasks from the configured task service, optionally filtered by
    /// status, priority, and list.
    pub(super) async fn handle_list_tasks(
        &self,
        status: Option<&domain::TaskStatus>,
        priority: Option<&domain::Priority>,
        list: Option<&str>,
    ) -> Result<ExecutionResult, ApplicationError> {
        let Some(ref task_service) = self.task_service else {
            return Ok(ExecutionResult {
                success: false,
                response: "📋 Tasks are not available.\n\n\
                          Task service is not configured. \
                          Please set up CalDAV with task support in your configuration."
                    .to_string(),
            });
        };

        let query = crate::ports::TaskQuery {
            status: status.copied(),
            priority: priority.copied(),
            include_completed: status.is_some_and(domain::TaskStatus::is_done),
            list: list.map(ToString::to_string),
            ..Default::default()
        };

        let user_id = UserId::default_user();
        let tasks = task_service.list_tasks(&user_id, &query).await?;

        let mut response = String::from("📋 **Tasks**\n\n");

        if tasks.is_empty() {
            let filter_desc = match (status, priority) {
                (Some(s), Some(p)) => format!(" matching status '{s}' and priority '{p}'"),
                (Some(s), None) => format!(" with status '{s}'"),
                (None, Some(p)) => format!(" with priority '{p}'"),
                (None, None) => String::new(),
            };
            response.push_str(&format!("No tasks found{filter_desc}.\n"));
        } else {
            let today = chrono::Local::now().date_naive();
            for task in &tasks {
                let status_emoji = match task.status {
                    domain::TaskStatus::Completed => "✅",
                    domain::TaskStatus::InProgress => "🔄",
                    domain::TaskStatus::Cancelled => "❌",
                    domain::TaskStatus::NeedsAction => "⬜",
                };
                let priority_emoji = match task.priority {
                    domain::Priority::High => "🔴",
                    domain::Priority::Medium => "🟡",
                    domain::Priority::Low => "🟢",
                };

                let is_overdue = task.due_date.is_some_and(|d| d < today);
                let overdue_marker = if is_overdue { " ⚠️ OVERDUE" } else { "" };
                let due_str = task
                    .due_date
                    .map_or(String::new(), |d| format!(" (due: {d})"));

                response.push_str(&format!(
                    "{} {} **{}**{}{}\n  ID: `{}`\n",
                    status_emoji, priority_emoji, task.summary, due_str, overdue_marker, task.id
                ));
            }
        }

        let active_count = tasks.iter().filter(|t| t.status.is_active()).count();
        let completed_count = tasks.iter().filter(|t| t.status.is_done()).count();
        response.push_str(&format!(
            "\n---\n*{active_count} active task(s), {completed_count} completed*"
        ));

        Ok(ExecutionResult {
            success: true,
            response,
        })
    }

    /// Handle listing task lists
    ///
    /// Lists all available task lists/calendars from the configured task service.
    pub(super) async fn handle_list_task_lists(&self) -> Result<ExecutionResult, ApplicationError> {
        let Some(ref task_service) = self.task_service else {
            return Ok(ExecutionResult {
                success: false,
                response: "📋 Task lists are not available.\n\n\
                          Task service is not configured. \
                          Please set up CalDAV with task support in your configuration."
                    .to_string(),
            });
        };

        let user_id = UserId::default_user();
        let lists = task_service.list_task_lists(&user_id).await?;

        let mut response = String::from("📋 **Task Lists**\n\n");

        if lists.is_empty() {
            response.push_str("No task lists found.\n");
        } else {
            for list in &lists {
                response.push_str(&format!("• **{}**\n  ID: `{}`\n", list.name, list.id));
            }
        }

        response.push_str(&format!("\n---\n*{} list(s) available*", lists.len()));

        Ok(ExecutionResult {
            success: true,
            response,
        })
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::super::{AgentService, test_support::MockInferenceEngine};

    #[tokio::test]
    async fn agent_service_with_task_service() {
        use crate::ports::MockTaskPort;

        let mock_inference = MockInferenceEngine::new();
        let mock_task = MockTaskPort::new();

        let service =
            AgentService::new(Arc::new(mock_inference)).with_task_service(Arc::new(mock_task));

        let debug = format!("{service:?}");
        assert!(debug.contains("has_task: true"));
    }
}
