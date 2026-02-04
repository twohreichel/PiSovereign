//! HTTP middleware components
//!
//! This module contains middleware for authentication, rate limiting,
//! and other cross-cutting concerns.

pub mod auth;
pub mod rate_limit;
pub mod validation;

pub use auth::{ApiKeyAuth, ApiKeyAuthLayer};
pub use rate_limit::{
    RateLimiter, RateLimiterConfig, RateLimiterLayer, RateLimiterState, spawn_cleanup_task,
};
pub use validation::{ValidatedJson, ValidationError};
