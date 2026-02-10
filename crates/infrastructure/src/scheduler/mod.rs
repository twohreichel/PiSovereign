//! Cron-based task scheduler for recurring tasks
//!
//! Provides scheduled task execution for:
//! - Weather data refresh
//! - Calendar synchronization
//! - Automated daily briefings
//! - Backup operations
//!
//! Uses `tokio-cron-scheduler` for cron-based scheduling.

use std::{
    collections::HashMap,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
};

use chrono::{DateTime, Utc};
use parking_lot::RwLock;
use thiserror::Error;
use tokio::sync::{Mutex as AsyncMutex, mpsc};
use tokio_cron_scheduler::{Job, JobScheduler, JobSchedulerError};
use tracing::{debug, error, info, instrument, warn};
use uuid::Uuid;

/// Scheduler errors
#[derive(Debug, Error)]
pub enum SchedulerError {
    /// Invalid cron expression
    #[error("Invalid cron expression: {0}")]
    InvalidCronExpression(String),

    /// Task not found
    #[error("Task not found: {0}")]
    TaskNotFound(String),

    /// Scheduler failed to start
    #[error("Scheduler failed to start: {0}")]
    StartupFailed(String),

    /// Task execution failed
    #[error("Task execution failed: {0}")]
    ExecutionFailed(String),

    /// Scheduler is not running
    #[error("Scheduler is not running")]
    NotRunning,

    /// Internal scheduler error
    #[error("Internal scheduler error: {0}")]
    Internal(String),
}

impl From<JobSchedulerError> for SchedulerError {
    fn from(err: JobSchedulerError) -> Self {
        Self::Internal(err.to_string())
    }
}

/// Task status for monitoring
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskStatus {
    /// Task is scheduled and waiting
    Scheduled,
    /// Task is currently running
    Running,
    /// Task completed successfully
    Completed,
    /// Task failed
    Failed,
    /// Task is paused
    Paused,
}

impl std::fmt::Display for TaskStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Scheduled => write!(f, "scheduled"),
            Self::Running => write!(f, "running"),
            Self::Completed => write!(f, "completed"),
            Self::Failed => write!(f, "failed"),
            Self::Paused => write!(f, "paused"),
        }
    }
}

/// Statistics for a scheduled task
#[derive(Debug, Clone)]
pub struct TaskStats {
    /// Task name
    pub name: String,
    /// Cron expression
    pub cron_expression: String,
    /// Current status
    pub status: TaskStatus,
    /// Number of successful executions
    pub success_count: u64,
    /// Number of failed executions
    pub failure_count: u64,
    /// Last execution time
    pub last_run: Option<DateTime<Utc>>,
    /// Last successful execution time
    pub last_success: Option<DateTime<Utc>>,
    /// Last failure time
    pub last_failure: Option<DateTime<Utc>>,
    /// Last error message
    pub last_error: Option<String>,
    /// Next scheduled run
    pub next_run: Option<DateTime<Utc>>,
    /// Average execution duration in milliseconds
    pub avg_duration_ms: u64,
}

/// Internal task metadata
struct TaskMetadata {
    name: String,
    cron_expression: String,
    job_id: Uuid,
    status: TaskStatus,
    success_count: AtomicU64,
    failure_count: AtomicU64,
    last_run: RwLock<Option<DateTime<Utc>>>,
    last_success: RwLock<Option<DateTime<Utc>>>,
    last_failure: RwLock<Option<DateTime<Utc>>>,
    last_error: RwLock<Option<String>>,
    total_duration_ms: AtomicU64,
    paused: AtomicBool,
}

impl TaskMetadata {
    #[allow(clippy::missing_const_for_fn)] // RwLock::new is not const in parking_lot
    fn new(name: String, cron_expression: String, job_id: Uuid) -> Self {
        Self {
            name,
            cron_expression,
            job_id,
            status: TaskStatus::Scheduled,
            success_count: AtomicU64::new(0),
            failure_count: AtomicU64::new(0),
            last_run: RwLock::new(None),
            last_success: RwLock::new(None),
            last_failure: RwLock::new(None),
            last_error: RwLock::new(None),
            total_duration_ms: AtomicU64::new(0),
            paused: AtomicBool::new(false),
        }
    }

