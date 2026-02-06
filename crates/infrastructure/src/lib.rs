//! Infrastructure layer - Adapters for external systems
//!
//! Implements ports defined in the application layer.
//! Contains adapters for Hailo inference, databases, external APIs, etc.

pub mod adapters;
pub mod cache;
pub mod config;
pub mod persistence;
pub mod retry;
pub mod telemetry;
pub mod validation;

pub use adapters::*;
pub use cache::{MokaCache, MultiLayerCache, RedbCache, generate_cache_key, llm_cache_key};
pub use config::{
    AppConfig, DatabaseConfig, DegradedModeAppConfig, Environment, RetryAppConfig, SecurityConfig,
    ServerConfig, TelemetryAppConfig, WhatsAppConfig,
};
pub use persistence::{ConnectionPool, SqliteConversationStore, SqliteDraftStore, create_pool};
pub use retry::{Retryable, RetryConfig, RetryResult, retry, with_retry};
pub use telemetry::{TelemetryConfig, TelemetryGuard, init_telemetry};
pub use validation::{SecurityValidator, SecurityWarning, WarningSeverity};
