//! Sled embedded cache implementation
//!
//! Persistent key-value store for L2 caching.
//! Uses Sled for ACID-compliant storage without external dependencies.

use std::{
    path::Path,
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use application::{
    error::ApplicationError,
    ports::{CachePort, CacheStats},
};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use tracing::{debug, instrument};

/// Entry wrapper that includes expiration time
#[derive(Debug, Serialize, Deserialize)]
struct CacheEntry {
    /// Serialized value as bytes
    data: Vec<u8>,
    /// Expiration timestamp (Unix epoch seconds)
    expires_at: u64,
}

/// Sled-based persistent cache
///
/// Stores cache entries on disk with TTL support.
/// Suitable for L2 caching where persistence across restarts is desired.
pub struct SledCache {
    db: sled::Db,
    hits: AtomicU64,
    misses: AtomicU64,
}

impl std::fmt::Debug for SledCache {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SledCache")
            .field("entries", &self.db.len())
            .field("hits", &self.hits.load(Ordering::Relaxed))
            .field("misses", &self.misses.load(Ordering::Relaxed))
            .finish()
    }
}

impl SledCache {
    /// Create a new Sled cache at the specified path
    ///
    /// # Errors
    ///
    /// Returns an error if the database cannot be opened.
    pub fn new<P: AsRef<Path>>(path: P) -> Result<Self, ApplicationError> {
        let db = sled::open(path).map_err(|e| {
            ApplicationError::Internal(format!("Failed to open Sled database: {e}"))
        })?;

        Ok(Self {
            db,
            hits: AtomicU64::new(0),
            misses: AtomicU64::new(0),
        })
    }

    /// Create an in-memory Sled cache (for testing)
    #[cfg(test)]
    pub fn in_memory() -> Result<Self, ApplicationError> {
        let config = sled::Config::new().temporary(true);
        let db = config.open().map_err(|e| {
            ApplicationError::Internal(format!("Failed to create temporary Sled: {e}"))
        })?;

        Ok(Self {
            db,
            hits: AtomicU64::new(0),
            misses: AtomicU64::new(0),
        })
    }

    /// Get current Unix timestamp
    fn now_timestamp() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0)
    }

    /// Check if an entry has expired
    fn is_expired(entry: &CacheEntry) -> bool {
        Self::now_timestamp() >= entry.expires_at
    }

    /// Remove expired entries (background cleanup)
    pub fn cleanup_expired(&self) -> Result<u64, ApplicationError> {
        let mut removed = 0u64;
        let now = Self::now_timestamp();

        for result in self.db.iter() {
            let (key, value) = result
                .map_err(|e| ApplicationError::Internal(format!("Sled iteration error: {e}")))?;

            if let Ok(entry) = bincode::deserialize::<CacheEntry>(&value) {
                if now >= entry.expires_at {
                    self.db.remove(&key).map_err(|e| {
                        ApplicationError::Internal(format!("Sled remove error: {e}"))
                    })?;
                    removed += 1;
                }
            }
        }

        if removed > 0 {
            debug!(removed = removed, "Cleaned up expired cache entries");
        }

        Ok(removed)
    }
}

#[async_trait]
impl CachePort for SledCache {
    #[instrument(skip(self), level = "debug")]
    async fn get_bytes(&self, key: &str) -> Result<Option<Vec<u8>>, ApplicationError> {
        let db = self.db.clone();
        let key_str = key.to_string();

        // Sled operations are blocking, but fast for local SSD
        let result = tokio::task::spawn_blocking({
            let key_bytes = key_str.as_bytes().to_vec();
            move || db.get(&key_bytes)
        })
        .await
        .map_err(|e| ApplicationError::Internal(format!("Task join error: {e}")))?
        .map_err(|e| ApplicationError::Internal(format!("Sled get error: {e}")))?;

        if let Some(bytes) = result {
            let entry: CacheEntry = bincode::deserialize(&bytes).map_err(|e| {
                ApplicationError::Internal(format!("Cache entry deserialize error: {e}"))
            })?;

            // Check expiration
            if Self::is_expired(&entry) {
                self.misses.fetch_add(1, Ordering::Relaxed);
                // Lazy deletion - remove expired entry
                let _ = self.db.remove(key.as_bytes());
                debug!(key = %key, "Cache entry expired");
                return Ok(None);
            }

            self.hits.fetch_add(1, Ordering::Relaxed);
            debug!(key = %key, "Cache hit (Sled)");
            Ok(Some(entry.data))
        } else {
            self.misses.fetch_add(1, Ordering::Relaxed);
            debug!(key = %key, "Cache miss (Sled)");
            Ok(None)
        }
    }

