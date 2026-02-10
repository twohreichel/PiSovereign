//! Fault policy definitions for chaos engineering.
//!
//! Defines the types of faults that can be injected and their parameters.

use std::time::Duration;

use serde::{Deserialize, Serialize};

/// Types of faults that can be injected
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "params")]
pub enum FaultType {
    /// Return an error with the given message
    Error(String),

    /// Add latency before completing the operation
    Latency(LatencyDistribution),

    /// Return an error after adding latency
    LatencyThenError {
        latency: LatencyDistribution,
        error: String,
    },

    /// Simulate a timeout (wait for duration then return timeout error)
    Timeout(Duration),

    /// Return a partial/corrupted response
    CorruptedResponse {
        /// Probability of corruption (0.0-1.0)
        corruption_rate: f64,
    },

    /// Simulate connection refused
    ConnectionRefused,

    /// Simulate connection reset
    ConnectionReset,

    /// Simulate resource exhaustion
    ResourceExhausted(String),

    /// Simulate rate limiting
    RateLimited,

    /// Custom fault with a name for tracking
    Custom(String),
}

impl Default for FaultType {
    fn default() -> Self {
        Self::Error("Injected fault".to_string())
    }
}

/// Distribution for latency injection
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct LatencyDistribution {
    /// Minimum latency to add
    pub min: Duration,
    /// Maximum latency to add
    pub max: Duration,
    /// Distribution type for random selection
    pub distribution: LatencyDistributionType,
}

impl LatencyDistribution {
    /// Create a constant latency (no variation)
    pub const fn constant(duration: Duration) -> Self {
        Self {
            min: duration,
            max: duration,
            distribution: LatencyDistributionType::Uniform,
        }
    }

    /// Create a uniformly distributed latency range
    pub const fn uniform(min: Duration, max: Duration) -> Self {
        Self {
            min,
            max,
            distribution: LatencyDistributionType::Uniform,
        }
    }

    /// Create a normally distributed latency (bell curve)
    pub const fn normal(mean: Duration, std_dev: Duration) -> Self {
        Self {
            min: mean,
            max: std_dev, // Using max field for std_dev in normal distribution
            distribution: LatencyDistributionType::Normal,
        }
    }

    /// Sample a latency value from this distribution
    #[allow(clippy::cast_possible_truncation)]
    pub fn sample(&self) -> Duration {
        use rand::Rng;
        let mut rng = rand::thread_rng();

        match self.distribution {
            LatencyDistributionType::Uniform => {
                let range = self.max.as_nanos() - self.min.as_nanos();
                if range == 0 {
                    return self.min;
                }
                let random_nanos = rng.gen_range(0..=range);
                self.min + Duration::from_nanos(random_nanos as u64)
            },
            LatencyDistributionType::Normal => {
                // Box-Muller transform for normal distribution
                let u1: f64 = rng.r#gen();
                let u2: f64 = rng.r#gen();
                let z0 = (-2.0 * u1.ln()).sqrt() * (2.0 * std::f64::consts::PI * u2).cos();

                let mean = self.min.as_secs_f64();
                let std_dev = self.max.as_secs_f64();
                let sample = std_dev.mul_add(z0, mean);

                // Ensure non-negative
                Duration::from_secs_f64(sample.max(0.0))
            },
            LatencyDistributionType::Exponential => {
                // Exponential distribution with lambda based on mean
                let mean = f64::midpoint(self.min.as_secs_f64(), self.max.as_secs_f64());
                let lambda = 1.0 / mean;
                let u: f64 = rng.r#gen();
                let sample = -u.ln() / lambda;

                // Clamp to min/max
                let sample = sample.clamp(self.min.as_secs_f64(), self.max.as_secs_f64());
                Duration::from_secs_f64(sample)
            },
        }
    }
}

impl Default for LatencyDistribution {
    fn default() -> Self {
        Self {
            min: Duration::from_millis(100),
            max: Duration::from_millis(500),
            distribution: LatencyDistributionType::Uniform,
        }
    }
}

/// Type of latency distribution
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
pub enum LatencyDistributionType {
    /// Uniform distribution between min and max
    #[default]
    Uniform,
    /// Normal (Gaussian) distribution
    Normal,
    /// Exponential distribution
    Exponential,
}

/// Policy for fault injection
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FaultPolicy {
    /// Probability of injecting a fault (0.0 to 1.0)
    pub fault_rate: f64,

    /// Type of fault to inject
    pub fault_type: FaultType,

    /// Whether fault injection is enabled
    pub enabled: bool,

    /// Optional list of operation names to target
    /// If empty, all operations are targeted
    pub target_operations: Vec<String>,

    /// Optional list of operation names to exclude
    pub exclude_operations: Vec<String>,

    /// Maximum number of faults to inject (None = unlimited)
    pub max_faults: Option<usize>,

    /// Cooldown between injections (prevents fault storms)
    pub cooldown: Option<Duration>,
}