    fn to_stats(&self, next_run: Option<DateTime<Utc>>) -> TaskStats {
        let success = self.success_count.load(Ordering::Relaxed);
        let failure = self.failure_count.load(Ordering::Relaxed);
        let total = success + failure;
        let avg_duration = if total > 0 {
            self.total_duration_ms.load(Ordering::Relaxed) / total
        } else {
            0
        };

        let status = if self.paused.load(Ordering::Relaxed) {
            TaskStatus::Paused
        } else {
            self.status
        };

        TaskStats {
            name: self.name.clone(),
            cron_expression: self.cron_expression.clone(),
            status,
            success_count: success,
            failure_count: failure,
            last_run: *self.last_run.read(),
            last_success: *self.last_success.read(),
            last_failure: *self.last_failure.read(),
            last_error: self.last_error.read().clone(),
            next_run,
            avg_duration_ms: avg_duration,
        }
    }

    fn record_success(&self, duration_ms: u64) {
        let now = Utc::now();
        self.success_count.fetch_add(1, Ordering::Relaxed);
        self.total_duration_ms
            .fetch_add(duration_ms, Ordering::Relaxed);
        *self.last_run.write() = Some(now);
        *self.last_success.write() = Some(now);
    }

    fn record_failure(&self, error: String, duration_ms: u64) {
        let now = Utc::now();
        self.failure_count.fetch_add(1, Ordering::Relaxed);
        self.total_duration_ms
            .fetch_add(duration_ms, Ordering::Relaxed);
        *self.last_run.write() = Some(now);
        *self.last_failure.write() = Some(now);
        *self.last_error.write() = Some(error);
    }
}

/// Task completion event sent to the event channel
#[derive(Debug, Clone)]
pub struct TaskEvent {
    /// Task name
    pub task_name: String,
    /// Whether the task succeeded
    pub success: bool,
    /// Error message if failed
    pub error: Option<String>,
    /// Execution duration in milliseconds
    pub duration_ms: u64,
    /// When the task completed
    pub completed_at: DateTime<Utc>,
}

/// Scheduler configuration
#[derive(Debug, Clone)]
pub struct SchedulerConfig {
    /// Whether to start the scheduler immediately
    pub auto_start: bool,
    /// Maximum concurrent task executions
    pub max_concurrent_tasks: usize,
    /// Task event buffer size
    pub event_buffer_size: usize,
}

impl Default for SchedulerConfig {
    fn default() -> Self {
        Self {
            auto_start: true,
            max_concurrent_tasks: 10,
            event_buffer_size: 100,
        }
    }
}

/// Predefined cron expressions for common schedules
pub mod schedules {
    /// Every minute
    pub const EVERY_MINUTE: &str = "0 * * * * *";
    /// Every 5 minutes
    pub const EVERY_5_MINUTES: &str = "0 */5 * * * *";
    /// Every 15 minutes
    pub const EVERY_15_MINUTES: &str = "0 */15 * * * *";
    /// Every 30 minutes
    pub const EVERY_30_MINUTES: &str = "0 */30 * * * *";
    /// Every hour
    pub const HOURLY: &str = "0 0 * * * *";
    /// Every day at midnight
    pub const DAILY_MIDNIGHT: &str = "0 0 0 * * *";
    /// Every day at 6 AM
    pub const DAILY_6AM: &str = "0 0 6 * * *";
    /// Every day at 7 AM (morning briefing)
    pub const DAILY_7AM: &str = "0 0 7 * * *";
    /// Every day at 8 PM (evening briefing)
    pub const DAILY_8PM: &str = "0 0 20 * * *";
    /// Every Sunday at midnight (using Sun instead of 0)
    pub const WEEKLY: &str = "0 0 0 * * Sun";
    /// First day of month at midnight
    pub const MONTHLY: &str = "0 0 0 1 * *";
}

