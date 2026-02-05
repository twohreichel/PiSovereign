//! Telemetry and distributed tracing infrastructure
//!
//! Provides OpenTelemetry integration for distributed tracing to Tempo/Jaeger.

mod otel;

pub use otel::{TelemetryConfig, TelemetryGuard, init_telemetry};
