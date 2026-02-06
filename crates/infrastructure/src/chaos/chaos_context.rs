//! Chaos context for tracking fault injection state and statistics.

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

use parking_lot::RwLock;
use serde::{Deserialize, Serialize};

/// Result of a fault injection attempt
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InjectionResult {
    /// No fault was injected (operation proceeds normally)
    NoInjection,
    /// Fault was injected
    Injected,
    /// Injection was skipped (disabled, excluded, or cooldown)
    Skipped,
    /// Maximum faults reached
    LimitReached,
}

/// Statistics about fault injection
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ChaosStats {
    /// Total number of calls processed
    pub total_calls: u64,
    /// Number of faults injected
    pub faults_injected: u64,
    /// Number of calls that were skipped (no injection)
    pub calls_skipped: u64,
    /// Number of errors injected
    pub errors_injected: u64,
    /// Number of latency faults injected
    pub latency_injected: u64,
    /// Number of timeouts injected
    pub timeouts_injected: u64,
    /// Total latency added (milliseconds)
    pub total_latency_added_ms: u64,
}

impl ChaosStats {
    /// Calculate the actual fault rate
    #[allow(clippy::cast_precision_loss)]
    pub fn actual_fault_rate(&self) -> f64 {
        if self.total_calls == 0 {
            0.0
        } else {
            self.faults_injected as f64 / self.total_calls as f64
        }
    }
}

/// Context for tracking chaos engineering state
#[derive(Debug)]
pub struct ChaosContext {
    stats: ChaosStats,
    last_injection: RwLock<Option<Instant>>,
    remaining_faults: Option<AtomicU64>,
}

impl Default for ChaosContext {
    fn default() -> Self {
        Self::new()
    }
}

impl ChaosContext {
    /// Create a new chaos context
    pub const fn new() -> Self {
        Self {
            stats: ChaosStats {
                total_calls: 0,
                faults_injected: 0,
                calls_skipped: 0,
                errors_injected: 0,
                latency_injected: 0,
                timeouts_injected: 0,
                total_latency_added_ms: 0,
            },
            last_injection: RwLock::new(None),
            remaining_faults: None,
        }
    }

    /// Create a chaos context with a maximum number of faults
    pub fn with_max_faults(max_faults: u64) -> Self {
        Self {
            stats: ChaosStats::default(),
            last_injection: RwLock::new(None),
            remaining_faults: Some(AtomicU64::new(max_faults)),
        }
    }

    /// Record a call being processed
    pub fn record_call(&mut self) {
        self.stats.total_calls += 1;
    }

    /// Record a fault being injected
    pub fn record_injection(&mut self, result: InjectionResult) {
        match result {
            InjectionResult::Injected => {
                self.stats.faults_injected += 1;
                *self.last_injection.write() = Some(Instant::now());

                // Decrement remaining faults if limited
                if let Some(ref remaining) = self.remaining_faults {
                    remaining.fetch_sub(1, Ordering::SeqCst);
                }
            },
            InjectionResult::NoInjection
            | InjectionResult::Skipped
            | InjectionResult::LimitReached => {
                self.stats.calls_skipped += 1;
            },
        }
    }

    /// Record an error injection
    pub fn record_error(&mut self) {
        self.stats.errors_injected += 1;
    }

    /// Record a latency injection
    pub fn record_latency(&mut self, latency_ms: u64) {
        self.stats.latency_injected += 1;
        self.stats.total_latency_added_ms += latency_ms;
    }

    /// Record a timeout injection
    pub fn record_timeout(&mut self) {
        self.stats.timeouts_injected += 1;
    }

    /// Check if more faults can be injected (respects max_faults limit)
    pub fn can_inject(&self) -> bool {
        match &self.remaining_faults {
            Some(remaining) => remaining.load(Ordering::SeqCst) > 0,
            None => true, // No limit
        }
    }

    /// Get time since last injection
    pub fn time_since_last_injection(&self) -> Option<std::time::Duration> {
        self.last_injection.read().map(|t| t.elapsed())
    }

