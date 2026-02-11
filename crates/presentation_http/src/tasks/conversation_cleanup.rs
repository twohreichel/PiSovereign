//! Conversation retention cleanup task
//!
//! Periodically removes messenger conversations older than the configured retention period.

use std::sync::Arc;
use std::time::Duration;

use application::ports::ConversationStore;
use chrono::Utc;
use tracing::{debug, error, info};

/// Default cleanup interval: once per hour
const DEFAULT_CLEANUP_INTERVAL_SECS: u64 = 3600;

/// Spawn a background task that periodically cleans up old messenger conversations.
///
/// The task will run at the specified interval and remove conversations older
/// than `retention_days`.
///
/// Returns a `JoinHandle` that can be used to abort the task when shutting down.
///
/// # Arguments
///
/// * `conversation_store` - The conversation store to clean
/// * `retention_days` - Remove conversations older than this many days
/// * `cleanup_interval` - How often to run the cleanup (defaults to 1 hour if None)
///
/// # Example
///
/// ```ignore
/// let cleanup_handle = spawn_conversation_cleanup_task(
///     conversation_store,
///     30, // 30 days retention
///     None, // Use default interval
/// );
///
/// // On shutdown:
/// cleanup_handle.abort();
/// ```
pub fn spawn_conversation_cleanup_task(
    conversation_store: Arc<dyn ConversationStore>,
    retention_days: u32,
    cleanup_interval: Option<Duration>,
) -> tokio::task::JoinHandle<()> {
    let interval = cleanup_interval.unwrap_or(Duration::from_secs(DEFAULT_CLEANUP_INTERVAL_SECS));

    info!(
        retention_days = retention_days,
        interval_secs = interval.as_secs(),
        "Starting conversation cleanup task"
    );

    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(interval);
        // Don't run immediately on startup
        ticker.tick().await;

        loop {
            ticker.tick().await;

            let cutoff = Utc::now() - chrono::Duration::days(i64::from(retention_days));

            debug!(
                cutoff = %cutoff,
                retention_days = retention_days,
                "Running conversation cleanup"
            );

            match conversation_store.cleanup_older_than(cutoff).await {
                Ok(removed) => {
                    if removed > 0 {
                        info!(
                            removed_count = removed,
                            retention_days = retention_days,
                            "Cleaned up old messenger conversations"
                        );
                    } else {
                        debug!("No conversations to clean up");
                    }
                },
                Err(e) => {
                    error!(
                        error = %e,
                        "Failed to clean up old conversations"
                    );
                },
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use application::error::ApplicationError;
    use async_trait::async_trait;
    use chrono::DateTime;
    use domain::ChatMessage;
    use domain::entities::Conversation;
    use domain::entities::ConversationSource;
    use domain::value_objects::ConversationId;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use tokio::sync::RwLock;

    struct MockConversationStore {
        cleanup_calls: AtomicUsize,
        cleanup_results: RwLock<Vec<usize>>,
    }

    impl MockConversationStore {
        fn new(results: Vec<usize>) -> Self {
            Self {
                cleanup_calls: AtomicUsize::new(0),
                cleanup_results: RwLock::new(results),
            }
        }

        fn cleanup_call_count(&self) -> usize {
            self.cleanup_calls.load(Ordering::SeqCst)
        }
    }

    #[async_trait]
    impl ConversationStore for MockConversationStore {
        async fn save(&self, _: &Conversation) -> Result<(), ApplicationError> {
            Ok(())
        }

        async fn get(&self, _: &ConversationId) -> Result<Option<Conversation>, ApplicationError> {
            Ok(None)
        }

        async fn get_by_phone_number(
            &self,
            _: ConversationSource,
            _: &str,
        ) -> Result<Option<Conversation>, ApplicationError> {
            Ok(None)
        }

        async fn update(&self, _: &Conversation) -> Result<(), ApplicationError> {
            Ok(())
        }

        async fn delete(&self, _: &ConversationId) -> Result<(), ApplicationError> {
            Ok(())
        }

        async fn add_message(
            &self,
            _: &ConversationId,
            _: &ChatMessage,
        ) -> Result<(), ApplicationError> {
            Ok(())
        }

        async fn list_recent(&self, _: usize) -> Result<Vec<Conversation>, ApplicationError> {
            Ok(vec![])
        }

        async fn search(&self, _: &str, _: usize) -> Result<Vec<Conversation>, ApplicationError> {
            Ok(vec![])
        }

        async fn cleanup_older_than(&self, _: DateTime<Utc>) -> Result<usize, ApplicationError> {
            self.cleanup_calls.fetch_add(1, Ordering::SeqCst);
            let mut results = self.cleanup_results.write().await;
            Ok(results.pop().unwrap_or(0))
        }
    }

    #[tokio::test]
    async fn cleanup_task_calls_cleanup_periodically() {
        let store = Arc::new(MockConversationStore::new(vec![5, 3, 0]));

        // Use a very short interval for testing
        let handle =
            spawn_conversation_cleanup_task(store.clone(), 30, Some(Duration::from_millis(50)));

        // Wait for a few cleanup cycles
        tokio::time::sleep(Duration::from_millis(200)).await;

        // Abort the task
        handle.abort();

        // Should have been called at least once
        assert!(store.cleanup_call_count() >= 1);
    }

    #[tokio::test]
    async fn cleanup_task_can_be_aborted() {
        let store = Arc::new(MockConversationStore::new(vec![]));

        let handle = spawn_conversation_cleanup_task(
            store,
            30,
            Some(Duration::from_secs(3600)), // Long interval
        );

        // Should be able to abort immediately
        handle.abort();

        // Task should finish
        let result = handle.await;
        assert!(result.is_err()); // JoinError indicates abort
    }
}