/// Task scheduler for recurring background tasks
pub struct TaskScheduler {
    scheduler: AsyncMutex<JobScheduler>,
    tasks: Arc<RwLock<HashMap<String, Arc<TaskMetadata>>>>,
    running: Arc<AtomicBool>,
    event_tx: mpsc::Sender<TaskEvent>,
    event_rx: Arc<RwLock<Option<mpsc::Receiver<TaskEvent>>>>,
}

impl std::fmt::Debug for TaskScheduler {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TaskScheduler")
            .field("running", &self.running.load(Ordering::Relaxed))
            .field("task_count", &self.tasks.read().len())
            .finish_non_exhaustive()
    }
}

impl TaskScheduler {
    /// Create a new task scheduler
    #[instrument(skip_all)]
    pub async fn new(config: SchedulerConfig) -> Result<Self, SchedulerError> {
        let scheduler = JobScheduler::new().await?;
        let (event_tx, event_rx) = mpsc::channel(config.event_buffer_size);

        let instance = Self {
            scheduler: AsyncMutex::new(scheduler),
            tasks: Arc::new(RwLock::new(HashMap::new())),
            running: Arc::new(AtomicBool::new(false)),
            event_tx,
            event_rx: Arc::new(RwLock::new(Some(event_rx))),
        };

        if config.auto_start {
            instance.start().await?;
        }

        info!("Task scheduler initialized");
        Ok(instance)
    }

    /// Start the scheduler
    #[instrument(skip(self))]
    pub async fn start(&self) -> Result<(), SchedulerError> {
        if self.running.load(Ordering::Relaxed) {
            debug!("Scheduler already running");
            return Ok(());
        }

        self.scheduler.lock().await.start().await?;
        self.running.store(true, Ordering::Relaxed);
        info!("Task scheduler started");
        Ok(())
    }

    /// Stop the scheduler gracefully
    #[instrument(skip(self))]
    pub async fn stop(&self) -> Result<(), SchedulerError> {
        if !self.running.load(Ordering::Relaxed) {
            debug!("Scheduler already stopped");
            return Ok(());
        }

        self.scheduler.lock().await.shutdown().await?;
        self.running.store(false, Ordering::Relaxed);
        info!("Task scheduler stopped");
        Ok(())
    }