impl Default for FaultPolicy {
    fn default() -> Self {
        Self {
            fault_rate: 0.1, // 10% fault rate by default
            fault_type: FaultType::default(),
            enabled: false, // Disabled by default for safety
            target_operations: Vec::new(),
            exclude_operations: Vec::new(),
            max_faults: None,
            cooldown: None,
        }
    }
}

impl FaultPolicy {
    /// Create a policy that never injects faults
    pub fn never() -> Self {
        Self {
            fault_rate: 0.0,
            enabled: false,
            ..Default::default()
        }
    }

    /// Create a policy that always injects the given fault
    pub fn always(fault_type: FaultType) -> Self {
        Self {
            fault_rate: 1.0,
            fault_type,
            enabled: true,
            ..Default::default()
        }
    }

    /// Create a new fault policy with the given fault rate
    pub fn with_rate(fault_rate: f64) -> Self {
        Self {
            fault_rate,
            enabled: true,
            ..Default::default()
        }
    }

    /// Create an error injection policy
    pub fn error(fault_rate: f64, message: impl Into<String>) -> Self {
        Self {
            fault_rate,
            fault_type: FaultType::Error(message.into()),
            enabled: true,
            ..Default::default()
        }
    }

    /// Create a latency injection policy
    pub fn latency(fault_rate: f64, latency: LatencyDistribution) -> Self {
        Self {
            fault_rate,
            fault_type: FaultType::Latency(latency),
            enabled: true,
            ..Default::default()
        }
    }

    /// Create a timeout injection policy
    pub fn timeout(fault_rate: f64, timeout: Duration) -> Self {
        Self {
            fault_rate,
            fault_type: FaultType::Timeout(timeout),
            enabled: true,
            ..Default::default()
        }
    }

    /// Set the fault type
    #[must_use]
    pub fn with_fault_type(mut self, fault_type: FaultType) -> Self {
        self.fault_type = fault_type;
        self
    }

    /// Enable or disable the policy
    #[must_use]
    pub const fn with_enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    /// Set target operations
    #[must_use]
    pub fn with_targets(mut self, operations: Vec<String>) -> Self {
        self.target_operations = operations;
        self
    }

    /// Set excluded operations
    #[must_use]
    pub fn with_exclusions(mut self, operations: Vec<String>) -> Self {
        self.exclude_operations = operations;
        self
    }

    /// Set maximum number of faults
    #[must_use]
    pub const fn with_max_faults(mut self, max: usize) -> Self {
        self.max_faults = Some(max);
        self
    }

    /// Set cooldown between faults
    #[must_use]
    pub const fn with_cooldown(mut self, cooldown: Duration) -> Self {
        self.cooldown = Some(cooldown);
        self
    }

    /// Check if a given operation should be considered for fault injection
    pub fn should_target(&self, operation: &str) -> bool {
        // If excluded, don't target
        if self.exclude_operations.iter().any(|op| op == operation) {
            return false;
        }

        // If targets are specified and operation is not in list, don't target
        if !self.target_operations.is_empty()
            && !self.target_operations.iter().any(|op| op == operation)
        {
            return false;
        }

        true
    }

    /// Check if a fault should be injected based on probability
    pub fn should_inject(&self) -> bool {
        use rand::Rng;

        if !self.enabled {
            return false;
        }

        if self.fault_rate <= 0.0 {
            return false;
        }

        if self.fault_rate >= 1.0 {
            return true;
        }

        let mut rng = rand::thread_rng();
        rng.r#gen::<f64>() < self.fault_rate
    }

