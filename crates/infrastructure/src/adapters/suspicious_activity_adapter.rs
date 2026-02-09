//! In-memory suspicious activity tracker adapter
//!
//! This adapter implements the `SuspiciousActivityPort` trait for tracking
//! security violations and managing IP-based blocking.

use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::Arc;

use application::ports::{
    SuspiciousActivityConfig, SuspiciousActivityPort, ViolationRecord, ViolationSummary,
};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use domain::entities::ThreatLevel;
use parking_lot::RwLock;
use tracing::{debug, info, warn};

/// Internal record for tracking violations per IP
#[derive(Debug, Clone, Default)]
struct IpViolationData {
    violations: Vec<ViolationRecord>,
    blocked_until: Option<DateTime<Utc>>,
}

/// In-memory implementation of suspicious activity tracking
///
/// This adapter stores violation data in memory with automatic expiration.
/// For production use with multiple instances, consider a Redis-backed implementation.
#[derive(Debug)]
pub struct InMemorySuspiciousActivityTracker {
    data: Arc<RwLock<HashMap<IpAddr, IpViolationData>>>,
    config: SuspiciousActivityConfig,
}

impl InMemorySuspiciousActivityTracker {
    /// Create a new tracker with the given configuration
    #[must_use]
    pub fn new(config: SuspiciousActivityConfig) -> Self {
        Self {
            data: Arc::new(RwLock::new(HashMap::new())),
            config,
        }
    }

    /// Check if the IP should be blocked based on current violations
    #[allow(clippy::cast_possible_truncation)] // Violation count won't exceed u32::MAX
    #[allow(clippy::cast_possible_wrap)] // Duration seconds won't overflow
    fn should_block(
        &self,
        violations: &[ViolationRecord],
        new_violation: &ViolationRecord,
    ) -> bool {
        // Auto-block on critical threats
        if self.config.auto_block_on_critical && new_violation.threat_level == ThreatLevel::Critical
        {
            return true;
        }

        // Count violations in the current window
        let window_start =
            Utc::now() - chrono::Duration::seconds(self.config.violation_window_secs as i64);
        let violations_in_window = violations
            .iter()
            .filter(|v| v.timestamp >= window_start)
            .count() as u32
            + 1; // Include the new violation

        violations_in_window >= self.config.max_violations_before_block
    }

    /// Calculate the block expiration time
    #[allow(clippy::cast_possible_wrap)] // Duration seconds won't overflow in practice
    fn calculate_block_expiration(&self) -> DateTime<Utc> {
        Utc::now() + chrono::Duration::seconds(self.config.block_duration_secs as i64)
    }
}

impl Clone for InMemorySuspiciousActivityTracker {
    fn clone(&self) -> Self {
        Self {
            data: Arc::clone(&self.data),
            config: self.config.clone(),
        }
    }
}

impl Default for InMemorySuspiciousActivityTracker {
    fn default() -> Self {
        Self::new(SuspiciousActivityConfig::default())
    }
}

#[async_trait]
#[allow(clippy::cast_possible_truncation)] // Violation counts won't exceed u32::MAX
#[allow(clippy::cast_possible_wrap)] // Duration seconds won't overflow in practice
impl SuspiciousActivityPort for InMemorySuspiciousActivityTracker {
    async fn record_violation(&self, ip: IpAddr, violation: ViolationRecord) {
        let mut data = self.data.write();
        let entry = data.entry(ip).or_default();

        // Check if we should block
        let should_block = self.should_block(&entry.violations, &violation);

        // Add the violation
        entry.violations.push(violation.clone());

        // Block if threshold reached
        if should_block {
            let expires = self.calculate_block_expiration();
            entry.blocked_until = Some(expires);
            warn!(
                ip = %ip,
                category = %violation.category,
                threat_level = %violation.threat_level,
                expires_at = %expires,
                "IP blocked due to suspicious activity"
            );
        } else {
            debug!(
                ip = %ip,
                category = %violation.category,
                threat_level = %violation.threat_level,
                "Security violation recorded"
            );
        }
    }