    /// Check if the scheduler is running
    #[must_use]
    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::Relaxed)
    }

    /// Take the event receiver (can only be called once)
    pub fn take_event_receiver(&self) -> Option<mpsc::Receiver<TaskEvent>> {
        self.event_rx.write().take()
    }

    /// Add a scheduled task
    ///
    /// # Arguments
    /// * `name` - Unique task name
    /// * `cron_expression` - Cron expression (6 fields: sec min hour day month weekday)
    /// * `task` - Async task function
    ///
    /// # Cron Format
    /// ```text
    /// ┌──────────── second (0-59)
    /// │ ┌────────── minute (0-59)
    /// │ │ ┌──────── hour (0-23)
    /// │ │ │ ┌────── day of month (1-31)
    /// │ │ │ │ ┌──── month (1-12)
    /// │ │ │ │ │ ┌── day of week (0-6, Sunday=0)
    /// │ │ │ │ │ │
    /// * * * * * *
    /// ```
    #[instrument(skip(self, task))]
    pub async fn add_task<F, Fut>(
        &self,
        name: &str,
        cron_expression: &str,
        task: F,
    ) -> Result<(), SchedulerError>
    where
        F: Fn() -> Fut + Send + Sync + 'static,
        Fut: std::future::Future<Output = Result<(), String>> + Send + 'static,
    {
        // Validate cron expression
        cron_expression.parse::<cron::Schedule>().map_err(|e| {
            SchedulerError::InvalidCronExpression(format!("{cron_expression}: {e}"))
        })?;

        let name_clone = name.to_string();
        let _cron_clone = cron_expression.to_string();
        let tasks = Arc::clone(&self.tasks);
        let event_tx = self.event_tx.clone();

        let job = Job::new_async(cron_expression, move |_uuid, _lock| {
            let name = name_clone.clone();
            let tasks = Arc::clone(&tasks);
            let event_tx = event_tx.clone();
            let task_future = task();

            Box::pin(async move {
                // Check if task is paused
                if let Some(metadata) = tasks.read().get(&name) {
                    if metadata.paused.load(Ordering::Relaxed) {
                        debug!(task = %name, "Task is paused, skipping execution");
                        return;
                    }
                }

                debug!(task = %name, "Starting scheduled task");
                let start = std::time::Instant::now();
                let result = task_future.await;
                let duration_ms = u64::try_from(start.elapsed().as_millis()).unwrap_or(u64::MAX);

                let (success, error) = match result {
                    Ok(()) => {
                        if let Some(metadata) = tasks.read().get(&name) {
                            metadata.record_success(duration_ms);
                        }
                        info!(task = %name, duration_ms, "Task completed successfully");
                        (true, None)
                    },
                    Err(e) => {
                        if let Some(metadata) = tasks.read().get(&name) {
                            metadata.record_failure(e.clone(), duration_ms);
                        }
                        error!(task = %name, error = %e, duration_ms, "Task failed");
                        (false, Some(e))
                    },
                };

                // Send event notification
                let event = TaskEvent {
                    task_name: name,
                    success,
                    error,
                    duration_ms,
                    completed_at: Utc::now(),
                };
                let _ = event_tx.try_send(event);
            })
        })
        .map_err(|e| SchedulerError::InvalidCronExpression(e.to_string()))?;

        let job_id = job.guid();
        self.scheduler.lock().await.add(job).await?;

        // Store task metadata
        let metadata = Arc::new(TaskMetadata::new(
            name.to_string(),
            cron_expression.to_string(),
            job_id,
        ));
        self.tasks.write().insert(name.to_string(), metadata);

        info!(task = %name, cron = %cron_expression, "Task scheduled");
        Ok(())
    }

    /// Remove a scheduled task
    #[instrument(skip(self))]
    pub async fn remove_task(&self, name: &str) -> Result<(), SchedulerError> {
        let metadata = self
            .tasks
            .write()
            .remove(name)
            .ok_or_else(|| SchedulerError::TaskNotFound(name.to_string()))?;

        self.scheduler.lock().await.remove(&metadata.job_id).await?;
        info!(task = %name, "Task removed");
        Ok(())
    }

    /// Pause a task (keeps it scheduled but skips executions)
    #[instrument(skip(self))]
    pub fn pause_task(&self, name: &str) -> Result<(), SchedulerError> {
        let tasks = self.tasks.read();
        let metadata = tasks
            .get(name)
            .ok_or_else(|| SchedulerError::TaskNotFound(name.to_string()))?;

        metadata.paused.store(true, Ordering::Relaxed);
        info!(task = %name, "Task paused");
        Ok(())
    }

    /// Resume a paused task
    #[instrument(skip(self))]
    pub fn resume_task(&self, name: &str) -> Result<(), SchedulerError> {
        let tasks = self.tasks.read();
        let metadata = tasks
            .get(name)
            .ok_or_else(|| SchedulerError::TaskNotFound(name.to_string()))?;

        metadata.paused.store(false, Ordering::Relaxed);
        info!(task = %name, "Task resumed");
        Ok(())
    }

    /// Get statistics for a specific task
    #[must_use]
    pub fn get_task_stats(&self, name: &str) -> Option<TaskStats> {
        let tasks = self.tasks.read();
        tasks.get(name).map(|m| m.to_stats(None))
    }

    /// Get statistics for all tasks
    #[must_use]
    pub fn get_all_stats(&self) -> Vec<TaskStats> {
        let tasks = self.tasks.read();
        tasks.values().map(|m| m.to_stats(None)).collect()
    }

    /// List all scheduled task names
    #[must_use]
    pub fn list_tasks(&self) -> Vec<String> {
        self.tasks.read().keys().cloned().collect()
    }

    /// Get the number of scheduled tasks
    #[must_use]
    pub fn task_count(&self) -> usize {
        self.tasks.read().len()
    }
}

