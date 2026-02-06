//! Domain layer for PiSovereign
//!
//! Contains core business logic, entities, value objects, and domain errors.
//! This layer has no external dependencies and defines the ubiquitous language.

pub mod commands;
pub mod entities;
pub mod errors;
pub mod value_objects;

// Re-export tenant module for convenient access
pub use value_objects::tenant;

pub use commands::{AgentCommand, SystemCommand};
pub use entities::*;
pub use errors::DomainError;
pub use value_objects::*;
