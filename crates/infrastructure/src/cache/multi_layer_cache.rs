//! Multi-layer cache implementation
//!
//! Combines L1 (in-memory) and L2 (persistent) caches using the
//! Cache-Aside pattern for optimal performance and durability.

use std::time::Duration;

use application::{
    error::ApplicationError,
    ports::{CachePort, CacheStats},
};
use async_trait::async_trait;
use tracing::{debug, instrument};

use super::{MokaCache, RedbCache};

/// Multi-layer cache combining L1 (fast) and L2 (persistent) caches
///
/// Read path: L1 -> miss -> L2 -> miss -> return None
/// Write path: Write to L1 and L2 (write-through)
/// Invalidation: Invalidate both layers
pub struct MultiLayerCache {
    /// L1: Fast in-memory cache (Moka)
    l1: MokaCache,
    /// L2: Persistent cache (Redb)
    l2: RedbCache,
}

impl std::fmt::Debug for MultiLayerCache {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MultiLayerCache")
            .field("l1", &self.l1)
            .field("l2", &self.l2)
            .finish()
    }
}

impl MultiLayerCache {
    /// Create a new multi-layer cache with the given L1 and L2 caches
    #[must_use]
    pub const fn new(l1: MokaCache, l2: RedbCache) -> Self {
        Self { l1, l2 }
    }

    /// Get the L1 cache for direct access (e.g., stats)
    #[must_use]
    pub const fn l1(&self) -> &MokaCache {
        &self.l1
    }

    /// Get the L2 cache for direct access (e.g., stats)
    #[must_use]
    pub const fn l2(&self) -> &RedbCache {
        &self.l2
    }
}

#[async_trait]
impl CachePort for MultiLayerCache {
    #[instrument(skip(self), level = "debug")]
    async fn get_bytes(&self, key: &str) -> Result<Option<Vec<u8>>, ApplicationError> {
        // Try L1 first (fast path)
        if let Some(value) = self.l1.get_bytes(key).await? {
            debug!(key = %key, layer = "L1", "Cache hit");
            return Ok(Some(value));
        }

        // Try L2 (slower, persistent)
        if let Some(value) = self.l2.get_bytes(key).await? {
            debug!(key = %key, layer = "L2", "Cache hit, promoting to L1");

            // Promote to L1 for faster future access
            // Use a reasonable TTL for the promotion
            if let Err(e) = self
                .l1
                .set_bytes(key, value.clone(), Duration::from_secs(1800))
                .await
            {
                // Log but don't fail - L2 hit is still a success
                tracing::warn!(error = %e, key = %key, "Failed to promote L2 hit to L1");
            }

            return Ok(Some(value));
        }

        debug!(key = %key, "Cache miss (all layers)");
        Ok(None)
    }

    #[instrument(skip(self, value), level = "debug")]
    async fn set_bytes(
        &self,
        key: &str,
        value: Vec<u8>,
        ttl: Duration,
    ) -> Result<(), ApplicationError> {
        // Write-through: write to both layers
        // Write to L1 first (fast), then L2 (durable)
        self.l1.set_bytes(key, value.clone(), ttl).await?;
        self.l2.set_bytes(key, value, ttl).await?;

        debug!(key = %key, ttl_secs = ttl.as_secs(), "Cache set (both layers)");
        Ok(())
    }

    #[instrument(skip(self), level = "debug")]
    async fn invalidate(&self, key: &str) -> Result<(), ApplicationError> {
        // Invalidate both layers
        self.l1.invalidate(key).await?;
        self.l2.invalidate(key).await?;

        debug!(key = %key, "Cache invalidated (both layers)");
        Ok(())
    }

    #[instrument(skip(self), level = "debug")]
    async fn invalidate_pattern(&self, pattern: &str) -> Result<u64, ApplicationError> {
        // Invalidate in both layers, sum the counts
        let l1_count = self.l1.invalidate_pattern(pattern).await?;
        let l2_count = self.l2.invalidate_pattern(pattern).await?;

        // Return max since same keys may be in both layers
        let count = l1_count.max(l2_count);
        debug!(pattern = %pattern, l1 = l1_count, l2 = l2_count, "Pattern invalidation complete");
        Ok(count)
    }

    #[instrument(skip(self), level = "debug")]
    async fn exists(&self, key: &str) -> Result<bool, ApplicationError> {
        // Check L1 first, then L2
        if self.l1.exists(key).await? {
            return Ok(true);
        }
        self.l2.exists(key).await
    }

