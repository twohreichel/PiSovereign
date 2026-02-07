//! Fault injector for chaos engineering.
//!
//! This module provides the core fault injection functionality that can be used
//! to simulate various failure modes in tests.

use std::future::Future;
use std::io;
use std::time::Duration;

use thiserror::Error;

use super::{ChaosContext, FaultPolicy, FaultType, InjectionResult};

/// Configuration for the fault injector
#[derive(Debug, Clone)]
pub struct FaultInjectorConfig {
    /// Whether fault injection is enabled
    pub enabled: bool,
    /// Minimum time between fault injections
    pub cooldown: Option<Duration>,
}

impl Default for FaultInjectorConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            cooldown: None,
        }
    }
}

impl FaultInjectorConfig {
    /// Create a new config with fault injection enabled
    pub const fn enabled() -> Self {
        Self {
            enabled: true,
            cooldown: None,
        }
    }

    /// Create a new config with fault injection disabled
    pub const fn disabled() -> Self {
        Self {
            enabled: false,
            cooldown: None,
        }
    }

    /// Set the cooldown period between injections
    pub const fn with_cooldown(mut self, cooldown: Duration) -> Self {
        self.cooldown = Some(cooldown);
        self
    }
}

/// Error types that can be injected
#[derive(Debug, Error)]
pub enum InjectedError {
    /// Generic error
    #[error("Injected error: {0}")]
    Generic(String),

    /// Simulated I/O error
    #[error("Injected I/O error: {0}")]
    Io(#[from] io::Error),

    /// Simulated timeout
    #[error("Injected timeout after {0:?}")]
    Timeout(Duration),

    /// Connection refused
    #[error("Injected connection refused")]
    ConnectionRefused,

    /// Connection reset
    #[error("Injected connection reset")]
    ConnectionReset,

    /// Resource exhausted
    #[error("Injected resource exhaustion: {0}")]
    ResourceExhausted(String),

    /// Rate limited
    #[error("Injected rate limit")]
    RateLimited,
}

impl InjectedError {
    /// Create an I/O error from a fault type
    pub fn from_fault_type(fault_type: &FaultType) -> Self {
        match fault_type {
            FaultType::Error(msg) => Self::Generic(msg.clone()),
            FaultType::Timeout(d) => Self::Timeout(*d),
            FaultType::ConnectionRefused => Self::ConnectionRefused,
            FaultType::ConnectionReset => Self::ConnectionReset,
            FaultType::ResourceExhausted(msg) => Self::ResourceExhausted(msg.clone()),
            FaultType::RateLimited => Self::RateLimited,
            FaultType::Latency(_) => {
                // Latency is not an error type
                Self::Generic("Unexpected latency fault".to_string())
            },
            FaultType::LatencyThenError { error, .. } => Self::Generic(error.clone()),
            FaultType::CorruptedResponse { .. } => Self::Generic("Corrupted response".to_string()),
            FaultType::Custom(name) => Self::Generic(format!("Custom fault: {name}")),
        }
    }
}

/// Fault injector for simulating failures
#[derive(Debug)]
pub struct FaultInjector {
    config: FaultInjectorConfig,
    policy: FaultPolicy,
    context: ChaosContext,
}

impl FaultInjector {
    /// Create a new fault injector with the given policy
    pub fn new(policy: FaultPolicy) -> Self {
        Self {
            config: FaultInjectorConfig::default(),
            policy,
            context: ChaosContext::new(),
        }
    }

    /// Create a new fault injector with custom configuration
    pub fn with_config(config: FaultInjectorConfig, policy: FaultPolicy) -> Self {
        Self {
            config,
            policy,
            context: ChaosContext::new(),
        }
    }

    /// Create a fault injector with a maximum number of faults
    pub fn with_max_faults(policy: FaultPolicy, max_faults: u64) -> Self {
        Self {
            config: FaultInjectorConfig::default(),
            policy,
            context: ChaosContext::with_max_faults(max_faults),
        }
    }

    /// Create a disabled fault injector (no-op)
    pub fn disabled() -> Self {
        Self {
            config: FaultInjectorConfig::disabled(),
            policy: FaultPolicy::never(),
            context: ChaosContext::new(),
        }
    }

