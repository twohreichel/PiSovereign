//! Chaos engineering framework for resilience testing.
//!
//! Provides tools for injecting faults and failures into the system
//! to test resilience patterns like retries, circuit breakers, and fallbacks.
//!
//! # Overview
//!
//! The chaos framework consists of:
//! - `FaultInjector`: Intercepts calls and injects faults based on configuration
//! - `FaultPolicy`: Defines what kinds of faults to inject and how often
//! - `ChaosContext`: Tracks fault injection state and statistics
//!
//! # Example
//!
//! ```ignore
//! use infrastructure::chaos::{FaultInjector, FaultPolicy, FaultType};
//!
//! // Create a fault injector that fails 30% of requests
//! let injector = FaultInjector::new(FaultPolicy {
//!     fault_rate: 0.3,
//!     fault_type: FaultType::Error("Service unavailable".to_string()),
//!     ..Default::default()
//! });
//!
//! // Wrap operations with fault injection
//! let result = injector.maybe_inject(|| async {
//!     // Your actual operation
//!     Ok(42)
//! }).await;
//! ```

mod chaos_context;
mod fault_injector;
mod fault_policy;

pub use chaos_context::{ChaosContext, ChaosStats, InjectionResult};
pub use fault_injector::{FaultInjector, FaultInjectorConfig};
pub use fault_policy::{FaultPolicy, FaultType, LatencyDistribution};
