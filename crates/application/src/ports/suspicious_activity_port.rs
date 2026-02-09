//! Port for tracking suspicious activity and managing security violations
//!
//! This port defines the interface for tracking security violations per user/IP,
//! enabling automatic blocking after reaching violation thresholds.

use std::net::IpAddr;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use domain::entities::ThreatLevel;
use serde::{Deserialize, Serialize};

/// Record of a security violation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ViolationRecord {
    /// When the violation occurred
    pub timestamp: DateTime<Utc>,
    /// Category of the violation
    pub category: String,
    /// Severity level
    pub threat_level: ThreatLevel,
    /// Additional details
    pub details: Option<String>,
}

impl ViolationRecord {
    /// Create a new violation record
    #[must_use]
    pub fn new(category: impl Into<String>, threat_level: ThreatLevel) -> Self {
        Self {
            timestamp: Utc::now(),
            category: category.into(),
            threat_level,
            details: None,
        }
    }

    /// Add details to the violation record
    #[must_use]
    pub fn with_details(mut self, details: impl Into<String>) -> Self {
        self.details = Some(details.into());
        self
    }
}

/// Summary of violations for an identifier
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ViolationSummary {
    /// Total number of violations
    pub total_violations: u32,
    /// Number of violations in the current window
    pub violations_in_window: u32,
    /// Whether the identifier is currently blocked
    pub is_blocked: bool,
    /// When the block expires (if blocked)
    pub block_expires_at: Option<DateTime<Utc>>,
    /// Highest threat level seen
    pub max_threat_level: Option<ThreatLevel>,
    /// First violation timestamp
    pub first_violation_at: Option<DateTime<Utc>>,
    /// Last violation timestamp
    pub last_violation_at: Option<DateTime<Utc>>,
}

/// Configuration for suspicious activity tracking
#[derive(Debug, Clone)]
pub struct SuspiciousActivityConfig {
    /// Maximum violations before blocking
    pub max_violations_before_block: u32,
    /// Time window for counting violations (in seconds)
    pub violation_window_secs: u64,
    /// How long to block an IP after exceeding threshold (in seconds)
    pub block_duration_secs: u64,
    /// Whether to auto-block on critical threats
    pub auto_block_on_critical: bool,
}

impl Default for SuspiciousActivityConfig {
    fn default() -> Self {
        Self {
            max_violations_before_block: 3,
            violation_window_secs: 3600,      // 1 hour
            block_duration_secs: 86400,       // 24 hours
            auto_block_on_critical: true,
        }
    }
}

/// Port for tracking suspicious activity
#[async_trait]
pub trait SuspiciousActivityPort: Send + Sync {
    /// Record a security violation for an IP address
    async fn record_violation(&self, ip: IpAddr, violation: ViolationRecord);

    /// Get the violation summary for an IP address
    async fn get_violation_summary(&self, ip: IpAddr) -> ViolationSummary;

    /// Check if an IP address is currently blocked
    async fn is_blocked(&self, ip: IpAddr) -> bool;

    /// Manually block an IP address
    async fn block_ip(&self, ip: IpAddr, duration_secs: u64);

    /// Manually unblock an IP address
    async fn unblock_ip(&self, ip: IpAddr);

    /// Clear violations for an IP address (does not unblock)
    async fn clear_violations(&self, ip: IpAddr);

    /// Get all currently blocked IPs
    async fn get_blocked_ips(&self) -> Vec<IpAddr>;

    /// Cleanup expired blocks and old violations
    async fn cleanup_expired(&self);
}

#[cfg(test)]
pub use mock::MockSuspiciousActivityPort;

#[cfg(test)]
mod mock {
    use super::*;
    use parking_lot::Mutex;
    use std::collections::HashMap;
    use std::sync::Arc;

    /// Mock implementation for testing
    #[derive(Debug, Clone, Default)]
    pub struct MockSuspiciousActivityPort {
        violations: Arc<Mutex<HashMap<IpAddr, Vec<ViolationRecord>>>>,
        blocked: Arc<Mutex<HashMap<IpAddr, DateTime<Utc>>>>,
        config: SuspiciousActivityConfig,
    }