    /// Check if fault injection should occur based on policy and context
    fn should_inject(&self) -> InjectionResult {
        // Check if enabled
        if !self.config.enabled {
            return InjectionResult::Skipped;
        }

        // Check if max faults reached
        if !self.context.can_inject() {
            return InjectionResult::LimitReached;
        }

        // Check cooldown
        if let Some(cooldown) = self.config.cooldown {
            if let Some(elapsed) = self.context.time_since_last_injection() {
                if elapsed < cooldown {
                    return InjectionResult::Skipped;
                }
            }
        }

        // Check probability
        if self.policy.should_inject() {
            InjectionResult::Injected
        } else {
            InjectionResult::NoInjection
        }
    }

    /// Maybe inject a fault based on the policy
    ///
    /// Returns `Some(fault_type)` if a fault should be injected, `None` otherwise.
    #[allow(clippy::cast_possible_truncation)]
    pub fn maybe_inject(&mut self) -> Option<FaultType> {
        self.context.record_call();
        let result = self.should_inject();

        if result == InjectionResult::Injected {
            let fault = self.policy.select_fault();
            if let Some(ref fault_type) = fault {
                self.context.record_injection(InjectionResult::Injected);

                // Update specific counters
                match fault_type {
                    FaultType::Error(_)
                    | FaultType::ConnectionRefused
                    | FaultType::ConnectionReset
                    | FaultType::ResourceExhausted(_)
                    | FaultType::RateLimited
                    | FaultType::CorruptedResponse { .. }
                    | FaultType::Custom(_) => {
                        self.context.record_error();
                    },
                    FaultType::Latency(dist) => {
                        let latency = dist.sample();
                        self.context.record_latency(latency.as_millis() as u64);
                    },
                    FaultType::LatencyThenError { latency, .. } => {
                        let delay = latency.sample();
                        self.context.record_latency(delay.as_millis() as u64);
                        self.context.record_error();
                    },
                    FaultType::Timeout(_) => {
                        self.context.record_timeout();
                    },
                }
            }
            fault
        } else {
            self.context.record_injection(result);
            None
        }
    }

    /// Wrap an async operation with potential fault injection
    ///
    /// If a fault is selected, it will either:
    /// - Add latency before executing the operation
    /// - Return an error instead of executing the operation
    pub async fn wrap<F, T, E>(&mut self, operation: F) -> Result<T, E>
    where
        F: Future<Output = Result<T, E>>,
        E: From<InjectedError>,
    {
        if let Some(fault_type) = self.maybe_inject() {
            match &fault_type {
                FaultType::Latency(dist) => {
                    // Add latency, then execute
                    let delay = dist.sample();
                    tokio::time::sleep(delay).await;
                    operation.await
                },
                FaultType::LatencyThenError { latency, .. } => {
                    // Add latency, then return error
                    let delay = latency.sample();
                    tokio::time::sleep(delay).await;
                    Err(InjectedError::from_fault_type(&fault_type).into())
                },
                FaultType::Timeout(duration) => {
                    // Simulate timeout by sleeping and then returning error
                    tokio::time::sleep(*duration).await;
                    Err(InjectedError::Timeout(*duration).into())
                },
                _ => {
                    // Return error immediately
                    Err(InjectedError::from_fault_type(&fault_type).into())
                },
            }
        } else {
            // No fault injection, execute normally
            operation.await
        }
    }

    /// Wrap an async operation, executing it normally if no error fault is injected
    ///
    /// Unlike `wrap`, this will NOT add latency - only error faults will interrupt execution.
    pub async fn wrap_errors_only<F, T, E>(&mut self, operation: F) -> Result<T, E>
    where
        F: Future<Output = Result<T, E>>,
        E: From<InjectedError>,
    {
        if let Some(fault_type) = self.maybe_inject() {
            match &fault_type {
                FaultType::Latency(_) => {
                    // Ignore latency, execute normally
                    operation.await
                },
                _ => {
                    // Return error
                    Err(InjectedError::from_fault_type(&fault_type).into())
                },
            }
        } else {
            operation.await
        }
    }

    /// Get current statistics
    pub fn stats(&self) -> &super::ChaosStats {
        self.context.stats()
    }

    /// Reset the injector state
    pub fn reset(&mut self) {
        self.context.reset();
    }

