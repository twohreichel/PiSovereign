//! Hot-reloadable configuration support
//!
//! Provides SIGHUP signal handling for runtime configuration reload
//! without server restart.

use std::sync::Arc;

use arc_swap::ArcSwap;
use infrastructure::AppConfig;
use tokio::sync::watch;
use tracing::{error, info, warn};

/// A wrapper around `AppConfig` that supports atomic reload via SIGHUP
#[derive(Debug, Clone)]
pub struct ReloadableConfig {
    inner: Arc<ArcSwap<AppConfig>>,
    /// Notifier for config change events
    notify: watch::Sender<u64>,
    /// Receiver for config change events
    receiver: watch::Receiver<u64>,
}

impl ReloadableConfig {
    /// Create a new reloadable configuration
    #[must_use]
    pub fn new(config: AppConfig) -> Self {
        let (notify, receiver) = watch::channel(0);
        Self {
            inner: Arc::new(ArcSwap::new(Arc::new(config))),
            notify,
            receiver,
        }
    }

    /// Get the current configuration
    #[must_use]
    pub fn load(&self) -> Arc<AppConfig> {
        self.inner.load_full()
    }

    /// Reload configuration from disk
    ///
    /// Returns `true` if the reload was successful
    pub fn reload(&self) -> bool {
        match AppConfig::load() {
            Ok(new_config) => {
                let old_config = self.inner.swap(Arc::new(new_config));
                info!(
                    old_host = %old_config.server.host,
                    old_port = %old_config.server.port,
                    "Configuration reloaded successfully"
                );
                // Notify watchers (increment version)
                let version = *self.notify.borrow() + 1;
                if self.notify.send(version).is_err() {
                    warn!("No config change receivers active");
                }
                true
            },
            Err(e) => {
                error!("Failed to reload configuration: {}", e);
                false
            },
        }
    }

    /// Subscribe to configuration change notifications
    #[must_use]
    pub fn subscribe(&self) -> watch::Receiver<u64> {
        self.receiver.clone()
    }
}

/// Spawn a background task that listens for SIGHUP and reloads configuration
///
/// Returns a handle that can be used to manually trigger reloads
#[cfg(unix)]
pub fn spawn_config_reload_handler(config: ReloadableConfig) -> ReloadableConfig {
    use tokio::signal::unix::{SignalKind, signal};

    let config_clone = config.clone();
    tokio::spawn(async move {
        let mut sighup = match signal(SignalKind::hangup()) {
            Ok(s) => s,
            Err(e) => {
                error!("Failed to install SIGHUP handler: {}", e);
                return;
            },
        };

        loop {
            sighup.recv().await;
            info!("📥 Received SIGHUP, reloading configuration...");
            if config_clone.reload() {
                info!("✅ Configuration reload complete");
            } else {
                warn!("⚠️ Configuration reload failed, keeping previous config");
            }
        }
    });

    config
}

/// No-op on non-Unix systems
#[cfg(not(unix))]
pub fn spawn_config_reload_handler(config: ReloadableConfig) -> ReloadableConfig {
    warn!("SIGHUP config reload not supported on this platform");
    config
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reloadable_config_new() {
        let config = AppConfig::default();
        let reloadable = ReloadableConfig::new(config);

        let loaded = reloadable.load();
        assert_eq!(loaded.server.port, 3000);
    }

    #[test]
    fn reloadable_config_load_returns_current() {
        let mut config = AppConfig::default();
        config.server.port = 8080;
        let reloadable = ReloadableConfig::new(config);

        let loaded = reloadable.load();
        assert_eq!(loaded.server.port, 8080);
    }

    #[test]
    fn reloadable_config_subscribe() {
        let config = AppConfig::default();
        let reloadable = ReloadableConfig::new(config);

        let receiver = reloadable.subscribe();
        assert_eq!(*receiver.borrow(), 0);
    }

    #[tokio::test]
    async fn reloadable_config_change_notification() {
        let config = AppConfig::default();
        let reloadable = ReloadableConfig::new(config);
        let mut receiver = reloadable.subscribe();

        // Manually trigger internal notification (simulating reload)
        let version = *reloadable.notify.borrow() + 1;
        reloadable.notify.send(version).ok();

        // Wait for notification
        receiver.changed().await.ok();
        assert_eq!(*receiver.borrow(), 1);
    }
}
