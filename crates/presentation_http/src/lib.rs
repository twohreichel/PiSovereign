//! PiSovereign HTTP presentation layer
//!
//! This crate provides the HTTP API for PiSovereign.

pub mod config_reload;
pub mod error;
pub mod handlers;
pub mod middleware;
pub mod openapi;
pub mod routes;
pub mod state;

pub use config_reload::{ReloadableConfig, spawn_config_reload_handler};
pub use error::ApiError;
pub use middleware::{
    ApiKeyAuthLayer, ApiKeyStore, RateLimiterConfig, RateLimiterLayer, RequestId, RequestIdLayer,
    SecurityHeadersLayer, ValidatedJson, ValidationError, spawn_cleanup_task,
};
pub use openapi::{ApiDoc, create_openapi_routes};
pub use routes::create_router;
pub use state::AppState;
