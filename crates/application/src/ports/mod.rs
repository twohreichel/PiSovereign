//! Port definitions for application layer
//!
//! Ports are interfaces that define how the application interacts with
//! external systems. Adapters in the infrastructure layer implement these ports.

mod approval_queue;
mod audit_log;
mod cache_port;
mod calendar_port;
mod conversation_store;
mod database_health_port;
mod draft_store;
mod email_port;
mod inference_port;
mod message_gateway_port;
mod model_registry_port;
mod secret_store;
mod task_port;
mod user_profile_store;
mod weather_port;

pub use approval_queue::ApprovalQueuePort;
pub use audit_log::{AuditLogPort, AuditQuery};
pub use cache_port::{CachePort, CachePortExt, CacheStats, ttl};
pub use calendar_port::{CalendarError, CalendarEvent, CalendarInfo, CalendarPort, NewEvent};
pub use conversation_store::ConversationStore;
#[cfg(test)]
pub use database_health_port::MockDatabaseHealthPort;
pub use database_health_port::{DatabaseHealth, DatabaseHealthPort};
pub use draft_store::DraftStorePort;
#[cfg(test)]
pub use draft_store::MockDraftStorePort;
pub use email_port::{EmailDraft, EmailError, EmailPort, EmailSummary};
pub use inference_port::{InferencePort, InferenceResult, InferenceStream, StreamingChunk};
pub use message_gateway_port::{IncomingMessage, MessageGatewayPort, OutgoingMessage};
pub use model_registry_port::{ModelCapabilities, ModelCapability, ModelInfo, ModelRegistryPort};
pub use secret_store::{SecretStoreExt, SecretStorePort};
#[cfg(test)]
pub use task_port::MockTaskPort;
pub use task_port::{NewTask, Task, TaskPort, TaskQuery, TaskStatus, TaskUpdates};
pub use user_profile_store::UserProfileStore;
#[cfg(test)]
pub use weather_port::MockWeatherPort;
pub use weather_port::{CurrentWeather, DailyForecast, WeatherCondition, WeatherPort};
