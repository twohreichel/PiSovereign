//! Infrastructure layer - Adapters for external systems
//!
//! Implements ports defined in the application layer.
//! Contains adapters for Hailo inference, databases, external APIs, etc.

pub mod adapters;
pub mod cache;
pub mod config;
pub mod persistence;

pub use adapters::*;
pub use cache::{MokaCache, MultiLayerCache, SledCache, generate_cache_key, llm_cache_key};
pub use config::{AppConfig, DatabaseConfig, SecurityConfig, ServerConfig, WhatsAppConfig};
pub use persistence::{ConnectionPool, SqliteConversationStore, create_pool};
