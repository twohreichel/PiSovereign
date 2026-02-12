//! SQLite suspicious activity tracker
//!
//! Implements the `SuspiciousActivityPort` for persistent security violation
//! tracking using SQLite. This enables violation data to survive restarts
//! and be shared across multiple instances.

use std::net::IpAddr;

use application::ports::{
    SuspiciousActivityConfig, SuspiciousActivityPort, ViolationRecord, ViolationSummary,
};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use domain::entities::ThreatLevel;
use sqlx::SqlitePool;
use tracing::{debug, info, warn};

/// SQLite-backed suspicious activity tracker
///
/// Persists violation records and IP blocks to SQLite for durability
/// across restarts and multi-instance deployments.
#[derive(Debug, Clone)]
pub struct SqliteSuspiciousActivityTracker {
    pool: SqlitePool,
    config: SuspiciousActivityConfig,
}

impl SqliteSuspiciousActivityTracker {
    /// Create a new SQLite-backed tracker
    #[must_use]
    pub fn new(pool: SqlitePool, config: SuspiciousActivityConfig) -> Self {
        Self { pool, config }
    }

    /// Check if the IP should be blocked based on current violations
    #[allow(clippy::cast_possible_wrap)]
    async fn should_block(&self, ip: IpAddr, new_violation: &ViolationRecord) -> bool {
        // Auto-block on critical threats
        if self.config.auto_block_on_critical && new_violation.threat_level == ThreatLevel::Critical
        {
            return true;
        }

        // Count violations in the current window
        let window_start =
            Utc::now() - chrono::Duration::seconds(self.config.violation_window_secs as i64);
        let ip_str = ip.to_string();

        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM security_violations WHERE ip = $1 AND created_at >= $2",
        )
        .bind(&ip_str)
        .bind(window_start.to_rfc3339())
        .fetch_one(&self.pool)
        .await
        .unwrap_or(0);

        // +1 for the new violation being recorded
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let violations_in_window = (count as u32) + 1;
        violations_in_window >= self.config.max_violations_before_block
    }

    /// Calculate the block expiration time
    #[allow(clippy::cast_possible_wrap)]
    fn calculate_block_expiration(&self) -> DateTime<Utc> {
        Utc::now() + chrono::Duration::seconds(self.config.block_duration_secs as i64)
    }
}

#[async_trait]
impl SuspiciousActivityPort for SqliteSuspiciousActivityTracker {
    async fn record_violation(&self, ip: IpAddr, violation: ViolationRecord) {
        let ip_str = ip.to_string();
        let should_block = self.should_block(ip, &violation).await;

        // Insert the violation record
        if let Err(e) = sqlx::query(
            "INSERT INTO security_violations (ip, category, threat_level, details, created_at)
             VALUES ($1, $2, $3, $4, $5)",
        )
        .bind(&ip_str)
        .bind(&violation.category)
        .bind(violation.threat_level.to_string())
        .bind(&violation.details)
        .bind(violation.timestamp.to_rfc3339())
        .execute(&self.pool)
        .await
        {
            warn!(error = %e, ip = %ip, "Failed to record security violation");
            return;
        }

        // Block if threshold reached
        if should_block {
            let expires = self.calculate_block_expiration();
            if let Err(e) = sqlx::query(
                "INSERT INTO ip_blocks (ip, blocked_until) VALUES ($1, $2)
                 ON CONFLICT(ip) DO UPDATE SET blocked_until = excluded.blocked_until",
            )
            .bind(&ip_str)
            .bind(expires.to_rfc3339())
            .execute(&self.pool)
            .await
            {
                warn!(error = %e, ip = %ip, "Failed to block IP");
                return;
            }
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

    #[allow(clippy::cast_possible_truncation)]
    async fn get_violation_summary(&self, ip: IpAddr) -> ViolationSummary {
        let ip_str = ip.to_string();
        let now = Utc::now();

        // Total violations
        let total: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM security_violations WHERE ip = $1")
                .bind(&ip_str)
                .fetch_one(&self.pool)
                .await
                .unwrap_or(0);

        if total == 0 {
            return ViolationSummary::default();
        }

        // Violations in window
        #[allow(clippy::cast_possible_wrap)]
        let window_start =
            now - chrono::Duration::seconds(self.config.violation_window_secs as i64);
        let in_window: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM security_violations WHERE ip = $1 AND created_at >= $2",
        )
        .bind(&ip_str)
        .bind(window_start.to_rfc3339())
        .fetch_one(&self.pool)
        .await
        .unwrap_or(0);

        // Block status
        let block_row: Option<(String,)> =
            sqlx::query_as("SELECT blocked_until FROM ip_blocks WHERE ip = $1")
                .bind(&ip_str)
                .fetch_optional(&self.pool)
                .await
                .unwrap_or(None);

        let block_expires_at = block_row.and_then(|(ts,)| {
            DateTime::parse_from_rfc3339(&ts)
                .ok()
                .map(|dt| dt.with_timezone(&Utc))
                .filter(|exp| *exp > now)
        });
        let is_blocked = block_expires_at.is_some();

        // Max threat level via severity-ordered query
        let max_threat_row: Option<(String,)> = sqlx::query_as(
            "SELECT threat_level FROM security_violations WHERE ip = $1
             ORDER BY CASE threat_level
               WHEN 'critical' THEN 4 WHEN 'high' THEN 3
               WHEN 'medium' THEN 2 WHEN 'low' THEN 1 ELSE 0
             END DESC LIMIT 1",
        )
        .bind(&ip_str)
        .fetch_optional(&self.pool)
        .await
        .unwrap_or(None);

        let max_threat_level = max_threat_row.and_then(|(tl,)| tl.parse().ok());

        // First and last violation timestamps
        let timestamps: Option<(String, String)> = sqlx::query_as(
            "SELECT MIN(created_at), MAX(created_at)
             FROM security_violations WHERE ip = $1",
        )
        .bind(&ip_str)
        .fetch_optional(&self.pool)
        .await
        .unwrap_or(None);

        let (first_violation_at, last_violation_at) =
            timestamps.map_or((None, None), |(first, last)| {
                (
                    DateTime::parse_from_rfc3339(&first)
                        .ok()
                        .map(|dt| dt.with_timezone(&Utc)),
                    DateTime::parse_from_rfc3339(&last)
                        .ok()
                        .map(|dt| dt.with_timezone(&Utc)),
                )
            });

        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        ViolationSummary {
            total_violations: total as u32,
            violations_in_window: in_window as u32,
            is_blocked,
            block_expires_at,
            max_threat_level,
            first_violation_at,
            last_violation_at,
        }
    }

    async fn is_blocked(&self, ip: IpAddr) -> bool {
        let ip_str = ip.to_string();
        let now = Utc::now().to_rfc3339();

        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM ip_blocks WHERE ip = $1 AND blocked_until > $2",
        )
        .bind(&ip_str)
        .bind(&now)
        .fetch_one(&self.pool)
        .await
        .unwrap_or(0);

        count > 0
    }