    /// Get remaining fault count (if limited)
    pub fn remaining_faults(&self) -> Option<u64> {
        self.context.remaining_faults()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_default() {
        let config = FaultInjectorConfig::default();
        assert!(config.enabled);
        assert!(config.cooldown.is_none());
    }

    #[test]
    fn config_disabled() {
        let config = FaultInjectorConfig::disabled();
        assert!(!config.enabled);
    }

    #[test]
    fn config_with_cooldown() {
        let config = FaultInjectorConfig::enabled().with_cooldown(Duration::from_secs(5));
        assert!(config.enabled);
        assert_eq!(config.cooldown, Some(Duration::from_secs(5)));
    }

    #[test]
    fn injector_new() {
        let injector =
            FaultInjector::new(FaultPolicy::always(FaultType::Error("test".to_string())));
        assert_eq!(injector.stats().total_calls, 0);
    }

    #[test]
    fn injector_disabled_never_injects() {
        let mut injector = FaultInjector::disabled();
        for _ in 0..10 {
            assert!(injector.maybe_inject().is_none());
        }
        assert_eq!(injector.stats().total_calls, 10);
        assert_eq!(injector.stats().faults_injected, 0);
    }

    #[test]
    fn injector_always_injects() {
        let mut injector =
            FaultInjector::new(FaultPolicy::always(FaultType::Error("test".to_string())));
        for _ in 0..10 {
            assert!(injector.maybe_inject().is_some());
        }
        assert_eq!(injector.stats().faults_injected, 10);
        assert_eq!(injector.stats().errors_injected, 10);
    }

    #[test]
    fn injector_respects_max_faults() {
        let mut injector = FaultInjector::with_max_faults(
            FaultPolicy::always(FaultType::Error("test".to_string())),
            3,
        );

        // First 3 should inject
        assert!(injector.maybe_inject().is_some());
        assert!(injector.maybe_inject().is_some());
        assert!(injector.maybe_inject().is_some());

        // Rest should not
        assert!(injector.maybe_inject().is_none());
        assert!(injector.maybe_inject().is_none());

        assert_eq!(injector.stats().faults_injected, 3);
        assert_eq!(injector.remaining_faults(), Some(0));
    }

    #[test]
    fn injector_reset() {
        let mut injector =
            FaultInjector::new(FaultPolicy::always(FaultType::Error("test".to_string())));
        injector.maybe_inject();
        assert_eq!(injector.stats().faults_injected, 1);
        injector.reset();
        assert_eq!(injector.stats().faults_injected, 0);
    }

    #[tokio::test]
    async fn injector_wrap_latency() {
        use std::time::Instant;

        let mut injector = FaultInjector::new(FaultPolicy::always(FaultType::Latency(
            super::super::LatencyDistribution::uniform(
                Duration::from_millis(50),
                Duration::from_millis(50),
            ),
        )));

        let start = Instant::now();
        let result: Result<i32, InjectedError> = injector.wrap(async { Ok(42) }).await;

        assert_eq!(result.unwrap(), 42);
        assert!(start.elapsed() >= Duration::from_millis(50));
    }

    #[tokio::test]
    async fn injector_wrap_error() {
        let mut injector = FaultInjector::new(FaultPolicy::always(FaultType::Error(
            "forced error".to_string(),
        )));

        let result: Result<i32, InjectedError> = injector.wrap(async { Ok(42) }).await;

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, InjectedError::Generic(msg) if msg == "forced error"));
    }

    #[tokio::test]
    async fn injector_wrap_errors_only_ignores_latency() {
        use std::time::Instant;

        let mut injector = FaultInjector::new(FaultPolicy::always(FaultType::Latency(
            super::super::LatencyDistribution::uniform(
                Duration::from_millis(100),
                Duration::from_millis(100),
            ),
        )));

        let start = Instant::now();
        let result: Result<i32, InjectedError> = injector.wrap_errors_only(async { Ok(42) }).await;

        assert_eq!(result.unwrap(), 42);
        // Should complete quickly since latency is ignored
        assert!(start.elapsed() < Duration::from_millis(50));
    }

    #[tokio::test]
    async fn injector_wrap_connection_refused() {
        let mut injector = FaultInjector::new(FaultPolicy::always(FaultType::ConnectionRefused));

        let result: Result<i32, InjectedError> = injector.wrap(async { Ok(42) }).await;

        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            InjectedError::ConnectionRefused
        ));
    }

    #[tokio::test]
    async fn injector_wrap_no_fault() {
        let mut injector = FaultInjector::new(FaultPolicy::never());

        let result: Result<i32, InjectedError> = injector.wrap(async { Ok(42) }).await;

        assert_eq!(result.unwrap(), 42);
    }

    #[test]
    fn injected_error_display() {
        assert_eq!(
            InjectedError::Generic("test".to_string()).to_string(),
            "Injected error: test"
        );
        assert_eq!(
            InjectedError::ConnectionRefused.to_string(),
            "Injected connection refused"
        );
        assert_eq!(
            InjectedError::Timeout(Duration::from_secs(5)).to_string(),
            "Injected timeout after 5s"
        );
    }
}
