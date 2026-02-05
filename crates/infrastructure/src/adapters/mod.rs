//! Infrastructure adapters
//!
//! Adapters connect application ports to concrete implementations.

mod cached_inference_adapter;
mod caldav_calendar_adapter;
mod circuit_breaker;
mod degraded_inference;
mod env_secret_store;
mod hailo_inference_adapter;
mod model_registry_adapter;
mod proton_email_adapter;
mod task_adapter;
mod vault_secret_store;
mod weather_adapter;

pub use cached_inference_adapter::CachedInferenceAdapter;
pub use caldav_calendar_adapter::CalDavCalendarAdapter;
pub use circuit_breaker::{
    CircuitBreaker, CircuitBreakerConfig, CircuitBreakerError, CircuitOpenError, CircuitState,
};
pub use degraded_inference::{
    DegradedInferenceAdapter, DegradedModeConfig, DegradedModeStats, ServiceStatus,
};
pub use env_secret_store::EnvSecretStore;
pub use hailo_inference_adapter::HailoInferenceAdapter;
pub use model_registry_adapter::HailoModelRegistryAdapter;
pub use proton_email_adapter::ProtonEmailAdapter;
pub use task_adapter::TaskAdapter;
pub use vault_secret_store::{ChainedSecretStore, VaultConfig, VaultSecretStore};
pub use weather_adapter::WeatherAdapter;
