//! Testing utilities for infrastructure integration tests.
//!
//! This module provides utilities for running integration tests with real
//! containerized databases using testcontainers.
//!
//! # Features
//!
//! - PostgreSQL container support for database integration tests
//! - Redis container support for caching integration tests
//! - Automatic cleanup after tests
//!
//! # Example
//!
//! ```ignore
//! use infrastructure::testing::{PostgresContainer, RedisContainer};
//!
//! #[tokio::test]
//! async fn test_with_postgres() {
//!     let container = PostgresContainer::start().await.unwrap();
//!     let pool = container.get_pool().await.unwrap();
//!     
//!     // Run tests with real database
//!     // Container is automatically cleaned up when dropped
//! }
//! ```

mod containers;
mod test_fixtures;

pub use containers::{
    PostgresContainer, PostgresContainerConfig, RedisContainer, RedisContainerConfig,
};
pub use test_fixtures::{TestConversation, TestFixtures, TestMessage};
