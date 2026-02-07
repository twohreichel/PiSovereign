//! Configuration validation module
//!
//! Provides security validation and startup checks for application configuration.

pub mod security;

pub use security::{SecurityValidator, SecurityWarning, WarningSeverity};
