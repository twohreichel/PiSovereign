//! Moka in-memory cache implementation
//!
//! High-performance, thread-safe in-memory cache with TTL support.
//! Suitable for L1 caching layer with automatic eviction.

use std::{
    sync::atomic::{AtomicU64, Ordering},
    time::Duration,
};

use application::{
    error::ApplicationError,
    ports::{CachePort, CacheStats},
};
use async_trait::async_trait;
use moka::future::Cache;
use tracing::{debug, instrument};

/// Maximum cache size in MB (default: 100MB for Pi)
const DEFAULT_MAX_CAPACITY_MB: u64 = 100;

/// Configuration for Moka cache
#[derive(Debug, Clone, Copy)]
pub struct MokaCacheConfig {
    /// Maximum capacity in megabytes
    pub max_capacity_mb: u64,
    /// Default TTL for entries
    pub default_ttl: Duration,
    /// Time to idle before eviction (optional)
    pub time_to_idle: Option<Duration>,
}

impl Default for MokaCacheConfig {
    fn default() -> Self {
        Self {
            max_capacity_mb: DEFAULT_MAX_CAPACITY_MB,
            default_ttl: Duration::from_secs(3600), // 1 hour
            time_to_idle: Some(Duration::from_secs(1800)), // 30 minutes
        }
    }
}

/// Moka-based in-memory cache
///
/// Uses Moka's async cache for high-performance concurrent access.
/// Automatically evicts entries based on TTL and memory pressure.
///
/// Note: Moka 0.12 uses a global TTL configured at build time. Per-entry TTL
/// requires the `Expiry` trait which adds complexity. For this implementation,
/// we use the cache-level TTL which is sufficient for most use cases.
pub struct MokaCache {
    cache: Cache<String, Vec<u8>>,
    hits: AtomicU64,
    misses: AtomicU64,
}

impl std::fmt::Debug for MokaCache {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MokaCache")
            .field("entries", &self.cache.entry_count())
            .field("hits", &self.hits.load(Ordering::Relaxed))
            .field("misses", &self.misses.load(Ordering::Relaxed))
            .finish()
    }
}

impl MokaCache {
    /// Create a new Moka cache with default configuration
    #[must_use]
    pub fn new() -> Self {
        Self::with_config(MokaCacheConfig::default())
    }

    /// Create a new Moka cache with custom configuration
    #[must_use]
    pub fn with_config(config: MokaCacheConfig) -> Self {
        let max_capacity_bytes = config.max_capacity_mb * 1024 * 1024;

        let mut builder = Cache::builder()
            .max_capacity(max_capacity_bytes)
            .time_to_live(config.default_ttl)
            .weigher(|_key: &String, value: &Vec<u8>| -> u32 {
                // Weight by size in bytes, capped at u32::MAX
                value.len().try_into().unwrap_or(u32::MAX)
            });

        if let Some(tti) = config.time_to_idle {
            builder = builder.time_to_idle(tti);
        }

        Self {
            cache: builder.build(),
            hits: AtomicU64::new(0),
            misses: AtomicU64::new(0),
        }
    }

    /// Create a cache optimized for LLM response caching
    #[must_use]
    pub fn for_llm_responses() -> Self {
        Self::with_config(MokaCacheConfig {
            max_capacity_mb: 50,                           // Lower for Pi memory constraints
            default_ttl: Duration::from_secs(3600),        // 1 hour
            time_to_idle: Some(Duration::from_secs(1800)), // 30 min idle
        })
    }

    /// Estimate memory usage based on entry count and average size
    fn estimate_memory(&self) -> u64 {
        // Rough estimate: entry count * average entry size
        // Moka doesn't expose exact memory usage, so we estimate
        self.cache.entry_count() * 512 // Assume ~512 bytes average
    }
}

impl Default for MokaCache {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl CachePort for MokaCache {
    #[instrument(skip(self), level = "debug")]
    #[allow(clippy::option_if_let_else)]
    async fn get_bytes(&self, key: &str) -> Result<Option<Vec<u8>>, ApplicationError> {
        if let Some(bytes) = self.cache.get(key).await {
            self.hits.fetch_add(1, Ordering::Relaxed);
            debug!(key = %key, "Cache hit");
            Ok(Some(bytes))
        } else {
            self.misses.fetch_add(1, Ordering::Relaxed);
            debug!(key = %key, "Cache miss");
            Ok(None)
        }
    }