    fn stats(&self) -> CacheStats {
        let l1_stats = self.l1.stats();
        let l2_stats = self.l2.stats();

        // Combine stats from both layers
        CacheStats {
            hits: l1_stats.hits + l2_stats.hits,
            misses: l1_stats.misses.max(l2_stats.misses), // L1 miss leads to L2 check
            entries: l1_stats.entries + l2_stats.entries,
            memory_bytes: l1_stats.memory_bytes + l2_stats.memory_bytes,
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
    }

    fn create_multi_cache() -> MultiLayerCache {
        let l1 = MokaCache::new();
        let l2 = RedbCache::in_memory().unwrap();
        MultiLayerCache::new(l1, l2)
    }

    #[tokio::test]
    async fn set_and_get_from_l1() {
        let cache = create_multi_cache();
        let data = TestData {
            value: "test".to_string(),
        };

        cache
            .set("key", &data, Duration::from_secs(60))
            .await
            .unwrap();

        let result: Option<TestData> = cache.get("key").await.unwrap();
        assert_eq!(result, Some(data));

        // Verify it's in L1
        let l1_result: Option<TestData> = cache.l1().get("key").await.unwrap();
        assert!(l1_result.is_some());
    }

    #[tokio::test]
    async fn promotes_l2_hit_to_l1() {
        let l1 = MokaCache::new();
        let l2 = RedbCache::in_memory().unwrap();

        let data = TestData {
            value: "l2_only".to_string(),
        };

        // Set only in L2 directly
        l2.set("key", &data, Duration::from_secs(60)).await.unwrap();

        // Verify not in L1
        let l1_before: Option<TestData> = l1.get("key").await.unwrap();
        assert!(l1_before.is_none());

        let cache = MultiLayerCache::new(l1, l2);

        // Get through multi-layer (should promote)
        let result: Option<TestData> = cache.get("key").await.unwrap();
        assert_eq!(result, Some(data.clone()));

        // Now it should be in L1
        let l1_after: Option<TestData> = cache.l1().get("key").await.unwrap();
        assert_eq!(l1_after, Some(data));
    }

    #[tokio::test]
    async fn invalidate_removes_from_both_layers() {
        let cache = create_multi_cache();

        cache
            .set("key", &"value".to_string(), Duration::from_secs(60))
            .await
            .unwrap();

        cache.invalidate("key").await.unwrap();

        // Verify removed from both layers
        assert!(!cache.l1().exists("key").await.unwrap());
        assert!(!cache.l2().exists("key").await.unwrap());
    }

    #[tokio::test]
    async fn invalidate_pattern_removes_from_both_layers() {
        let cache = create_multi_cache();

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

        // Verify removed from both layers
        assert!(!cache.l1().exists("prefix:a").await.unwrap());
        assert!(!cache.l2().exists("prefix:a").await.unwrap());
        assert!(cache.l1().exists("other:c").await.unwrap());
    }

    #[tokio::test]
    async fn stats_combines_both_layers() {
        let cache = create_multi_cache();

        cache
            .set("key1", &1, Duration::from_secs(60))
            .await
            .unwrap();
        cache
            .set("key2", &2, Duration::from_secs(60))
            .await
            .unwrap();

        let stats = cache.stats();
        // Stats should reflect entries in both layers
        // L1 (Moka) entry count may be delayed, so check ranges
        let l2_stats = cache.l2().stats();

        // L2 (Redb) should have 2 entries
        assert_eq!(l2_stats.entries, 2);

        // Combined stats should include both layers
        assert!(stats.entries >= 2, "Should have at least L2 entries");
        assert!(stats.entries <= 4, "Should have at most L1 + L2 entries");
    }

    #[tokio::test]
    async fn exists_checks_both_layers() {
        let l1 = MokaCache::new();
        let l2 = RedbCache::in_memory().unwrap();

        // Add to L2 only
        l2.set("l2_only", &"value".to_string(), Duration::from_secs(60))
            .await
            .unwrap();

        let cache = MultiLayerCache::new(l1, l2);

        // Should find it via multi-layer
        assert!(cache.exists("l2_only").await.unwrap());

        // Add to L1 only
        cache
            .l1()
            .set("l1_only", &"value".to_string(), Duration::from_secs(60))
            .await
            .unwrap();

        // Should find it via multi-layer
        assert!(cache.exists("l1_only").await.unwrap());

        // Non-existent key
        assert!(!cache.exists("missing").await.unwrap());
    }
}