/// Builder for creating common scheduled tasks
#[derive(Debug)]
pub struct TaskBuilder {
    /// Weather check interval (default: every 30 minutes)
    pub weather_cron: String,
    /// Calendar sync interval (default: every 15 minutes)
    pub calendar_cron: String,
    /// Morning briefing time (default: 7 AM)
    pub morning_briefing_cron: String,
    /// Evening briefing time (default: 8 PM)
    pub evening_briefing_cron: String,
    /// Backup interval (default: daily at midnight)
    pub backup_cron: String,
}

impl Default for TaskBuilder {
    fn default() -> Self {
        Self {
            weather_cron: schedules::EVERY_30_MINUTES.to_string(),
            calendar_cron: schedules::EVERY_15_MINUTES.to_string(),
            morning_briefing_cron: schedules::DAILY_7AM.to_string(),
            evening_briefing_cron: schedules::DAILY_8PM.to_string(),
            backup_cron: schedules::DAILY_MIDNIGHT.to_string(),
        }
    }
}

impl TaskBuilder {
    /// Create a new task builder with default schedules
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Set weather check schedule
    #[must_use]
    pub fn weather_schedule(mut self, cron: impl Into<String>) -> Self {
        self.weather_cron = cron.into();
        self
    }

    /// Set calendar sync schedule
    #[must_use]
    pub fn calendar_schedule(mut self, cron: impl Into<String>) -> Self {
        self.calendar_cron = cron.into();
        self
    }

    /// Set morning briefing schedule
    #[must_use]
    pub fn morning_briefing_schedule(mut self, cron: impl Into<String>) -> Self {
        self.morning_briefing_cron = cron.into();
        self
    }

    /// Set evening briefing schedule
    #[must_use]
    pub fn evening_briefing_schedule(mut self, cron: impl Into<String>) -> Self {
        self.evening_briefing_cron = cron.into();
        self
    }

