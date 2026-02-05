//! Cache port definition
//!
//! Defines the interface for caching operations used throughout the application.
//! Implementations may use in-memory caches (Moka), embedded stores (Sled),
//! or distributed caches (Redis).

use std::time::Duration;

use async_trait::async_trait;

use crate::error::ApplicationError;

/// Cache port for storing and retrieving cached values
///
/// Implementations should be thread-safe and support async operations.
/// Values are stored as raw bytes - callers handle serialization.
#[async_trait]
pub trait CachePort: Send + Sync + std::fmt::Debug {
    /// Get a cached value by key
    ///
    /// Returns `None` if the key doesn't exist or has expired.
    async fn get_bytes(&self, key: &str) -> Result<Option<Vec<u8>>, ApplicationError>;

    /// Set a cached value with a time-to-live
    ///
    /// If the key already exists, its value and TTL are updated.
    async fn set_bytes(
        &self,
        key: &str,
        value: Vec<u8>,
        ttl: Duration,
    ) -> Result<(), ApplicationError>;

    /// Invalidate (delete) a single cache entry
    async fn invalidate(&self, key: &str) -> Result<(), ApplicationError>;

    /// Invalidate all cache entries matching a pattern
    ///
    /// The pattern format depends on the implementation:
    /// - For simple caches: prefix matching (e.g., "llm:*")
    /// - For Redis: glob patterns
    async fn invalidate_pattern(&self, pattern: &str) -> Result<u64, ApplicationError>;

    /// Check if a key exists in the cache (without deserializing)
    async fn exists(&self, key: &str) -> Result<bool, ApplicationError>;

    /// Get cache statistics (hits, misses, size)
    fn stats(&self) -> CacheStats;
}

/// Extension trait for typed cache operations
///
/// Provides convenient typed get/set methods on top of the raw byte interface.
#[async_trait]
pub trait CachePortExt: CachePort {
    /// Get a typed value from cache
    async fn get<T>(&self, key: &str) -> Result<Option<T>, ApplicationError>
    where
        T: serde::de::DeserializeOwned + Send,
    {
        match self.get_bytes(key).await? {
            Some(bytes) => {
                let value: T = serde_json::from_slice(&bytes).map_err(|e| {
                    ApplicationError::Internal(format!("Cache deserialization error: {e}"))
                })?;
                Ok(Some(value))
            },
            None => Ok(None),
        }
    }

    /// Set a typed value in cache
    async fn set<T>(&self, key: &str, value: &T, ttl: Duration) -> Result<(), ApplicationError>
    where
        T: serde::Serialize + Send + Sync,
    {
        let bytes = serde_json::to_vec(value)
            .map_err(|e| ApplicationError::Internal(format!("Cache serialization error: {e}")))?;
        self.set_bytes(key, bytes, ttl).await
    }
}

// Blanket implementation for all CachePort implementors
impl<T: CachePort + ?Sized> CachePortExt for T {}

/// Cache statistics for monitoring
#[derive(Debug, Clone, Default)]
pub struct CacheStats {
    /// Number of cache hits
    pub hits: u64,
    /// Number of cache misses
    pub misses: u64,
    /// Current number of entries
    pub entries: u64,
    /// Approximate memory usage in bytes
    pub memory_bytes: u64,
}

impl CacheStats {
    /// Calculate the hit rate as a percentage (0.0 - 1.0)
    #[must_use]
    #[allow(clippy::cast_precision_loss)]
    pub fn hit_rate(&self) -> f64 {
        let total = self.hits + self.misses;
        if total == 0 {
            0.0
        } else {
            // Precision loss is acceptable for statistics display
            self.hits as f64 / total as f64
        }
    }
}

/// Standard TTL values for different cache categories
pub mod ttl {
    use std::time::Duration;

    /// Short TTL for frequently changing data (5 minutes)
    pub const SHORT: Duration = Duration::from_secs(5 * 60);

    /// Medium TTL for moderately stable data (1 hour)
    pub const MEDIUM: Duration = Duration::from_secs(60 * 60);

    /// Long TTL for stable data (24 hours)
    pub const LONG: Duration = Duration::from_secs(24 * 60 * 60);

    /// TTL for LLM responses with high temperature (1 hour)
    pub const LLM_DYNAMIC: Duration = Duration::from_secs(60 * 60);

    /// TTL for LLM responses with low temperature (24 hours)
    pub const LLM_STABLE: Duration = Duration::from_secs(24 * 60 * 60);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cache_stats_hit_rate_zero_when_empty() {
        let stats = CacheStats::default();
        assert!(stats.hit_rate().abs() < f64::EPSILON);
    }

    #[test]
    fn cache_stats_hit_rate_calculates_correctly() {
        let stats = CacheStats {
            hits: 75,
            misses: 25,
            entries: 100,
            memory_bytes: 1024,
        };
        assert!((stats.hit_rate() - 0.75).abs() < f64::EPSILON);
    }

    #[test]
    fn cache_stats_hit_rate_all_hits() {
        let stats = CacheStats {
            hits: 100,
            misses: 0,
            entries: 50,
            memory_bytes: 512,
        };
        assert!((stats.hit_rate() - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn cache_stats_hit_rate_all_misses() {
        let stats = CacheStats {
            hits: 0,
            misses: 100,
            entries: 0,
            memory_bytes: 0,
        };
        assert!(stats.hit_rate().abs() < f64::EPSILON);
    }

    #[test]
    fn ttl_values_are_reasonable() {
        assert!(ttl::SHORT < ttl::MEDIUM);
        assert!(ttl::MEDIUM < ttl::LONG);
        assert_eq!(ttl::LLM_DYNAMIC, ttl::MEDIUM);
        assert_eq!(ttl::LLM_STABLE, ttl::LONG);
    }
}