    /// Get current statistics
    pub fn stats(&self) -> &ChaosStats {
        &self.stats
    }

    /// Get a copy of current statistics
    pub fn stats_snapshot(&self) -> ChaosStats {
        self.stats.clone()
    }

    /// Reset statistics
    pub fn reset(&mut self) {
        self.stats = ChaosStats::default();
        *self.last_injection.write() = None;
    }

    /// Get remaining fault count (if limited)
    pub fn remaining_faults(&self) -> Option<u64> {
        self.remaining_faults
            .as_ref()
            .map(|r| r.load(Ordering::SeqCst))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chaos_stats_default() {
        let stats = ChaosStats::default();
        assert_eq!(stats.total_calls, 0);
        assert_eq!(stats.faults_injected, 0);
    }

    #[test]
    fn chaos_stats_actual_fault_rate() {
        let stats = ChaosStats {
            total_calls: 100,
            faults_injected: 25,
            ..Default::default()
        };
        assert!((stats.actual_fault_rate() - 0.25).abs() < f64::EPSILON);
    }

    #[test]
    fn chaos_stats_fault_rate_zero_calls() {
        let stats = ChaosStats::default();
        assert!((stats.actual_fault_rate() - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn chaos_context_new() {
        let ctx = ChaosContext::new();
        assert!(ctx.can_inject());
        assert!(ctx.time_since_last_injection().is_none());
    }

    #[test]
    fn chaos_context_with_max_faults() {
        let ctx = ChaosContext::with_max_faults(5);
        assert!(ctx.can_inject());
        assert_eq!(ctx.remaining_faults(), Some(5));
    }

    #[test]
    fn chaos_context_record_call() {
        let mut ctx = ChaosContext::new();
        ctx.record_call();
        ctx.record_call();
        assert_eq!(ctx.stats().total_calls, 2);
    }

    #[test]
    fn chaos_context_record_injection() {
        let mut ctx = ChaosContext::new();
        ctx.record_call();
        ctx.record_injection(InjectionResult::Injected);
        assert_eq!(ctx.stats().faults_injected, 1);
        assert!(ctx.time_since_last_injection().is_some());
    }

    #[test]
    fn chaos_context_record_no_injection() {
        let mut ctx = ChaosContext::new();
        ctx.record_call();
        ctx.record_injection(InjectionResult::NoInjection);
        assert_eq!(ctx.stats().calls_skipped, 1);
    }

    #[test]
    fn chaos_context_decrements_remaining() {
        let mut ctx = ChaosContext::with_max_faults(2);
        ctx.record_call();
        ctx.record_injection(InjectionResult::Injected);
        assert_eq!(ctx.remaining_faults(), Some(1));
        ctx.record_call();
        ctx.record_injection(InjectionResult::Injected);
        assert_eq!(ctx.remaining_faults(), Some(0));
        assert!(!ctx.can_inject());
    }

    #[test]
    fn chaos_context_reset() {
        let mut ctx = ChaosContext::new();
        ctx.record_call();
        ctx.record_injection(InjectionResult::Injected);
        ctx.reset();
        assert_eq!(ctx.stats().total_calls, 0);
        assert!(ctx.time_since_last_injection().is_none());
    }

    #[test]
    fn chaos_context_record_error() {
        let mut ctx = ChaosContext::new();
        ctx.record_error();
        assert_eq!(ctx.stats().errors_injected, 1);
    }

    #[test]
    fn chaos_context_record_latency() {
        let mut ctx = ChaosContext::new();
        ctx.record_latency(100);
        ctx.record_latency(200);
        assert_eq!(ctx.stats().latency_injected, 2);
        assert_eq!(ctx.stats().total_latency_added_ms, 300);
    }

    #[test]
    fn chaos_context_record_timeout() {
        let mut ctx = ChaosContext::new();
        ctx.record_timeout();
        assert_eq!(ctx.stats().timeouts_injected, 1);
    }
}