    /// Select and return the fault type to inject
    pub fn select_fault(&self) -> Option<FaultType> {
        if self.enabled {
            Some(self.fault_type.clone())
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fault_type_default() {
        let ft = FaultType::default();
        assert!(matches!(ft, FaultType::Error(_)));
    }

    #[test]
    fn latency_distribution_constant() {
        let dist = LatencyDistribution::constant(Duration::from_millis(100));
        let sample = dist.sample();
        assert_eq!(sample, Duration::from_millis(100));
    }

    #[test]
    fn latency_distribution_uniform() {
        let dist =
            LatencyDistribution::uniform(Duration::from_millis(100), Duration::from_millis(500));
        let sample = dist.sample();
        assert!(sample >= Duration::from_millis(100));
        assert!(sample <= Duration::from_millis(500));
    }

    #[test]
    fn fault_policy_default() {
        let policy = FaultPolicy::default();
        assert!((policy.fault_rate - 0.1).abs() < f64::EPSILON);
        assert!(!policy.enabled); // Disabled by default
    }

    #[test]
    fn fault_policy_error() {
        let policy = FaultPolicy::error(0.5, "Test error");
        assert!((policy.fault_rate - 0.5).abs() < f64::EPSILON);
        assert!(policy.enabled);
        assert!(matches!(policy.fault_type, FaultType::Error(ref msg) if msg == "Test error"));
    }

    #[test]
    fn fault_policy_latency() {
        let latency = LatencyDistribution::constant(Duration::from_secs(1));
        let policy = FaultPolicy::latency(0.2, latency);
        assert!((policy.fault_rate - 0.2).abs() < f64::EPSILON);
        assert!(matches!(policy.fault_type, FaultType::Latency(_)));
    }

    #[test]
    fn fault_policy_timeout() {
        let policy = FaultPolicy::timeout(0.4, Duration::from_secs(5));
        assert!((policy.fault_rate - 0.4).abs() < f64::EPSILON);
        assert!(matches!(policy.fault_type, FaultType::Timeout(_)));
    }

    #[test]
    fn fault_policy_should_target_all() {
        let policy = FaultPolicy::default();
        assert!(policy.should_target("any_operation"));
    }

    #[test]
    fn fault_policy_should_target_specific() {
        let policy =
            FaultPolicy::default().with_targets(vec!["op1".to_string(), "op2".to_string()]);
        assert!(policy.should_target("op1"));
        assert!(policy.should_target("op2"));
        assert!(!policy.should_target("op3"));
    }

    #[test]
    fn fault_policy_should_target_exclusions() {
        let policy = FaultPolicy::default().with_exclusions(vec!["excluded_op".to_string()]);
        assert!(policy.should_target("any_op"));
        assert!(!policy.should_target("excluded_op"));
    }

    #[test]
    fn fault_policy_builder_chain() {
        let policy = FaultPolicy::with_rate(0.3)
            .with_fault_type(FaultType::Timeout(Duration::from_secs(5)))
            .with_enabled(true)
            .with_max_faults(100)
            .with_cooldown(Duration::from_secs(1));

        assert!((policy.fault_rate - 0.3).abs() < f64::EPSILON);
        assert!(policy.enabled);
        assert!(matches!(policy.fault_type, FaultType::Timeout(_)));
        assert_eq!(policy.max_faults, Some(100));
        assert_eq!(policy.cooldown, Some(Duration::from_secs(1)));
    }

    #[test]
    fn fault_policy_never() {
        let policy = FaultPolicy::never();
        assert!(!policy.enabled);
        assert!(!policy.should_inject());
    }

    #[test]
    fn fault_policy_always() {
        let policy = FaultPolicy::always(FaultType::ConnectionRefused);
        assert!(policy.enabled);
        assert!((policy.fault_rate - 1.0).abs() < f64::EPSILON);
        assert!(policy.should_inject());
    }

    #[test]
    fn fault_policy_select_fault() {
        let policy = FaultPolicy::always(FaultType::RateLimited);
        let fault = policy.select_fault();
        assert!(matches!(fault, Some(FaultType::RateLimited)));
    }

    #[test]
    fn fault_policy_select_fault_disabled() {
        let policy = FaultPolicy::default(); // Disabled by default
        let fault = policy.select_fault();
        assert!(fault.is_none());
    }

    #[test]
    fn latency_distribution_normal() {
        let dist =
            LatencyDistribution::normal(Duration::from_millis(100), Duration::from_millis(10));
        // Normal distribution should produce valid durations
        for _ in 0..10 {
            let _sample = dist.sample();
            // Duration is always non-negative by definition
        }
    }

    #[test]
    fn latency_distribution_default() {
        let dist = LatencyDistribution::default();
        assert_eq!(dist.min, Duration::from_millis(100));
        assert_eq!(dist.max, Duration::from_millis(500));
        assert_eq!(dist.distribution, LatencyDistributionType::Uniform);
    }

    #[test]
    fn latency_distribution_type_default() {
        let dt = LatencyDistributionType::default();
        assert_eq!(dt, LatencyDistributionType::Uniform);
    }

    #[test]
    fn fault_type_variants() {
        // Test all fault type variants
        let _ = FaultType::Error("test".to_string());
        let _ = FaultType::Latency(LatencyDistribution::default());
        let _ = FaultType::LatencyThenError {
            latency: LatencyDistribution::default(),
            error: "test".to_string(),
        };
        let _ = FaultType::Timeout(Duration::from_secs(1));
        let _ = FaultType::CorruptedResponse {
            corruption_rate: 0.5,
        };
        let _ = FaultType::ConnectionRefused;
        let _ = FaultType::ConnectionReset;
        let _ = FaultType::ResourceExhausted("memory".to_string());
        let _ = FaultType::RateLimited;
        let _ = FaultType::Custom("my_fault".to_string());
    }

    #[test]
    fn fault_type_debug() {
        let ft = FaultType::Error("debug test".to_string());
        let debug = format!("{ft:?}");
        assert!(debug.contains("Error"));
        assert!(debug.contains("debug test"));
    }

    #[test]
    fn fault_type_clone() {
        let ft = FaultType::ConnectionRefused;
        #[allow(clippy::redundant_clone)]
        let cloned = ft.clone();
        assert!(matches!(cloned, FaultType::ConnectionRefused));
    }

    #[test]
    fn fault_type_serialization() {
        // Test JSON serialization of fault types
        let ft = FaultType::Error("serialization test".to_string());
        let json = serde_json::to_string(&ft).unwrap();
        assert!(json.contains("Error"));

        let ft2 = FaultType::Timeout(Duration::from_secs(5));
        let json2 = serde_json::to_string(&ft2).unwrap();
        assert!(json2.contains("Timeout"));

        let ft3 = FaultType::CorruptedResponse {
            corruption_rate: 0.3,
        };
        let json3 = serde_json::to_string(&ft3).unwrap();
        assert!(json3.contains("corruption_rate"));
    }

    #[test]
    fn latency_distribution_serialization() {
        let dist = LatencyDistribution::constant(Duration::from_millis(100));
        let json = serde_json::to_string(&dist).unwrap();
        assert!(json.contains("Uniform"));

        let parsed: LatencyDistribution = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.distribution, dist.distribution);
    }

    #[test]
    fn fault_policy_serialization() {
        let policy = FaultPolicy::error(0.5, "test error");
        let json = serde_json::to_string(&policy).unwrap();

        let parsed: FaultPolicy = serde_json::from_str(&json).unwrap();
        assert!((parsed.fault_rate - 0.5).abs() < f64::EPSILON);
        assert!(parsed.enabled);
    }

    #[test]
    fn fault_policy_should_inject_rate_zero() {
        let mut policy = FaultPolicy::always(FaultType::Error("test".to_string()));
        policy.fault_rate = 0.0;
        assert!(!policy.should_inject());
    }

    #[test]
    fn fault_policy_should_inject_rate_one() {
        let mut policy = FaultPolicy::with_rate(1.0);
        policy.enabled = true;
        assert!(policy.should_inject());
    }

    #[test]
    fn fault_policy_should_inject_disabled() {
        let mut policy = FaultPolicy::with_rate(1.0);
        policy.enabled = false;
        assert!(!policy.should_inject());
    }

    #[test]
    fn fault_policy_targets_and_exclusions() {
        let policy = FaultPolicy::default()
            .with_targets(vec!["op1".to_string(), "op2".to_string()])
            .with_exclusions(vec!["op2".to_string()]);

        // op1 is targeted and not excluded
        assert!(policy.should_target("op1"));
        // op2 is targeted but also excluded
        assert!(!policy.should_target("op2"));
        // op3 is not in targets
        assert!(!policy.should_target("op3"));
    }

    #[test]
    fn latency_distribution_exponential() {
        // Create a distribution with Exponential type manually
        let dist = LatencyDistribution {
            min: Duration::from_millis(10),
            max: Duration::from_millis(1000),
            distribution: LatencyDistributionType::Exponential,
        };

        // Exponential distribution should produce values clamped to min/max
        for _ in 0..10 {
            let sample = dist.sample();
            assert!(sample >= Duration::from_millis(10));
            assert!(sample <= Duration::from_millis(1000));
        }
    }

    #[test]
    fn latency_distribution_type_eq() {
        assert_eq!(
            LatencyDistributionType::Uniform,
            LatencyDistributionType::Uniform
        );
        assert_eq!(
            LatencyDistributionType::Normal,
            LatencyDistributionType::Normal
        );
        assert_eq!(
            LatencyDistributionType::Exponential,
            LatencyDistributionType::Exponential
        );
        assert_ne!(
            LatencyDistributionType::Uniform,
            LatencyDistributionType::Normal
        );
    }

    #[test]
    fn latency_distribution_copy() {
        let dist = LatencyDistributionType::Uniform;
        let copied = dist;
        assert_eq!(dist, copied);
    }
}