    #[allow(clippy::option_if_let_else)] // Match is clearer here
    async fn get_violation_summary(&self, ip: IpAddr) -> ViolationSummary {
        let data = self.data.read();
        let now = Utc::now();

        match data.get(&ip) {
            Some(entry) => {
                let window_start =
                    now - chrono::Duration::seconds(self.config.violation_window_secs as i64);
                let violations_in_window = entry
                    .violations
                    .iter()
                    .filter(|v| v.timestamp >= window_start)
                    .count() as u32;

                let is_blocked = entry.blocked_until.is_some_and(|exp| exp > now);
                let block_expires_at = entry.blocked_until.filter(|exp| *exp > now);

                ViolationSummary {
                    total_violations: entry.violations.len() as u32,
                    violations_in_window,
                    is_blocked,
                    block_expires_at,
                    max_threat_level: entry.violations.iter().map(|v| v.threat_level).max(),
                    first_violation_at: entry.violations.first().map(|v| v.timestamp),
                    last_violation_at: entry.violations.last().map(|v| v.timestamp),
                }
            },
            None => ViolationSummary::default(),
        }
    }

    async fn is_blocked(&self, ip: IpAddr) -> bool {
        let data = self.data.read();
        data.get(&ip)
            .and_then(|entry| entry.blocked_until)
            .is_some_and(|exp| exp > Utc::now())
    }

    async fn block_ip(&self, ip: IpAddr, duration_secs: u64) {
        let mut data = self.data.write();
        let entry = data.entry(ip).or_default();
        let expires = Utc::now() + chrono::Duration::seconds(duration_secs as i64);
        entry.blocked_until = Some(expires);
        info!(ip = %ip, expires_at = %expires, "IP manually blocked");
    }

    async fn unblock_ip(&self, ip: IpAddr) {
        let mut data = self.data.write();
        if let Some(entry) = data.get_mut(&ip) {
            entry.blocked_until = None;
            info!(ip = %ip, "IP manually unblocked");
        }
    }

    async fn clear_violations(&self, ip: IpAddr) {
        let mut data = self.data.write();
        if let Some(entry) = data.get_mut(&ip) {
            entry.violations.clear();
            debug!(ip = %ip, "Violations cleared for IP");
        }
    }

    async fn get_blocked_ips(&self) -> Vec<IpAddr> {
        let data = self.data.read();
        let now = Utc::now();
        data.iter()
            .filter(|(_, entry)| entry.blocked_until.is_some_and(|exp| exp > now))
            .map(|(ip, _)| *ip)
            .collect()
    }

    async fn cleanup_expired(&self) {
        let mut data = self.data.write();
        let now = Utc::now();

        // Remove expired blocks
        for entry in data.values_mut() {
            if entry.blocked_until.is_some_and(|exp| exp <= now) {
                entry.blocked_until = None;
            }
        }

        // Remove old violations (keep violations for 24x the window for historical tracking)
        let retention_start =
            now - chrono::Duration::seconds(self.config.violation_window_secs as i64 * 24);
        for entry in data.values_mut() {
            entry.violations.retain(|v| v.timestamp >= retention_start);
        }

        // Remove entries with no violations and no active block
        data.retain(|_, entry| !entry.violations.is_empty() || entry.blocked_until.is_some());

        debug!("Cleaned up expired violations and blocks");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::Ipv4Addr;

    fn test_ip() -> IpAddr {
        IpAddr::V4(Ipv4Addr::new(192, 168, 1, 100))
    }

    fn test_config() -> SuspiciousActivityConfig {
        SuspiciousActivityConfig {
            max_violations_before_block: 3,
            violation_window_secs: 3600,
            block_duration_secs: 86400,
            auto_block_on_critical: true,
        }
    }

    #[tokio::test]
    async fn records_violations() {
        let tracker = InMemorySuspiciousActivityTracker::new(test_config());
        let ip = test_ip();

        tracker
            .record_violation(ip, ViolationRecord::new("test", ThreatLevel::Medium))
            .await;

        let summary = tracker.get_violation_summary(ip).await;
        assert_eq!(summary.total_violations, 1);
        assert_eq!(summary.violations_in_window, 1);
        assert!(!summary.is_blocked);
    }

    #[tokio::test]
    async fn blocks_after_threshold() {
        let tracker = InMemorySuspiciousActivityTracker::new(test_config());
        let ip = test_ip();

        // Record violations up to threshold
        for _ in 0..3 {
            tracker
                .record_violation(ip, ViolationRecord::new("test", ThreatLevel::Medium))
                .await;
        }

        assert!(tracker.is_blocked(ip).await);
        let summary = tracker.get_violation_summary(ip).await;
        assert!(summary.is_blocked);
        assert!(summary.block_expires_at.is_some());
    }

    #[tokio::test]
    async fn auto_blocks_on_critical() {
        let tracker = InMemorySuspiciousActivityTracker::new(test_config());
        let ip = test_ip();

        // Single critical violation should trigger block
        tracker
            .record_violation(ip, ViolationRecord::new("critical", ThreatLevel::Critical))
            .await;

        assert!(tracker.is_blocked(ip).await);
    }

    #[tokio::test]
    async fn no_auto_block_when_disabled() {
        let config = SuspiciousActivityConfig {
            auto_block_on_critical: false,
            ..test_config()
        };
        let tracker = InMemorySuspiciousActivityTracker::new(config);
        let ip = test_ip();

        tracker
            .record_violation(ip, ViolationRecord::new("critical", ThreatLevel::Critical))
            .await;

        // Should not be blocked yet (only 1 violation, threshold is 3)
        assert!(!tracker.is_blocked(ip).await);
    }

    #[tokio::test]
    async fn manual_block_unblock() {
        let tracker = InMemorySuspiciousActivityTracker::new(test_config());
        let ip = test_ip();

        tracker.block_ip(ip, 3600).await;
        assert!(tracker.is_blocked(ip).await);

        tracker.unblock_ip(ip).await;
        assert!(!tracker.is_blocked(ip).await);
    }

    #[tokio::test]
    async fn get_blocked_ips() {
        let tracker = InMemorySuspiciousActivityTracker::new(test_config());
        let ip1 = IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1));
        let ip2 = IpAddr::V4(Ipv4Addr::new(192, 168, 1, 2));