    #[allow(clippy::cast_possible_wrap)]
    async fn block_ip(&self, ip: IpAddr, duration_secs: u64) {
        let ip_str = ip.to_string();
        let expires = Utc::now() + chrono::Duration::seconds(duration_secs as i64);

        if let Err(e) = sqlx::query(
            "INSERT INTO ip_blocks (ip, blocked_until) VALUES ($1, $2)
             ON CONFLICT(ip) DO UPDATE SET blocked_until = excluded.blocked_until",
        )
        .bind(&ip_str)
        .bind(expires.to_rfc3339())
        .execute(&self.pool)
        .await
        {
            warn!(error = %e, ip = %ip, "Failed to block IP");
            return;
        }

        info!(ip = %ip, expires_at = %expires, "IP manually blocked");
    }

    async fn unblock_ip(&self, ip: IpAddr) {
        let ip_str = ip.to_string();

        if let Err(e) = sqlx::query("DELETE FROM ip_blocks WHERE ip = $1")
            .bind(&ip_str)
            .execute(&self.pool)
            .await
        {
            warn!(error = %e, ip = %ip, "Failed to unblock IP");
            return;
        }

        info!(ip = %ip, "IP manually unblocked");
    }

    async fn clear_violations(&self, ip: IpAddr) {
        let ip_str = ip.to_string();

        if let Err(e) = sqlx::query("DELETE FROM security_violations WHERE ip = $1")
            .bind(&ip_str)
            .execute(&self.pool)
            .await
        {
            warn!(error = %e, ip = %ip, "Failed to clear violations");
            return;
        }

        debug!(ip = %ip, "Violations cleared for IP");
    }

    async fn get_blocked_ips(&self) -> Vec<IpAddr> {
        let now = Utc::now().to_rfc3339();

        let rows: Vec<(String,)> =
            sqlx::query_as("SELECT ip FROM ip_blocks WHERE blocked_until > $1")
                .bind(&now)
                .fetch_all(&self.pool)
                .await
                .unwrap_or_default();

        rows.into_iter()
            .filter_map(|(ip_str,)| ip_str.parse().ok())
            .collect()
    }