    /// Set backup schedule
    #[must_use]
    pub fn backup_schedule(mut self, cron: impl Into<String>) -> Self {
        self.backup_cron = cron.into();
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Duration;
    use tokio::time::sleep;

    #[tokio::test]
    async fn test_scheduler_creation() {
        let config = SchedulerConfig {
            auto_start: false,
            ..Default::default()
        };
        let scheduler = TaskScheduler::new(config).await.unwrap();
        assert!(!scheduler.is_running());
    }

    #[tokio::test]
    async fn test_scheduler_start_stop() {
        let scheduler = TaskScheduler::new(SchedulerConfig::default())
            .await
            .unwrap();
        assert!(scheduler.is_running());

        scheduler.stop().await.unwrap();
        assert!(!scheduler.is_running());

        // Note: tokio-cron-scheduler doesn't support restart after shutdown,
        // so we don't test restart functionality. Create a new scheduler instead.
    }

    #[tokio::test]
    async fn test_add_task() {
        let scheduler = TaskScheduler::new(SchedulerConfig::default())
            .await
            .unwrap();

        scheduler
            .add_task("test-task", schedules::HOURLY, || async { Ok(()) })
            .await
            .unwrap();

        assert_eq!(scheduler.task_count(), 1);
        assert!(scheduler.list_tasks().contains(&"test-task".to_string()));

        scheduler.stop().await.unwrap();
    }

    #[tokio::test]
    async fn test_invalid_cron_expression() {
        let scheduler = TaskScheduler::new(SchedulerConfig::default())
            .await
            .unwrap();

        let result = scheduler
            .add_task("bad-task", "invalid cron", || async { Ok(()) })
            .await;

        assert!(matches!(
            result,
            Err(SchedulerError::InvalidCronExpression(_))
        ));

        scheduler.stop().await.unwrap();
    }

    #[tokio::test]
    async fn test_remove_task() {
        let scheduler = TaskScheduler::new(SchedulerConfig::default())
            .await
            .unwrap();

        scheduler
            .add_task("removable", schedules::HOURLY, || async { Ok(()) })
            .await
            .unwrap();

        assert_eq!(scheduler.task_count(), 1);

        scheduler.remove_task("removable").await.unwrap();
        assert_eq!(scheduler.task_count(), 0);

        scheduler.stop().await.unwrap();
    }

    #[tokio::test]
    async fn test_remove_nonexistent_task() {
        let scheduler = TaskScheduler::new(SchedulerConfig::default())
            .await
            .unwrap();

        let result = scheduler.remove_task("nonexistent").await;
        assert!(matches!(result, Err(SchedulerError::TaskNotFound(_))));

        scheduler.stop().await.unwrap();
    }

    #[tokio::test]
    async fn test_pause_resume_task() {
        let scheduler = TaskScheduler::new(SchedulerConfig::default())
            .await
            .unwrap();

        scheduler
            .add_task("pausable", schedules::HOURLY, || async { Ok(()) })
            .await
            .unwrap();

        scheduler.pause_task("pausable").unwrap();
        let stats = scheduler.get_task_stats("pausable").unwrap();
        assert_eq!(stats.status, TaskStatus::Paused);

        scheduler.resume_task("pausable").unwrap();
        let stats = scheduler.get_task_stats("pausable").unwrap();
        assert_eq!(stats.status, TaskStatus::Scheduled);

        scheduler.stop().await.unwrap();
    }

    #[tokio::test]
    async fn test_task_execution() {
        let counter = Arc::new(AtomicUsize::new(0));
        let counter_clone = Arc::clone(&counter);

        let scheduler = TaskScheduler::new(SchedulerConfig::default())
            .await
            .unwrap();

        // Schedule a task that runs every second
        scheduler
            .add_task("counter-task", "* * * * * *", move || {
                let counter = Arc::clone(&counter_clone);
                async move {
                    counter.fetch_add(1, Ordering::Relaxed);
                    Ok(())
                }
            })
            .await
            .unwrap();

        // Wait for at least one execution
        sleep(Duration::from_secs(2)).await;

        let count = counter.load(Ordering::Relaxed);
        assert!(
            count >= 1,
            "Task should have executed at least once, got {count}"
        );

        scheduler.stop().await.unwrap();
    }

    #[tokio::test]
    async fn test_task_stats_tracking() {
        let scheduler = TaskScheduler::new(SchedulerConfig::default())
            .await
            .unwrap();

        scheduler
            .add_task("stats-task", "* * * * * *", || async { Ok(()) })
            .await
            .unwrap();

        // Wait for execution
        sleep(Duration::from_secs(2)).await;

        let stats = scheduler.get_task_stats("stats-task").unwrap();
        assert!(stats.success_count >= 1);
        assert_eq!(stats.failure_count, 0);

        scheduler.stop().await.unwrap();
    }

    #[tokio::test]
    async fn test_task_failure_tracking() {
        let scheduler = TaskScheduler::new(SchedulerConfig::default())
            .await
            .unwrap();

        scheduler
            .add_task("failing-task", "* * * * * *", || async {
                Err("intentional failure".to_string())
            })
            .await
            .unwrap();

        // Wait for execution
        sleep(Duration::from_secs(2)).await;

        let stats = scheduler.get_task_stats("failing-task").unwrap();
        assert!(stats.failure_count >= 1);
        assert!(stats.last_error.is_some());

        scheduler.stop().await.unwrap();
    }

    #[tokio::test]
    async fn test_get_all_stats() {
        let scheduler = TaskScheduler::new(SchedulerConfig::default())
            .await
            .unwrap();

        scheduler
            .add_task("task-1", schedules::HOURLY, || async { Ok(()) })
            .await
            .unwrap();

        scheduler
            .add_task("task-2", schedules::DAILY_MIDNIGHT, || async { Ok(()) })
            .await
            .unwrap();

        let all_stats = scheduler.get_all_stats();
        assert_eq!(all_stats.len(), 2);

        scheduler.stop().await.unwrap();
    }

    #[tokio::test]
    async fn test_event_receiver() {
        let scheduler = TaskScheduler::new(SchedulerConfig::default())
            .await
            .unwrap();

        let mut receiver = scheduler.take_event_receiver().unwrap();

        scheduler
            .add_task("event-task", "* * * * * *", || async { Ok(()) })
            .await
            .unwrap();

        // Wait for event
        let event = tokio::time::timeout(Duration::from_secs(3), receiver.recv())
            .await
            .unwrap()
            .unwrap();

        assert_eq!(event.task_name, "event-task");
        assert!(event.success);

        scheduler.stop().await.unwrap();
    }

    #[test]
    fn test_task_builder() {
        let builder = TaskBuilder::new()
            .weather_schedule("0 */10 * * * *")
            .calendar_schedule("0 */5 * * * *");

        assert_eq!(builder.weather_cron, "0 */10 * * * *");
        assert_eq!(builder.calendar_cron, "0 */5 * * * *");
    }

    #[test]
    fn test_predefined_schedules() {
        // Validate all predefined cron expressions
        assert!(schedules::EVERY_MINUTE.parse::<cron::Schedule>().is_ok());
        assert!(schedules::EVERY_5_MINUTES.parse::<cron::Schedule>().is_ok());
        assert!(
            schedules::EVERY_15_MINUTES
                .parse::<cron::Schedule>()
                .is_ok()
        );
        assert!(
            schedules::EVERY_30_MINUTES
                .parse::<cron::Schedule>()
                .is_ok()
        );
        assert!(schedules::HOURLY.parse::<cron::Schedule>().is_ok());
        assert!(schedules::DAILY_MIDNIGHT.parse::<cron::Schedule>().is_ok());
        assert!(schedules::DAILY_6AM.parse::<cron::Schedule>().is_ok());
        assert!(schedules::DAILY_7AM.parse::<cron::Schedule>().is_ok());
        assert!(schedules::DAILY_8PM.parse::<cron::Schedule>().is_ok());
        assert!(schedules::WEEKLY.parse::<cron::Schedule>().is_ok());
        assert!(schedules::MONTHLY.parse::<cron::Schedule>().is_ok());
    }

    #[test]
    fn test_task_status_display() {
        assert_eq!(TaskStatus::Scheduled.to_string(), "scheduled");
        assert_eq!(TaskStatus::Running.to_string(), "running");
        assert_eq!(TaskStatus::Completed.to_string(), "completed");
        assert_eq!(TaskStatus::Failed.to_string(), "failed");
        assert_eq!(TaskStatus::Paused.to_string(), "paused");
    }

    #[test]
    fn test_scheduler_error_display() {
        let err = SchedulerError::InvalidCronExpression("bad cron".to_string());
        assert!(format!("{err}").contains("Invalid cron expression"));

        let err = SchedulerError::TaskNotFound("missing".to_string());
        assert!(format!("{err}").contains("Task not found"));

        let err = SchedulerError::StartupFailed("startup error".to_string());
        assert!(format!("{err}").contains("failed to start"));

        let err = SchedulerError::ExecutionFailed("exec error".to_string());
        assert!(format!("{err}").contains("execution failed"));

        let err = SchedulerError::NotRunning;
        assert!(format!("{err}").contains("not running"));

        let err = SchedulerError::Internal("internal error".to_string());
        assert!(format!("{err}").contains("Internal"));
    }

    #[test]
    fn test_scheduler_config_default() {
        let config = SchedulerConfig::default();
        assert!(config.auto_start);
        assert_eq!(config.max_concurrent_tasks, 10);
        assert_eq!(config.event_buffer_size, 100);
    }

    #[test]
    fn test_scheduler_config_custom() {
        let config = SchedulerConfig {
            auto_start: false,
            max_concurrent_tasks: 5,
            event_buffer_size: 50,
        };
        assert!(!config.auto_start);
        assert_eq!(config.max_concurrent_tasks, 5);
        assert_eq!(config.event_buffer_size, 50);
    }

    #[test]
    fn test_task_builder_all_methods() {
        let builder = TaskBuilder::new()
            .weather_schedule("0 */15 * * * *")
            .calendar_schedule("0 */10 * * * *")
            .morning_briefing_schedule("0 0 6 * * *")
            .evening_briefing_schedule("0 0 21 * * *")
            .backup_schedule("0 0 2 * * *");

        assert_eq!(builder.weather_cron, "0 */15 * * * *");
        assert_eq!(builder.calendar_cron, "0 */10 * * * *");
        assert_eq!(builder.morning_briefing_cron, "0 0 6 * * *");
        assert_eq!(builder.evening_briefing_cron, "0 0 21 * * *");
        assert_eq!(builder.backup_cron, "0 0 2 * * *");
    }

    #[test]
    fn test_task_builder_default() {
        let builder = TaskBuilder::default();
        assert_eq!(builder.weather_cron, schedules::EVERY_30_MINUTES);
        assert_eq!(builder.calendar_cron, schedules::EVERY_15_MINUTES);
        assert_eq!(builder.morning_briefing_cron, schedules::DAILY_7AM);
        assert_eq!(builder.evening_briefing_cron, schedules::DAILY_8PM);
        assert_eq!(builder.backup_cron, schedules::DAILY_MIDNIGHT);
    }

    #[test]
    fn test_task_builder_debug() {
        let builder = TaskBuilder::new();
        let debug = format!("{builder:?}");
        assert!(debug.contains("TaskBuilder"));
        assert!(debug.contains("weather_cron"));
    }

    #[tokio::test]
    async fn test_pause_nonexistent_task() {
        let scheduler = TaskScheduler::new(SchedulerConfig::default())
            .await
            .unwrap();

        let result = scheduler.pause_task("nonexistent");
        assert!(matches!(result, Err(SchedulerError::TaskNotFound(_))));

        scheduler.stop().await.unwrap();
    }

    #[tokio::test]
    async fn test_resume_nonexistent_task() {
        let scheduler = TaskScheduler::new(SchedulerConfig::default())
            .await
            .unwrap();

        let result = scheduler.resume_task("nonexistent");
        assert!(matches!(result, Err(SchedulerError::TaskNotFound(_))));

        scheduler.stop().await.unwrap();
    }

    #[tokio::test]
    async fn test_get_task_stats_nonexistent() {
        let scheduler = TaskScheduler::new(SchedulerConfig::default())
            .await
            .unwrap();

        let stats = scheduler.get_task_stats("nonexistent");
        assert!(stats.is_none());

        scheduler.stop().await.unwrap();
    }

    #[tokio::test]
    async fn test_scheduler_debug() {
        let scheduler = TaskScheduler::new(SchedulerConfig::default())
            .await
            .unwrap();

        let debug = format!("{scheduler:?}");
        assert!(debug.contains("TaskScheduler"));
        assert!(debug.contains("running"));
        assert!(debug.contains("task_count"));

        scheduler.stop().await.unwrap();
    }

    #[tokio::test]
    async fn test_start_already_running() {
        let scheduler = TaskScheduler::new(SchedulerConfig::default())
            .await
            .unwrap();

        // Already running from auto_start
        assert!(scheduler.is_running());

        // Starting again should be ok (no-op)
        scheduler.start().await.unwrap();
        assert!(scheduler.is_running());

        scheduler.stop().await.unwrap();
    }

    #[tokio::test]
    async fn test_stop_already_stopped() {
        let config = SchedulerConfig {
            auto_start: false,
            ..Default::default()
        };
        let scheduler = TaskScheduler::new(config).await.unwrap();

        // Already stopped
        assert!(!scheduler.is_running());

        // Stopping again should be ok (no-op)
        scheduler.stop().await.unwrap();
        assert!(!scheduler.is_running());
    }
}
