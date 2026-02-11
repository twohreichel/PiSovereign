//! Factory functions for common scheduled tasks
//!
//! Provides pre-built task closures for the scheduler to run:
//! - Reminder checker (every minute)
//! - CalDAV sync (every 15 minutes)
//! - Morning briefing (daily at 7 AM)

use std::sync::Arc;

use application::{
    ports::ReminderPort,
    services::NotificationService,
};
use futures::future::BoxFuture;
use tracing::{debug, error, info};

/// Task name for the reminder checker
pub const REMINDER_CHECKER_TASK: &str = "reminder_checker";
/// Task name for the morning briefing
pub const MORNING_BRIEFING_TASK: &str = "morning_briefing";
/// Task name for CalDAV sync
pub const CALDAV_SYNC_TASK: &str = "caldav_sync";

/// Callback type for sending notifications
pub type NotificationCallback = Arc<dyn Fn(String) -> BoxFuture<'static, Result<(), String>> + Send + Sync>;

/// Create a reminder checker task closure
///
/// This task polls for due reminders and calls the callback with formatted messages.
/// Designed to run every minute.
pub fn create_reminder_checker_task<R: ReminderPort + 'static>(
    notification_service: Arc<NotificationService<R>>,
    send_callback: NotificationCallback,
) -> impl Fn() -> BoxFuture<'static, Result<(), String>> + Send + Sync + 'static
{
    move || {
        let service = Arc::clone(&notification_service);
        let callback = Arc::clone(&send_callback);

        Box::pin(async move {
            debug!("Checking for due reminders");

            match service.process_due_reminders().await {
                Ok(notifications) => {
                    if notifications.is_empty() {
                        debug!("No due reminders to process");
                        return Ok(());
                    }

                    info!(count = notifications.len(), "Processing due reminders");

                    for notification in notifications {
                        let reminder_id = notification.reminder.id.to_string();
                        debug!(reminder_id = %reminder_id, "Sending reminder notification");

                        if let Err(e) = callback(notification.message).await {
                            error!(reminder_id = %reminder_id, error = %e, "Failed to send notification");
                        } else {
                            info!(reminder_id = %reminder_id, "Notification sent successfully");
                        }
                    }
                    Ok(())
                },
                Err(e) => {
                    error!(error = %e, "Failed to process due reminders");
                    Err(format!("Reminder check failed: {e}"))
                },
            }
        })
    }
}

/// Morning briefing configuration
pub struct MorningBriefingConfig {
    /// Callback to generate the briefing content
    pub generate_briefing: Arc<dyn Fn() -> BoxFuture<'static, Result<String, String>> + Send + Sync>,
    /// Callback to send the briefing
    pub send_callback: NotificationCallback,
}

impl std::fmt::Debug for MorningBriefingConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MorningBriefingConfig")
            .finish_non_exhaustive()
    }
}

/// Create a morning briefing task closure
///
/// This task generates and sends a daily morning briefing.
/// Designed to run daily at 7 AM.
pub fn create_morning_briefing_task(
    config: MorningBriefingConfig,
) -> impl Fn() -> BoxFuture<'static, Result<(), String>> + Send + Sync + 'static
{
    move || {
        let generate = Arc::clone(&config.generate_briefing);
        let send = Arc::clone(&config.send_callback);

        Box::pin(async move {
            info!("Generating morning briefing");

            match generate().await {
                Ok(briefing) => {
                    if briefing.is_empty() {
                        debug!("No morning briefing content generated");
                        return Ok(());
                    }

                    if let Err(e) = send(briefing).await {
                        error!(error = %e, "Failed to send morning briefing");
                        return Err(format!("Failed to send briefing: {e}"));
                    }

                    info!("Morning briefing sent successfully");
                    Ok(())
                },
                Err(e) => {
                    error!(error = %e, "Failed to generate morning briefing");
                    Err(format!("Briefing generation failed: {e}"))
                },
            }
        })
    }
}

/// CalDAV sync callback type
pub type CalDavSyncCallback = Arc<dyn Fn() -> BoxFuture<'static, Result<u32, String>> + Send + Sync>;

