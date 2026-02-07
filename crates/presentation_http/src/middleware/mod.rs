//! HTTP middleware components
//!
//! This module contains middleware for authentication, rate limiting,
//! request ID correlation, security headers, and other cross-cutting concerns.

pub mod auth;
pub mod rate_limit;
pub mod request_id;
pub mod security_headers;
pub mod validation;

pub use auth::{ApiKeyAuth, ApiKeyAuthLayer, ApiKeyStore};
pub use rate_limit::{
    RateLimiter, RateLimiterConfig, RateLimiterLayer, RateLimiterState, spawn_cleanup_task,
};
pub use request_id::{REQUEST_ID_HEADER, RequestId, RequestIdLayer};
pub use security_headers::{SecurityHeaders, SecurityHeadersLayer};
pub use validation::{ValidatedJson, ValidationError};