    #[instrument(skip(self, value), level = "debug")]
    async fn set_bytes(
        &self,
        key: &str,
        value: Vec<u8>,
        _ttl: Duration,
    ) -> Result<(), ApplicationError> {
        // Note: Moka 0.12 uses cache-level TTL, not per-entry TTL
        // The ttl parameter is ignored; entries use the cache's configured TTL
        self.cache.insert(key.to_string(), value).await;
        debug!(key = %key, "Cache set");
        Ok(())
    }

    #[instrument(skip(self), level = "debug")]
    async fn invalidate(&self, key: &str) -> Result<(), ApplicationError> {
        self.cache.invalidate(key).await;
        debug!(key = %key, "Cache invalidated");
        Ok(())
    }

    #[instrument(skip(self), level = "debug")]
    async fn invalidate_pattern(&self, pattern: &str) -> Result<u64, ApplicationError> {
        // Moka doesn't support pattern-based invalidation directly
        // We need to iterate and check prefixes
        let prefix = pattern.trim_end_matches('*');
        let mut count = 0u64;

        // Run pending maintenance tasks before iteration
        self.cache.run_pending_tasks().await;

        // Collect keys to invalidate (can't modify while iterating)
        // Iterator returns (Arc<K>, V), we need to dereference the Arc
        let keys_to_remove: Vec<String> = self
            .cache
            .iter()
            .filter(|(k, _)| k.starts_with(prefix))
            .map(|(k, _)| (*k).clone())
            .collect();

        for key in keys_to_remove {
            self.cache.invalidate(&key).await;
            count += 1;
        }

        debug!(pattern = %pattern, count = count, "Pattern invalidation complete");
        Ok(count)
    }

    #[instrument(skip(self), level = "debug")]
    async fn exists(&self, key: &str) -> Result<bool, ApplicationError> {
        Ok(self.cache.contains_key(key))
    }

    fn stats(&self) -> CacheStats {
        CacheStats {
            hits: self.hits.load(Ordering::Relaxed),
            misses: self.misses.load(Ordering::Relaxed),
            entries: self.cache.entry_count(),
            memory_bytes: self.estimate_memory(),
        }
    }
}

#[cfg(test)]
mod tests {
    use application::ports::CachePortExt;
    use serde::{Deserialize, Serialize};

    use super::*;

    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    struct TestData {
        value: String,
        count: i32,
    }

    #[tokio::test]
    async fn set_and_get_value() {
        let cache = MokaCache::new();
        let data = TestData {
            value: "hello".to_string(),
            count: 42,
        };

        cache
            .set("test_key", &data, Duration::from_secs(60))
            .await
            .unwrap();

        let retrieved: Option<TestData> = cache.get("test_key").await.unwrap();
        assert_eq!(retrieved, Some(data));
    }

