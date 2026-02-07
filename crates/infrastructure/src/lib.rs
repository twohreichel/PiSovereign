//! Infrastructure layer - Adapters for external systems
//!
//! Implements ports defined in the application layer.
//! Contains adapters for Hailo inference, databases, external APIs, etc.

pub mod adapters;
pub mod cache;
#[cfg(test)]
pub mod chaos;
pub mod config;
pub mod http;
pub mod persistence;
pub mod retry;
pub mod scheduler;
pub mod telemetry;
pub mod templates;
#[cfg(test)]
pub mod testing;
pub mod validation;

pub use adapters::*;
pub use cache::{MokaCache, MultiLayerCache, RedbCache, generate_cache_key, llm_cache_key};
pub use config::{
    AppConfig, CalDavAppConfig, DatabaseConfig, DegradedModeAppConfig, Environment,
    ProtonAppConfig, RetryAppConfig, SecurityConfig, ServerConfig, TelemetryAppConfig,
    WeatherConfig, WhatsAppConfig,
};
pub use http::{CorrelatedClientConfig, CorrelatedHttpClient, RequestIdProvider, X_REQUEST_ID};
pub use persistence::{
    ConnectionPool, SqliteConversationStore, SqliteDatabaseHealth, SqliteDraftStore, create_pool,
};
pub use retry::{RetryConfig, RetryResult, Retryable, retry, with_retry};
pub use scheduler::{
    SchedulerConfig, SchedulerError, TaskBuilder, TaskEvent, TaskScheduler, TaskStats, TaskStatus,
    schedules,
};
pub use telemetry::{TelemetryConfig, TelemetryGuard, init_telemetry};
pub use templates::{
    AssistantResponseData, CalendarEventData, EmailDraftData, ForecastDay, TemplateConfig,
    TemplateContext, TemplateEngine, TemplateError, WeatherReportData,
};
pub use validation::{SecurityValidator, SecurityWarning, WarningSeverity};