    #[allow(clippy::cast_possible_wrap)]
    async fn cleanup_expired(&self) {
        let now = Utc::now();

        // Remove expired blocks
        if let Err(e) = sqlx::query("DELETE FROM ip_blocks WHERE blocked_until <= $1")
            .bind(now.to_rfc3339())
            .execute(&self.pool)
            .await
        {
            warn!(error = %e, "Failed to cleanup expired blocks");
        }

        // Remove old violations (keep for 24x the window for historical tracking)
        let retention_start =
            now - chrono::Duration::seconds(self.config.violation_window_secs as i64 * 24);
        if let Err(e) = sqlx::query("DELETE FROM security_violations WHERE created_at < $1")
            .bind(retention_start.to_rfc3339())
            .execute(&self.pool)
            .await
        {
            warn!(error = %e, "Failed to cleanup old violations");
        }

        debug!("Cleaned up expired violations and blocks");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::persistence::async_connection::AsyncDatabase;
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

    async fn setup() -> SqliteSuspiciousActivityTracker {
        let db = AsyncDatabase::in_memory().await.unwrap();
        db.migrate().await.unwrap();
        SqliteSuspiciousActivityTracker::new(db.pool().clone(), test_config())
    }

    #[tokio::test]
    async fn records_violations() {
        let tracker = setup().await;
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
        let tracker = setup().await;
        let ip = test_ip();

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
        let tracker = setup().await;
        let ip = test_ip();

        tracker
            .record_violation(ip, ViolationRecord::new("critical", ThreatLevel::Critical))
            .await;

        assert!(tracker.is_blocked(ip).await);
    }

    #[tokio::test]
    async fn manual_block_and_unblock() {
        let tracker = setup().await;
        let ip = test_ip();

        tracker.block_ip(ip, 3600).await;
        assert!(tracker.is_blocked(ip).await);

        tracker.unblock_ip(ip).await;
        assert!(!tracker.is_blocked(ip).await);
    }

    #[tokio::test]
    async fn clear_violations_removes_records() {
        let tracker = setup().await;
        let ip = test_ip();

        tracker
            .record_violation(ip, ViolationRecord::new("test", ThreatLevel::Low))
            .await;

        let summary = tracker.get_violation_summary(ip).await;
        assert_eq!(summary.total_violations, 1);

        tracker.clear_violations(ip).await;

        let summary = tracker.get_violation_summary(ip).await;
        assert_eq!(summary.total_violations, 0);
    }

    #[tokio::test]
    async fn get_blocked_ips_returns_active_blocks() {
        let tracker = setup().await;
        let ip1 = IpAddr::V4(Ipv4Addr::new(10, 0, 0, 1));
        let ip2 = IpAddr::V4(Ipv4Addr::new(10, 0, 0, 2));

        tracker.block_ip(ip1, 3600).await;
        tracker.block_ip(ip2, 3600).await;

        let blocked = tracker.get_blocked_ips().await;
        assert_eq!(blocked.len(), 2);
        assert!(blocked.contains(&ip1));
        assert!(blocked.contains(&ip2));
    }

    #[tokio::test]
    async fn unblocked_ip_not_in_blocked_list() {
        let tracker = setup().await;
        let ip = test_ip();

        tracker.block_ip(ip, 3600).await;
        tracker.unblock_ip(ip).await;

        let blocked = tracker.get_blocked_ips().await;
        assert!(blocked.is_empty());
    }

    #[tokio::test]
    async fn violation_summary_for_unknown_ip() {
        let tracker = setup().await;
        let ip = test_ip();

        let summary = tracker.get_violation_summary(ip).await;
        assert_eq!(summary.total_violations, 0);
        assert!(!summary.is_blocked);
    }

    #[tokio::test]
    async fn cleanup_removes_expired_blocks() {
        let tracker = setup().await;
        let ip = test_ip();

        // Block with very short duration, then override with expired timestamp
        let ip_str = ip.to_string();
        let expired = (Utc::now() - chrono::Duration::hours(1)).to_rfc3339();
        sqlx::query("INSERT INTO ip_blocks (ip, blocked_until) VALUES ($1, $2)")
            .bind(&ip_str)
            .bind(&expired)
            .execute(&tracker.pool)
            .await
            .unwrap();

        assert!(!tracker.is_blocked(ip).await); // Already expired
        tracker.cleanup_expired().await;

        let blocked = tracker.get_blocked_ips().await;
        assert!(blocked.is_empty());
    }

    #[tokio::test]
    async fn violation_details_preserved() {
        let tracker = setup().await;
        let ip = test_ip();

        tracker
            .record_violation(
                ip,
                ViolationRecord::new("injection", ThreatLevel::High)
                    .with_details("SQL injection attempt"),
            )
            .await;

        let summary = tracker.get_violation_summary(ip).await;
        assert_eq!(summary.total_violations, 1);
        assert_eq!(summary.max_threat_level, Some(ThreatLevel::High));
    }

    #[tokio::test]
    async fn multiple_ips_tracked_independently() {
        let tracker = setup().await;
        let ip1 = IpAddr::V4(Ipv4Addr::new(10, 0, 0, 1));
        let ip2 = IpAddr::V4(Ipv4Addr::new(10, 0, 0, 2));

        tracker
            .record_violation(ip1, ViolationRecord::new("test", ThreatLevel::Medium))
            .await;

        let summary1 = tracker.get_violation_summary(ip1).await;
        let summary2 = tracker.get_violation_summary(ip2).await;

        assert_eq!(summary1.total_violations, 1);
        assert_eq!(summary2.total_violations, 0);
    }
}