    #[tokio::test]
    async fn get_nonexistent_returns_none() {
        let cache = MokaCache::new();
        let result: Option<TestData> = cache.get("nonexistent").await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn invalidate_removes_entry() {
        let cache = MokaCache::new();
        cache
            .set("key", &"value".to_string(), Duration::from_secs(60))
            .await
            .unwrap();

        cache.invalidate("key").await.unwrap();

        let result: Option<String> = cache.get("key").await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn exists_returns_true_for_existing_key() {
        let cache = MokaCache::new();
        cache
            .set("key", &"value".to_string(), Duration::from_secs(60))
            .await
            .unwrap();

        assert!(cache.exists("key").await.unwrap());
    }

    #[tokio::test]
    async fn exists_returns_false_for_missing_key() {
        let cache = MokaCache::new();
        assert!(!cache.exists("missing").await.unwrap());
    }

    #[tokio::test]
    async fn invalidate_pattern_removes_matching_keys() {
        let cache = MokaCache::new();
        cache
            .set("prefix:a", &1, Duration::from_secs(60))
            .await
            .unwrap();
        cache
            .set("prefix:b", &2, Duration::from_secs(60))
            .await
            .unwrap();
        cache
            .set("other:c", &3, Duration::from_secs(60))
            .await
            .unwrap();

        let count = cache.invalidate_pattern("prefix:*").await.unwrap();

        assert_eq!(count, 2);
        assert!(!cache.exists("prefix:a").await.unwrap());
        assert!(!cache.exists("prefix:b").await.unwrap());
        assert!(cache.exists("other:c").await.unwrap());
    }

    #[tokio::test]
    async fn stats_tracks_hits_and_misses() {
        let cache = MokaCache::new();
        cache
            .set("key", &"value".to_string(), Duration::from_secs(60))
            .await
            .unwrap();

        // One hit
        let _: Option<String> = cache.get("key").await.unwrap();
        // Two misses
        let _: Option<String> = cache.get("missing1").await.unwrap();
        let _: Option<String> = cache.get("missing2").await.unwrap();

        let stats = cache.stats();
        assert_eq!(stats.hits, 1);
        assert_eq!(stats.misses, 2);
    }

    #[test]
    fn default_config_values() {
        let config = MokaCacheConfig::default();
        assert_eq!(config.max_capacity_mb, 100);
        assert_eq!(config.default_ttl, Duration::from_secs(3600));
    }

    #[tokio::test]
    async fn for_llm_responses_creates_optimized_cache() {
        let cache = MokaCache::for_llm_responses();
        // Verify it's usable
        cache
            .set("llm:test", &"response".to_string(), Duration::from_secs(60))
            .await
            .unwrap();
        let result: Option<String> = cache.get("llm:test").await.unwrap();
        assert_eq!(result, Some("response".to_string()));
    }

    #[test]
    fn moka_cache_debug() {
        let cache = MokaCache::new();
        let debug = format!("{cache:?}");
        assert!(debug.contains("MokaCache"));
        assert!(debug.contains("entries"));
        assert!(debug.contains("hits"));
        assert!(debug.contains("misses"));
    }

    #[test]
    fn moka_cache_default() {
        let cache = MokaCache::default();
        let stats = cache.stats();
        assert_eq!(stats.entries, 0);
        assert_eq!(stats.hits, 0);
        assert_eq!(stats.misses, 0);
    }

    #[test]
    fn moka_config_copy() {
        let config = MokaCacheConfig::default();
        let copied = config;
        assert_eq!(config.max_capacity_mb, copied.max_capacity_mb);
    }

    #[tokio::test]
    async fn with_config_custom_settings() {
        let config = MokaCacheConfig {
            max_capacity_mb: 10,
            default_ttl: Duration::from_secs(60),
            time_to_idle: None,
        };
        let cache = MokaCache::with_config(config);
        cache
            .set("test", &42i32, Duration::from_secs(30))
            .await
            .unwrap();
        let result: Option<i32> = cache.get("test").await.unwrap();
        assert_eq!(result, Some(42));
    }

    #[tokio::test]
    async fn stats_shows_entry_count() {
        let cache = MokaCache::new();
        cache
            .set("key1", &1, Duration::from_secs(60))
            .await
            .unwrap();
        cache
            .set("key2", &2, Duration::from_secs(60))
            .await
            .unwrap();
        cache
            .set("key3", &3, Duration::from_secs(60))
            .await
            .unwrap();

        // Run pending tasks to ensure entries are counted
        cache.cache.run_pending_tasks().await;

        let stats = cache.stats();
        assert_eq!(stats.entries, 3);
    }

    #[tokio::test]
    async fn invalidate_pattern_no_matches() {
        let cache = MokaCache::new();
        cache
            .set("other:key", &1, Duration::from_secs(60))
            .await
            .unwrap();

        let count = cache.invalidate_pattern("nomatch:*").await.unwrap();
        assert_eq!(count, 0);

        // Original key should still exist
        assert!(cache.exists("other:key").await.unwrap());
    }

    #[tokio::test]
    async fn get_bytes_and_set_bytes_directly() {
        let cache = MokaCache::new();
        let data = b"raw binary data";

        cache
            .set_bytes("binary_key", data.to_vec(), Duration::from_secs(60))
            .await
            .unwrap();

        let result = cache.get_bytes("binary_key").await.unwrap();
        assert_eq!(result, Some(data.to_vec()));
    }
}
