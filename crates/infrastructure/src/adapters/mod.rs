//! Infrastructure adapters
//!
//! Adapters connect application ports to concrete implementations.

mod caldav_calendar_adapter;
mod circuit_breaker;
mod env_secret_store;
mod hailo_inference_adapter;
mod proton_email_adapter;
mod vault_secret_store;

pub use caldav_calendar_adapter::CalDavCalendarAdapter;
pub use circuit_breaker::{
    CircuitBreaker, CircuitBreakerConfig, CircuitBreakerError, CircuitOpenError, CircuitState,
};
pub use env_secret_store::EnvSecretStore;
pub use hailo_inference_adapter::HailoInferenceAdapter;
pub use proton_email_adapter::ProtonEmailAdapter;
pub use vault_secret_store::{ChainedSecretStore, VaultConfig, VaultSecretStore};