        tracker.block_ip(ip1, 3600).await;
        tracker.block_ip(ip2, 3600).await;

        let blocked = tracker.get_blocked_ips().await;
        assert_eq!(blocked.len(), 2);
        assert!(blocked.contains(&ip1));
        assert!(blocked.contains(&ip2));
    }

    #[tokio::test]
    async fn clear_violations() {
        let tracker = InMemorySuspiciousActivityTracker::new(test_config());
        let ip = test_ip();

        tracker
            .record_violation(ip, ViolationRecord::new("test", ThreatLevel::Medium))
            .await;
        tracker.clear_violations(ip).await;

        let summary = tracker.get_violation_summary(ip).await;
        assert_eq!(summary.total_violations, 0);
    }

    #[tokio::test]
    async fn tracks_max_threat_level() {
        let tracker = InMemorySuspiciousActivityTracker::new(test_config());
        let ip = test_ip();

        tracker
            .record_violation(ip, ViolationRecord::new("low", ThreatLevel::Low))
            .await;
        tracker
            .record_violation(ip, ViolationRecord::new("high", ThreatLevel::High))
            .await;
        tracker
            .record_violation(ip, ViolationRecord::new("medium", ThreatLevel::Medium))
            .await;

        let summary = tracker.get_violation_summary(ip).await;
        assert_eq!(summary.max_threat_level, Some(ThreatLevel::High));
    }

    #[tokio::test]
    async fn cleanup_removes_expired() {
        let tracker = InMemorySuspiciousActivityTracker::new(test_config());
        let ip = test_ip();

        // Block with 0 duration (expired immediately)
        tracker.block_ip(ip, 0).await;
        tracker.cleanup_expired().await;

        assert!(!tracker.is_blocked(ip).await);
    }

    #[tokio::test]
    async fn clone_shares_state() {
        let tracker = InMemorySuspiciousActivityTracker::new(test_config());
        let tracker_clone = tracker.clone();
        let ip = test_ip();

        tracker.block_ip(ip, 3600).await;
        assert!(tracker_clone.is_blocked(ip).await);
    }

    #[tokio::test]
    async fn default_config() {
        let tracker = InMemorySuspiciousActivityTracker::default();
        let ip = test_ip();

        // Default should have reasonable settings
        tracker
            .record_violation(ip, ViolationRecord::new("test", ThreatLevel::Medium))
            .await;
        // With default settings (3 violations), one violation should not block
        assert!(!tracker.is_blocked(ip).await);
    }

    #[tokio::test]
    async fn violation_summary_timestamps() {
        let tracker = InMemorySuspiciousActivityTracker::new(test_config());
        let ip = test_ip();

        tracker
            .record_violation(ip, ViolationRecord::new("first", ThreatLevel::Low))
            .await;

        let summary = tracker.get_violation_summary(ip).await;
        assert!(summary.first_violation_at.is_some());
        assert!(summary.last_violation_at.is_some());
    }

    #[tokio::test]
    async fn violation_record_with_details() {
        let tracker = InMemorySuspiciousActivityTracker::new(test_config());
        let ip = test_ip();

        let violation = ViolationRecord::new("injection", ThreatLevel::High)
            .with_details("Attempted to override system prompt");

        tracker.record_violation(ip, violation).await;

        let summary = tracker.get_violation_summary(ip).await;
        assert_eq!(summary.total_violations, 1);
    }
}