    #[instrument(skip(self, value), level = "debug")]
    async fn set_bytes(
        &self,
        key: &str,
        value: Vec<u8>,
        ttl: Duration,
    ) -> Result<(), ApplicationError> {
        let expires_at = Self::now_timestamp() + ttl.as_secs();
        let entry = CacheEntry {
            data: value,
            expires_at,
        };

        let entry_bytes = bincode::serialize(&entry)
            .map_err(|e| ApplicationError::Internal(format!("Entry serialize error: {e}")))?;

        let db = self.db.clone();
        let key_str = key.to_string();
        let ttl_secs = ttl.as_secs();

        tokio::task::spawn_blocking({
            let key_bytes = key_str.as_bytes().to_vec();
            move || db.insert(&key_bytes, entry_bytes)
        })
        .await
        .map_err(|e| ApplicationError::Internal(format!("Task join error: {e}")))?
        .map_err(|e| ApplicationError::Internal(format!("Sled insert error: {e}")))?;

        debug!(key = %key_str, ttl_secs = ttl_secs, "Cache set (Sled)");
        Ok(())
    }

    #[instrument(skip(self), level = "debug")]
    async fn invalidate(&self, key: &str) -> Result<(), ApplicationError> {
        let db = self.db.clone();
        let key_str = key.to_string();

        tokio::task::spawn_blocking({
            let key_bytes = key_str.as_bytes().to_vec();
            move || db.remove(&key_bytes)
        })
        .await
        .map_err(|e| ApplicationError::Internal(format!("Task join error: {e}")))?
        .map_err(|e| ApplicationError::Internal(format!("Sled remove error: {e}")))?;

        debug!(key = %key_str, "Cache invalidated (Sled)");
        Ok(())
    }

    #[instrument(skip(self), level = "debug")]
    async fn invalidate_pattern(&self, pattern: &str) -> Result<u64, ApplicationError> {
        let prefix = pattern.trim_end_matches('*');
        let prefix_bytes = prefix.as_bytes();
        let mut count = 0u64;

        // Sled supports prefix scanning efficiently
        for result in self.db.scan_prefix(prefix_bytes) {
            let (key, _) =
                result.map_err(|e| ApplicationError::Internal(format!("Sled scan error: {e}")))?;

            self.db
                .remove(&key)
                .map_err(|e| ApplicationError::Internal(format!("Sled remove error: {e}")))?;
            count += 1;
        }

        debug!(pattern = %pattern, count = count, "Pattern invalidation complete (Sled)");
        Ok(count)
    }

    #[instrument(skip(self), level = "debug")]
    async fn exists(&self, key: &str) -> Result<bool, ApplicationError> {
        let db = self.db.clone();
        let key_str = key.to_string();

        let result = tokio::task::spawn_blocking({
            let key_bytes = key_str.as_bytes().to_vec();
            move || db.contains_key(&key_bytes)
        })
        .await
        .map_err(|e| ApplicationError::Internal(format!("Task join error: {e}")))?
        .map_err(|e| ApplicationError::Internal(format!("Sled contains_key error: {e}")))?;

        Ok(result)
    }

    fn stats(&self) -> CacheStats {
        let entries = self.db.len() as u64;
        let size_on_disk = self.db.size_on_disk().unwrap_or(0);

        CacheStats {
            hits: self.hits.load(Ordering::Relaxed),
            misses: self.misses.load(Ordering::Relaxed),
            entries,
            memory_bytes: size_on_disk,
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
        let cache = SledCache::in_memory().unwrap();
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
        let cache = SledCache::in_memory().unwrap();
        let result: Option<TestData> = cache.get("nonexistent").await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn invalidate_removes_entry() {
        let cache = SledCache::in_memory().unwrap();
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
        let cache = SledCache::in_memory().unwrap();
        cache
            .set("key", &"value".to_string(), Duration::from_secs(60))
            .await
            .unwrap();

        assert!(cache.exists("key").await.unwrap());
    }

    #[tokio::test]
    async fn exists_returns_false_for_missing_key() {
        let cache = SledCache::in_memory().unwrap();
        assert!(!cache.exists("missing").await.unwrap());
    }

    #[tokio::test]
    async fn invalidate_pattern_removes_matching_keys() {
        let cache = SledCache::in_memory().unwrap();
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
        let cache = SledCache::in_memory().unwrap();
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

    #[tokio::test]
    async fn expired_entries_return_none() {
        let cache = SledCache::in_memory().unwrap();

        // Set with very short TTL
        cache
            .set("key", &"value".to_string(), Duration::from_millis(1))
            .await
            .unwrap();

        // Wait for expiration
        tokio::time::sleep(Duration::from_millis(10)).await;

        let result: Option<String> = cache.get("key").await.unwrap();
        assert!(result.is_none());
    }
}