    impl MockSuspiciousActivityPort {
        #[must_use]
        pub fn new() -> Self {
            Self::default()
        }

        #[must_use]
        pub fn with_config(config: SuspiciousActivityConfig) -> Self {
            Self {
                violations: Arc::new(Mutex::new(HashMap::new())),
                blocked: Arc::new(Mutex::new(HashMap::new())),
                config,
            }
        }

        pub fn get_violations(&self, ip: IpAddr) -> Vec<ViolationRecord> {
            self.violations.lock().get(&ip).cloned().unwrap_or_default()
        }
    }

    #[async_trait]
    impl SuspiciousActivityPort for MockSuspiciousActivityPort {
        async fn record_violation(&self, ip: IpAddr, violation: ViolationRecord) {
            let mut violations = self.violations.lock();
            let entry = violations.entry(ip).or_default();
            entry.push(violation.clone());

            // Check if we should auto-block
            let violations_in_window: u32 = entry
                .iter()
                .filter(|v| {
                    let window_start =
                        Utc::now() - chrono::Duration::seconds(self.config.violation_window_secs as i64);
                    v.timestamp >= window_start
                })
                .count() as u32;

            if violations_in_window >= self.config.max_violations_before_block
                || (self.config.auto_block_on_critical && violation.threat_level == ThreatLevel::Critical)
            {
                drop(violations);
                let expires = Utc::now() + chrono::Duration::seconds(self.config.block_duration_secs as i64);
                self.blocked.lock().insert(ip, expires);
            }
        }

        async fn get_violation_summary(&self, ip: IpAddr) -> ViolationSummary {
            let violations = self.violations.lock();
            let blocked = self.blocked.lock();

            let records = violations.get(&ip);
            let is_blocked = blocked.get(&ip).is_some_and(|&exp| exp > Utc::now());
            let block_expires_at = blocked.get(&ip).filter(|&&exp| exp > Utc::now()).copied();

            match records {
                Some(recs) if !recs.is_empty() => {
                    let window_start =
                        Utc::now() - chrono::Duration::seconds(self.config.violation_window_secs as i64);
                    let violations_in_window = recs.iter().filter(|v| v.timestamp >= window_start).count() as u32;

                    ViolationSummary {
                        total_violations: recs.len() as u32,
                        violations_in_window,
                        is_blocked,
                        block_expires_at,
                        max_threat_level: recs.iter().map(|v| v.threat_level).max(),
                        first_violation_at: recs.first().map(|v| v.timestamp),
                        last_violation_at: recs.last().map(|v| v.timestamp),
                    }
                }
                _ => ViolationSummary {
                    is_blocked,
                    block_expires_at,
                    ..Default::default()
                },
            }
        }

        async fn is_blocked(&self, ip: IpAddr) -> bool {
            self.blocked.lock().get(&ip).is_some_and(|&exp| exp > Utc::now())
        }

        async fn block_ip(&self, ip: IpAddr, duration_secs: u64) {
            let expires = Utc::now() + chrono::Duration::seconds(duration_secs as i64);
            self.blocked.lock().insert(ip, expires);
        }

        async fn unblock_ip(&self, ip: IpAddr) {
            self.blocked.lock().remove(&ip);
        }

        async fn clear_violations(&self, ip: IpAddr) {
            self.violations.lock().remove(&ip);
        }

        async fn get_blocked_ips(&self) -> Vec<IpAddr> {
            let now = Utc::now();
            self.blocked
                .lock()
                .iter()
                .filter(|(_, exp)| **exp > now)
                .map(|(ip, _)| *ip)
                .collect()
        }

