//! PiSovereign HTTP presentation layer
//!
//! This crate provides the HTTP API for PiSovereign.

pub mod error;
pub mod handlers;
pub mod routes;
pub mod state;

pub use error::ApiError;
pub use routes::create_router;
pub use state::AppState;