/// Create a CalDAV sync task closure
///
/// This task synchronizes calendar events and creates reminders.
/// Designed to run every 15 minutes.
pub fn create_caldav_sync_task(
    sync_callback: CalDavSyncCallback,
) -> impl Fn() -> BoxFuture<'static, Result<(), String>> + Send + Sync + 'static
{
    move || {
        let callback = Arc::clone(&sync_callback);

        Box::pin(async move {
            debug!("Starting CalDAV sync");

            match callback().await {
                Ok(count) => {
                    info!(events_synced = count, "CalDAV sync completed");
                    Ok(())
                },
                Err(e) => {
                    error!(error = %e, "CalDAV sync failed");
                    Err(format!("CalDAV sync failed: {e}"))
                },
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    // Skip reminder_checker test as it requires the mock port
    // which is not available from the infrastructure crate.
    // This functionality should be tested in integration tests.

    #[tokio::test]
    async fn morning_briefing_sends_content() {
        let sent = Arc::new(AtomicUsize::new(0));
        let sent_clone = Arc::clone(&sent);

        let config = MorningBriefingConfig {
            generate_briefing: Arc::new(|| {
                Box::pin(async { Ok("Good morning!".to_string()) })
            }),
            send_callback: Arc::new(move |_msg| {
                let count = Arc::clone(&sent_clone);
                Box::pin(async move {
                    count.fetch_add(1, Ordering::SeqCst);
                    Ok(())
                })
            }),
        };

        let task = create_morning_briefing_task(config);
        let result = task().await;

        assert!(result.is_ok());
        assert_eq!(sent.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn morning_briefing_handles_empty_content() {
        let sent = Arc::new(AtomicUsize::new(0));
        let sent_clone = Arc::clone(&sent);

        let config = MorningBriefingConfig {
            generate_briefing: Arc::new(|| Box::pin(async { Ok(String::new()) })),
            send_callback: Arc::new(move |_msg| {
                let count = Arc::clone(&sent_clone);
                Box::pin(async move {
                    count.fetch_add(1, Ordering::SeqCst);
                    Ok(())
                })
            }),
        };

        let task = create_morning_briefing_task(config);
        let result = task().await;

        assert!(result.is_ok());
        // Should not send when content is empty
        assert_eq!(sent.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn morning_briefing_handles_generation_error() {
        let config = MorningBriefingConfig {
            generate_briefing: Arc::new(|| {
                Box::pin(async { Err("API unavailable".to_string()) })
            }),
            send_callback: Arc::new(move |_msg| Box::pin(async { Ok(()) })),
        };

        let task = create_morning_briefing_task(config);
        let result = task().await;

        assert!(result.is_err());
        assert!(result.unwrap_err().contains("generation failed"));
    }

    #[tokio::test]
    async fn caldav_sync_runs_callback() {
        let sync_count = Arc::new(AtomicUsize::new(0));
        let sync_clone = Arc::clone(&sync_count);

        let callback: CalDavSyncCallback = Arc::new(move || {
            let count = Arc::clone(&sync_clone);
            Box::pin(async move {
                count.fetch_add(1, Ordering::SeqCst);
                Ok(5)
            })
        });

        let task = create_caldav_sync_task(callback);
        let result = task().await;

        assert!(result.is_ok());
        assert_eq!(sync_count.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn caldav_sync_handles_error() {
        let callback: CalDavSyncCallback =
            Arc::new(|| Box::pin(async { Err("Connection failed".to_string()) }));

        let task = create_caldav_sync_task(callback);
        let result = task().await;

        assert!(result.is_err());
        assert!(result.unwrap_err().contains("CalDAV sync failed"));
    }

    #[test]
    fn task_names_are_unique() {
        assert_ne!(REMINDER_CHECKER_TASK, MORNING_BRIEFING_TASK);
        assert_ne!(MORNING_BRIEFING_TASK, CALDAV_SYNC_TASK);
        assert_ne!(REMINDER_CHECKER_TASK, CALDAV_SYNC_TASK);
    }

    #[test]
    fn morning_briefing_config_has_debug() {
        let config = MorningBriefingConfig {
            generate_briefing: Arc::new(|| Box::pin(async { Ok(String::new()) })),
            send_callback: Arc::new(|_| Box::pin(async { Ok(()) })),
        };
        let debug_str = format!("{config:?}");
        assert!(debug_str.contains("MorningBriefingConfig"));
    }
}