        async fn cleanup_expired(&self) {
            let now = Utc::now();
            self.blocked.lock().retain(|_, exp| *exp > now);

            let window_start =
                now - chrono::Duration::seconds(self.config.violation_window_secs as i64 * 24);
            self.violations.lock().iter_mut().for_each(|(_, recs)| {
                recs.retain(|v| v.timestamp >= window_start);
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::Ipv4Addr;

    fn test_ip() -> IpAddr {
        IpAddr::V4(Ipv4Addr::new(192, 168, 1, 100))
    }

    #[tokio::test]
    async fn violation_record_creation() {
        let record = ViolationRecord::new("prompt_injection", ThreatLevel::High)
            .with_details("Attempted to override instructions");

        assert_eq!(record.category, "prompt_injection");
        assert_eq!(record.threat_level, ThreatLevel::High);
        assert!(record.details.is_some());
    }

    #[tokio::test]
    async fn mock_records_violations() {
        let port = MockSuspiciousActivityPort::new();
        let ip = test_ip();

        port.record_violation(ip, ViolationRecord::new("test", ThreatLevel::Medium))
            .await;

        let summary = port.get_violation_summary(ip).await;
        assert_eq!(summary.total_violations, 1);
        assert_eq!(summary.violations_in_window, 1);
    }

    #[tokio::test]
    async fn mock_blocks_after_threshold() {
        let config = SuspiciousActivityConfig {
            max_violations_before_block: 2,
            ..Default::default()
        };
        let port = MockSuspiciousActivityPort::with_config(config);
        let ip = test_ip();

        port.record_violation(ip, ViolationRecord::new("test", ThreatLevel::Medium))
            .await;
        assert!(!port.is_blocked(ip).await);

        port.record_violation(ip, ViolationRecord::new("test", ThreatLevel::Medium))
            .await;
        assert!(port.is_blocked(ip).await);
    }

    #[tokio::test]
    async fn mock_auto_blocks_on_critical() {
        let config = SuspiciousActivityConfig {
            auto_block_on_critical: true,
            ..Default::default()
        };
        let port = MockSuspiciousActivityPort::with_config(config);
        let ip = test_ip();

        port.record_violation(ip, ViolationRecord::new("critical", ThreatLevel::Critical))
            .await;
        assert!(port.is_blocked(ip).await);
    }

    #[tokio::test]
    async fn mock_manual_block_unblock() {
        let port = MockSuspiciousActivityPort::new();
        let ip = test_ip();

        port.block_ip(ip, 3600).await;
        assert!(port.is_blocked(ip).await);

        port.unblock_ip(ip).await;
        assert!(!port.is_blocked(ip).await);
    }

    #[tokio::test]
    async fn mock_get_blocked_ips() {
        let port = MockSuspiciousActivityPort::new();
        let ip1 = IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1));
        let ip2 = IpAddr::V4(Ipv4Addr::new(192, 168, 1, 2));

        port.block_ip(ip1, 3600).await;
        port.block_ip(ip2, 3600).await;

        let blocked = port.get_blocked_ips().await;
        assert_eq!(blocked.len(), 2);
        assert!(blocked.contains(&ip1));
        assert!(blocked.contains(&ip2));
    }

    #[tokio::test]
    async fn mock_clear_violations() {
        let port = MockSuspiciousActivityPort::new();
        let ip = test_ip();

        port.record_violation(ip, ViolationRecord::new("test", ThreatLevel::Medium))
            .await;
        port.clear_violations(ip).await;

        let summary = port.get_violation_summary(ip).await;
        assert_eq!(summary.total_violations, 0);
    }

    #[tokio::test]
    async fn violation_summary_tracks_threat_levels() {
        let port = MockSuspiciousActivityPort::new();
        let ip = test_ip();

        port.record_violation(ip, ViolationRecord::new("low", ThreatLevel::Low))
            .await;
        port.record_violation(ip, ViolationRecord::new("high", ThreatLevel::High))
            .await;
        port.record_violation(ip, ViolationRecord::new("medium", ThreatLevel::Medium))
            .await;

        let summary = port.get_violation_summary(ip).await;
        assert_eq!(summary.max_threat_level, Some(ThreatLevel::High));
    }

    #[test]
    fn config_default_values() {
        let config = SuspiciousActivityConfig::default();
        assert_eq!(config.max_violations_before_block, 3);
        assert_eq!(config.violation_window_secs, 3600);
        assert_eq!(config.block_duration_secs, 86400);
        assert!(config.auto_block_on_critical);
    }
}
