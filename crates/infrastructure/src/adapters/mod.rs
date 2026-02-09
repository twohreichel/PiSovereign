//! Infrastructure adapters
//!
//! Adapters connect application ports to concrete implementations.

mod api_key_hasher;
mod cached_inference_adapter;
mod caldav_calendar_adapter;
mod circuit_breaker;
mod degraded_inference;
mod encryption_adapter;
mod env_secret_store;
mod model_registry_adapter;
mod ollama_inference_adapter;
mod proton_email_adapter;
mod signal_adapter;
mod speech_adapter;
mod suspicious_activity_adapter;
mod task_adapter;
mod vault_secret_store;
mod weather_adapter;
mod websearch_adapter;
mod whatsapp_adapter;

pub use api_key_hasher::{ApiKeyHashError, ApiKeyHasher};
pub use cached_inference_adapter::CachedInferenceAdapter;
pub use caldav_calendar_adapter::CalDavCalendarAdapter;
pub use circuit_breaker::{
    CircuitBreaker, CircuitBreakerConfig, CircuitBreakerError, CircuitOpenError, CircuitState,
};
pub use degraded_inference::{
    DegradedInferenceAdapter, DegradedModeConfig, DegradedModeStats, ServiceStatus,
};
pub use encryption_adapter::ChaChaEncryptionAdapter;
pub use env_secret_store::EnvSecretStore;
pub use model_registry_adapter::OllamaModelRegistryAdapter;
pub use ollama_inference_adapter::OllamaInferenceAdapter;
pub use proton_email_adapter::ProtonEmailAdapter;
pub use signal_adapter::SignalMessengerAdapter;
pub use speech_adapter::SpeechAdapter;
pub use suspicious_activity_adapter::InMemorySuspiciousActivityTracker;
pub use task_adapter::TaskAdapter;
pub use vault_secret_store::{ChainedSecretStore, VaultConfig, VaultSecretStore};
pub use weather_adapter::WeatherAdapter;
pub use websearch_adapter::WebSearchAdapter;
pub use whatsapp_adapter::WhatsAppMessengerAdapter;
