//! PiSovereign HTTP presentation layer
//!
//! This crate provides the HTTP API for PiSovereign.

pub mod config_reload;
pub mod error;
pub mod handlers;
pub mod middleware;
pub mod routes;
pub mod state;

pub use config_reload::{ReloadableConfig, spawn_config_reload_handler};
pub use error::ApiError;
pub use middleware::{
    ApiKeyAuthLayer, RateLimiterConfig, RateLimiterLayer, ValidatedJson, ValidationError,
};
pub use routes::create_router;
pub use state::AppState;
