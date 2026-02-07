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

    // =========================================================================
    // Concurrency Tests
    // =========================================================================

    mod concurrency_tests {
        use super::*;
        use std::sync::Arc;
        use std::sync::atomic::{AtomicUsize, Ordering};

        #[tokio::test]
        async fn concurrent_reads_same_key() {
            let cache = Arc::new(create_multi_cache());
            let data = TestData {
                value: "shared".to_string(),
            };

            // Pre-populate the cache
            cache
                .set("shared_key", &data, Duration::from_secs(60))
                .await
                .unwrap();

            // Spawn 100 concurrent read tasks
            let mut handles = Vec::new();
            for _ in 0..100 {
                let cache_clone = Arc::clone(&cache);
                handles.push(tokio::spawn(async move {
                    let result: Option<TestData> = cache_clone.get("shared_key").await.unwrap();
                    result
                }));
            }

            // All reads should succeed and return the same value
            for handle in handles {
                let result = handle.await.unwrap();
                assert_eq!(
                    result,
                    Some(TestData {
                        value: "shared".to_string()
                    })
                );
            }
        }

        #[tokio::test]
        async fn concurrent_writes_different_keys() {
            let cache = Arc::new(create_multi_cache());

            // Spawn 50 concurrent write tasks, each with a different key
            let mut handles = Vec::new();
            for i in 0..50 {
                let cache_clone = Arc::clone(&cache);
                handles.push(tokio::spawn(async move {
                    let data = TestData {
                        value: format!("value_{i}"),
                    };
                    cache_clone
                        .set(&format!("key_{i}"), &data, Duration::from_secs(60))
                        .await
                }));
            }

            // All writes should succeed
            for handle in handles {
                handle.await.unwrap().unwrap();
            }

            // Verify all values are present
            for i in 0..50 {
                let result: Option<TestData> = cache.get(&format!("key_{i}")).await.unwrap();
                assert_eq!(
                    result,
                    Some(TestData {
                        value: format!("value_{i}")
                    })
                );
            }
        }

        #[tokio::test]
        async fn concurrent_reads_and_writes_same_key() {
            let cache = Arc::new(create_multi_cache());
            let success_count = Arc::new(AtomicUsize::new(0));

            // Initial value
            cache
                .set(
                    "contested_key",
                    &TestData {
                        value: "initial".to_string(),
                    },
                    Duration::from_secs(60),
                )
                .await
                .unwrap();

            // Spawn mixed read/write tasks
            let mut handles = Vec::new();

            // 50 readers
            for _ in 0..50 {
                let cache_clone = Arc::clone(&cache);
                let counter = Arc::clone(&success_count);
                handles.push(tokio::spawn(async move {
                    let result: Result<Option<TestData>, _> =
                        cache_clone.get("contested_key").await;
                    if result.is_ok() {
                        counter.fetch_add(1, Ordering::SeqCst);
                    }
                }));
            }

            // 25 writers
            for i in 0..25 {
                let cache_clone = Arc::clone(&cache);
                let counter = Arc::clone(&success_count);
                handles.push(tokio::spawn(async move {
                    let data = TestData {
                        value: format!("updated_{i}"),
                    };
                    let result = cache_clone
                        .set("contested_key", &data, Duration::from_secs(60))
                        .await;
                    if result.is_ok() {
                        counter.fetch_add(1, Ordering::SeqCst);
                    }
                }));
            }

            // Wait for all tasks
            for handle in handles {
                let _ = handle.await;
            }

            // All operations should have succeeded (75 total)
            assert_eq!(success_count.load(Ordering::SeqCst), 75);

            // Final value should exist (we don't care which write won)
            let final_result: Option<TestData> = cache.get("contested_key").await.unwrap();
            assert!(final_result.is_some());
        }

        #[tokio::test]
        async fn concurrent_invalidations() {
            let cache = Arc::new(create_multi_cache());

            // Pre-populate with many keys
            for i in 0..100 {
                cache
                    .set(
                        &format!("batch_{i}"),
                        &TestData {
                            value: format!("v{i}"),
                        },
                        Duration::from_secs(60),
                    )
                    .await
                    .unwrap();
            }

            // Spawn concurrent invalidation tasks
            let mut handles = Vec::new();
            for i in 0..50 {
                let cache_clone = Arc::clone(&cache);
                handles.push(tokio::spawn(async move {
                    cache_clone.invalidate(&format!("batch_{i}")).await
                }));
            }

            // All invalidations should succeed
            for handle in handles {
                handle.await.unwrap().unwrap();
            }

            // First 50 keys should be gone
            for i in 0..50 {
                assert!(!cache.exists(&format!("batch_{i}")).await.unwrap());
            }

            // Remaining 50 keys should still exist
            for i in 50..100 {
                assert!(cache.exists(&format!("batch_{i}")).await.unwrap());
            }
        }

        #[tokio::test]
        async fn concurrent_pattern_invalidation() {
            let cache = Arc::new(create_multi_cache());

            // Pre-populate with keys in different namespaces
            for i in 0..50 {
                cache
                    .set(&format!("ns_a:{i}"), &i, Duration::from_secs(60))
                    .await
                    .unwrap();
                cache
                    .set(&format!("ns_b:{i}"), &i, Duration::from_secs(60))
                    .await
                    .unwrap();
            }

            // Concurrent pattern invalidations for different namespaces
            let cache_a = Arc::clone(&cache);
            let cache_b = Arc::clone(&cache);

            let (count_a, count_b) = tokio::join!(
                async move { cache_a.invalidate_pattern("ns_a:*").await.unwrap() },
                async move { cache_b.invalidate_pattern("ns_b:*").await.unwrap() }
            );

            assert_eq!(count_a, 50);
            assert_eq!(count_b, 50);

            // All keys should be gone
            for i in 0..50 {
                assert!(!cache.exists(&format!("ns_a:{i}")).await.unwrap());
                assert!(!cache.exists(&format!("ns_b:{i}")).await.unwrap());
            }
        }

        #[tokio::test]
        async fn l2_promotion_under_concurrent_access() {
            let l1 = MokaCache::new();
            let l2 = RedbCache::in_memory().unwrap();

            // Only populate L2
            for i in 0..20 {
                l2.set(
                    &format!("promote_{i}"),
                    &TestData {
                        value: format!("l2_value_{i}"),
                    },
                    Duration::from_secs(60),
                )
                .await
                .unwrap();
            }

            let cache = Arc::new(MultiLayerCache::new(l1, l2));

            // Concurrent reads should all trigger promotion without race conditions
            let mut handles = Vec::new();
            for i in 0..20 {
                let cache_clone = Arc::clone(&cache);
                handles.push(tokio::spawn(async move {
                    // Multiple reads of the same key to stress promotion
                    for _ in 0..5 {
                        let result: Option<TestData> =
                            cache_clone.get(&format!("promote_{i}")).await.unwrap();
                        assert!(result.is_some());
                    }
                }));
            }

            for handle in handles {
                handle.await.unwrap();
            }

            // After promotion, all keys should be in L1
            for i in 0..20 {
                let l1_result: Option<TestData> =
                    cache.l1().get(&format!("promote_{i}")).await.unwrap();
                assert!(
                    l1_result.is_some(),
                    "Key promote_{i} should be promoted to L1"
                );
            }
        }
    }
}
