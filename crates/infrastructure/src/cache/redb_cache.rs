//! Redb embedded cache implementation
//!
//! Persistent key-value store for L2 caching.
//! Uses Redb for ACID-compliant storage without external dependencies.
//! Replaces the unmaintained Sled database.

use std::{
    fs,
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use application::{
    error::ApplicationError,
    ports::{CachePort, CacheStats},
};
use async_trait::async_trait;
use bincode::{Decode, Encode};
use redb::ReadableTableMetadata;
use redb::{Database, ReadableTable, TableDefinition};
use tracing::{debug, instrument, warn};

/// Table definition for cache entries
const CACHE_TABLE: TableDefinition<&[u8], &[u8]> = TableDefinition::new("cache");

/// Entry wrapper that includes expiration time
#[derive(Debug, Encode, Decode)]
struct CacheEntry {
    /// Serialized value as bytes
    data: Vec<u8>,
    /// Expiration timestamp (Unix epoch seconds)
    expires_at: u64,
}

/// Redb-based persistent cache
///
/// Stores cache entries on disk with TTL support.
/// Suitable for L2 caching where persistence across restarts is desired.
///
/// # Auto-Recovery
///
/// If the database file is corrupted or incompatible (e.g., after a bincode
/// version upgrade), the cache will automatically clear and recreate the
/// database file.
pub struct RedbCache {
    db: Arc<Database>,
    path: Option<PathBuf>,
    hits: AtomicU64,
    misses: AtomicU64,
}

impl std::fmt::Debug for RedbCache {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RedbCache")
            .field("db", &"<Database>")
            .field("path", &self.path)
            .field("hits", &self.hits.load(Ordering::Relaxed))
            .field("misses", &self.misses.load(Ordering::Relaxed))
            .finish()
    }
}

impl RedbCache {
    /// Create a new Redb cache at the specified path
    ///
    /// If the database file exists but is corrupted or incompatible,
    /// it will be deleted and recreated automatically.
    ///
    /// # Errors
    ///
    /// Returns an error if the database cannot be opened after retry.
    pub fn new<P: AsRef<Path>>(path: P) -> Result<Self, ApplicationError> {
        let path_buf = path.as_ref().to_path_buf();

        // Try to open existing database, recreate if corrupted
        let db = match Database::create(&path_buf) {
            Ok(db) => db,
            Err(e) => {
                warn!(
                    path = %path_buf.display(),
                    error = %e,
                    "Database corrupted or incompatible, recreating"
                );
                // Remove corrupted database file
                if path_buf.exists() {
                    fs::remove_file(&path_buf).map_err(|e| {
                        ApplicationError::Internal(format!(
                            "Failed to remove corrupted database: {e}"
                        ))
                    })?;
                }
                // Create fresh database
                Database::create(&path_buf).map_err(|e| {
                    ApplicationError::Internal(format!("Failed to create Redb database: {e}"))
                })?
            },
        };

        // Ensure table exists
        let write_txn = db.begin_write().map_err(|e| {
            ApplicationError::Internal(format!("Failed to begin write transaction: {e}"))
        })?;
        {
            // Opening the table creates it if it doesn't exist
            let _ = write_txn.open_table(CACHE_TABLE).map_err(|e| {
                ApplicationError::Internal(format!("Failed to open cache table: {e}"))
            })?;
        }
        write_txn.commit().map_err(|e| {
            ApplicationError::Internal(format!("Failed to commit transaction: {e}"))
        })?;

        Ok(Self {
            db: Arc::new(db),
            path: Some(path_buf),
            hits: AtomicU64::new(0),
            misses: AtomicU64::new(0),
        })
    }

    /// Create an in-memory Redb cache (for testing)
    #[cfg(test)]
    pub fn in_memory() -> Result<Self, ApplicationError> {
        let db = Database::builder()
            .create_with_backend(redb::backends::InMemoryBackend::new())
            .map_err(|e| {
                ApplicationError::Internal(format!("Failed to create in-memory Redb: {e}"))
            })?;

        // Ensure table exists
        let write_txn = db.begin_write().map_err(|e| {
            ApplicationError::Internal(format!("Failed to begin write transaction: {e}"))
        })?;
        {
            let _ = write_txn.open_table(CACHE_TABLE).map_err(|e| {
                ApplicationError::Internal(format!("Failed to open cache table: {e}"))
            })?;
        }
        write_txn.commit().map_err(|e| {
            ApplicationError::Internal(format!("Failed to commit transaction: {e}"))
        })?;

        Ok(Self {
            db: Arc::new(db),
            path: None,
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

        let write_txn = self.db.begin_write().map_err(|e| {
            ApplicationError::Internal(format!("Failed to begin write transaction: {e}"))
        })?;

        {
            let mut table = write_txn.open_table(CACHE_TABLE).map_err(|e| {
                ApplicationError::Internal(format!("Failed to open cache table: {e}"))
            })?;

            // Collect keys to remove (can't mutate while iterating)
            let keys_to_remove: Vec<Vec<u8>> = {
                let read_txn = self.db.begin_read().map_err(|e| {
                    ApplicationError::Internal(format!("Failed to begin read transaction: {e}"))
                })?;
                let read_table = read_txn.open_table(CACHE_TABLE).map_err(|e| {
                    ApplicationError::Internal(format!("Failed to open cache table for read: {e}"))
                })?;

                read_table
                    .iter()
                    .map_err(|e| ApplicationError::Internal(format!("Redb iteration error: {e}")))?
                    .filter_map(|result| {
                        result.ok().and_then(|(key, value)| {
                            let config = bincode::config::standard();
                            bincode::decode_from_slice::<CacheEntry, _>(value.value(), config)
                                .ok()
                                .filter(|(entry, _)| now >= entry.expires_at)
                                .map(|_| key.value().to_vec())
                        })
                    })
                    .collect()
            };

            for key in keys_to_remove {
                table
                    .remove(key.as_slice())
                    .map_err(|e| ApplicationError::Internal(format!("Redb remove error: {e}")))?;
                removed += 1;
            }
        }

        write_txn.commit().map_err(|e| {
            ApplicationError::Internal(format!("Failed to commit cleanup transaction: {e}"))
        })?;

        if removed > 0 {
            debug!(removed = removed, "Cleaned up expired cache entries");
        }

        Ok(removed)
    }

    /// Get the number of entries in the cache
    fn entry_count(&self) -> u64 {
        self.db
            .begin_read()
            .ok()
            .and_then(|txn| txn.open_table(CACHE_TABLE).ok())
            .and_then(|table| table.len().ok())
            .unwrap_or(0)
    }
}

#[async_trait]
impl CachePort for RedbCache {
    #[instrument(skip(self), level = "debug")]
    async fn get_bytes(&self, key: &str) -> Result<Option<Vec<u8>>, ApplicationError> {
        let db = self.db.clone();
        let key_bytes = key.as_bytes().to_vec();

        // Redb operations are blocking, wrap in spawn_blocking
        let result = tokio::task::spawn_blocking(move || {
            let read_txn = db.begin_read()?;
            let table = read_txn.open_table(CACHE_TABLE)?;
            Ok::<_, redb::Error>(table.get(key_bytes.as_slice())?.map(|v| v.value().to_vec()))
        })
        .await
        .map_err(|e| ApplicationError::Internal(format!("Task join error: {e}")))?
        .map_err(|e| ApplicationError::Internal(format!("Redb get error: {e}")))?;

        if let Some(bytes) = result {
            let config = bincode::config::standard();
            let (entry, _): (CacheEntry, _) =
                bincode::decode_from_slice(&bytes, config).map_err(|e| {
                    ApplicationError::Internal(format!("Cache entry deserialize error: {e}"))
                })?;

            // Check expiration
            if Self::is_expired(&entry) {
                self.misses.fetch_add(1, Ordering::Relaxed);
                // Lazy deletion - remove expired entry
                let db = self.db.clone();
                let key_bytes = key.as_bytes().to_vec();
                tokio::task::spawn_blocking(move || {
                    if let Ok(write_txn) = db.begin_write() {
                        let result = write_txn.open_table(CACHE_TABLE).is_ok_and(|mut table| {
                            let _ = table.remove(key_bytes.as_slice());
                            true
                        });
                        if result {
                            let _ = write_txn.commit();
                        }
                    }
                });
                debug!(key = %key, "Cache entry expired");
                return Ok(None);
            }

            self.hits.fetch_add(1, Ordering::Relaxed);
            debug!(key = %key, "Cache hit (Redb)");
            Ok(Some(entry.data))
        } else {
            self.misses.fetch_add(1, Ordering::Relaxed);
            debug!(key = %key, "Cache miss (Redb)");
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

        let config = bincode::config::standard();
        let entry_bytes = bincode::encode_to_vec(&entry, config)
            .map_err(|e| ApplicationError::Internal(format!("Entry serialize error: {e}")))?;

        let db = self.db.clone();
        let key_str = key.to_string();
        let ttl_secs = ttl.as_secs();

        tokio::task::spawn_blocking({
            let key_bytes = key_str.as_bytes().to_vec();
            move || {
                let write_txn = db.begin_write()?;
                {
                    let mut table = write_txn.open_table(CACHE_TABLE)?;
                    table.insert(key_bytes.as_slice(), entry_bytes.as_slice())?;
                }
                write_txn.commit()?;
                Ok::<_, redb::Error>(())
            }
        })
        .await
        .map_err(|e| ApplicationError::Internal(format!("Task join error: {e}")))?
        .map_err(|e| ApplicationError::Internal(format!("Redb insert error: {e}")))?;

        debug!(key = %key_str, ttl_secs = ttl_secs, "Cache set (Redb)");
        Ok(())
    }

    #[instrument(skip(self), level = "debug")]
    async fn invalidate(&self, key: &str) -> Result<(), ApplicationError> {
        let db = self.db.clone();
        let key_str = key.to_string();

        tokio::task::spawn_blocking({
            let key_bytes = key_str.as_bytes().to_vec();
            move || {
                let write_txn = db.begin_write()?;
                {
                    let mut table = write_txn.open_table(CACHE_TABLE)?;
                    table.remove(key_bytes.as_slice())?;
                }
                write_txn.commit()?;
                Ok::<_, redb::Error>(())
            }
        })
        .await
        .map_err(|e| ApplicationError::Internal(format!("Task join error: {e}")))?
        .map_err(|e| ApplicationError::Internal(format!("Redb remove error: {e}")))?;

        debug!(key = %key_str, "Cache invalidated (Redb)");
        Ok(())
    }

    #[instrument(skip(self), level = "debug")]
    async fn invalidate_pattern(&self, pattern: &str) -> Result<u64, ApplicationError> {
        let prefix = pattern.trim_end_matches('*');
        let prefix_bytes = prefix.as_bytes().to_vec();
        let db = self.db.clone();

        let count = tokio::task::spawn_blocking(move || {
            let mut removed = 0u64;

            // Collect keys matching prefix
            let keys_to_remove: Vec<Vec<u8>> = {
                let read_txn = db.begin_read()?;
                let table = read_txn.open_table(CACHE_TABLE)?;

                table
                    .iter()?
                    .filter_map(|result| {
                        result.ok().and_then(|(key, _)| {
                            let key_bytes = key.value();
                            if key_bytes.starts_with(&prefix_bytes) {
                                Some(key_bytes.to_vec())
                            } else {
                                None
                            }
                        })
                    })
                    .collect()
            };

            // Remove matching keys
            let write_txn = db.begin_write()?;
            {
                let mut table = write_txn.open_table(CACHE_TABLE)?;
                for key in keys_to_remove {
                    table.remove(key.as_slice())?;
                    removed += 1;
                }
            }
            write_txn.commit()?;

            Ok::<_, redb::Error>(removed)
        })
        .await
        .map_err(|e| ApplicationError::Internal(format!("Task join error: {e}")))?
        .map_err(|e| ApplicationError::Internal(format!("Redb pattern invalidate error: {e}")))?;

        debug!(pattern = %pattern, count = count, "Pattern invalidation complete (Redb)");
        Ok(count)
    }

    #[instrument(skip(self), level = "debug")]
    async fn exists(&self, key: &str) -> Result<bool, ApplicationError> {
        let db = self.db.clone();
        let key_bytes = key.as_bytes().to_vec();

        let result = tokio::task::spawn_blocking(move || {
            let read_txn = db.begin_read()?;
            let table = read_txn.open_table(CACHE_TABLE)?;
            Ok::<_, redb::Error>(table.get(key_bytes.as_slice())?.is_some())
        })
        .await
        .map_err(|e| ApplicationError::Internal(format!("Task join error: {e}")))?
        .map_err(|e| ApplicationError::Internal(format!("Redb exists error: {e}")))?;

        Ok(result)
    }

    fn stats(&self) -> CacheStats {
        let entries = self.entry_count();

        CacheStats {
            hits: self.hits.load(Ordering::Relaxed),
            misses: self.misses.load(Ordering::Relaxed),
            entries,
            // Redb doesn't expose size_on_disk easily, use entry count as proxy
            memory_bytes: entries * 256, // Rough estimate
        }
    }
}

#[cfg(test)]
mod tests {
    use application::ports::CachePortExt;
    use serde::{Deserialize, Serialize};
    use tempfile::TempDir;

    use super::*;

    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    struct TestData {
        value: String,
        count: i32,
    }

    #[tokio::test]
    async fn set_and_get_value() {
        let cache = RedbCache::in_memory().unwrap();
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
        let cache = RedbCache::in_memory().unwrap();
        let result: Option<TestData> = cache.get("nonexistent").await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn invalidate_removes_entry() {
        let cache = RedbCache::in_memory().unwrap();
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
        let cache = RedbCache::in_memory().unwrap();
        cache
            .set("key", &"value".to_string(), Duration::from_secs(60))
            .await
            .unwrap();

        assert!(cache.exists("key").await.unwrap());
    }

    #[tokio::test]
    async fn exists_returns_false_for_missing_key() {
        let cache = RedbCache::in_memory().unwrap();
        assert!(!cache.exists("missing").await.unwrap());
    }

    #[tokio::test]
    async fn invalidate_pattern_removes_matching_keys() {
        let cache = RedbCache::in_memory().unwrap();
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
        let cache = RedbCache::in_memory().unwrap();
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
        let cache = RedbCache::in_memory().unwrap();

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

    #[test]
    fn test_debug_impl() {
        let cache = RedbCache::in_memory().unwrap();
        let debug = format!("{cache:?}");
        assert!(debug.contains("RedbCache"));
        assert!(debug.contains("hits"));
        assert!(debug.contains("misses"));
        assert!(debug.contains("path"));
    }

    #[tokio::test]
    async fn test_stats_entries_count() {
        let cache = RedbCache::in_memory().unwrap();

        // Initially empty
        let stats = cache.stats();
        assert_eq!(stats.entries, 0);

        // Add some entries
        cache
            .set("key1", &"value1".to_string(), Duration::from_secs(60))
            .await
            .unwrap();
        cache
            .set("key2", &"value2".to_string(), Duration::from_secs(60))
            .await
            .unwrap();
        cache
            .set("key3", &"value3".to_string(), Duration::from_secs(60))
            .await
            .unwrap();

        let stats = cache.stats();
        assert_eq!(stats.entries, 3);
    }

    #[tokio::test]
    async fn test_stats_memory_estimation() {
        let cache = RedbCache::in_memory().unwrap();
        cache
            .set("key", &"value".to_string(), Duration::from_secs(60))
            .await
            .unwrap();

        let stats = cache.stats();
        // Memory is estimated as entries * 256
        assert_eq!(stats.memory_bytes, 256);
    }

    #[tokio::test]
    async fn test_cleanup_expired_removes_old_entries() {
        let cache = RedbCache::in_memory().unwrap();

        // Set entries with very short TTL
        cache
            .set("expired1", &"val1".to_string(), Duration::from_millis(1))
            .await
            .unwrap();
        cache
            .set("expired2", &"val2".to_string(), Duration::from_millis(1))
            .await
            .unwrap();
        cache
            .set("valid", &"val3".to_string(), Duration::from_secs(60))
            .await
            .unwrap();

        // Wait for expiration
        tokio::time::sleep(Duration::from_millis(50)).await;

        // Cleanup expired entries
        let removed = cache.cleanup_expired().unwrap();
        assert_eq!(removed, 2);

        // Valid entry should still exist
        assert!(cache.exists("valid").await.unwrap());
    }

    #[tokio::test]
    async fn test_cleanup_no_expired_entries() {
        let cache = RedbCache::in_memory().unwrap();

        // Set entries with long TTL
        cache
            .set("key1", &"value1".to_string(), Duration::from_secs(3600))
            .await
            .unwrap();
        cache
            .set("key2", &"value2".to_string(), Duration::from_secs(3600))
            .await
            .unwrap();

        // Cleanup should remove nothing
        let removed = cache.cleanup_expired().unwrap();
        assert_eq!(removed, 0);

        // All entries should still exist
        assert!(cache.exists("key1").await.unwrap());
        assert!(cache.exists("key2").await.unwrap());
    }

    #[tokio::test]
    async fn test_overwrite_existing_key() {
        let cache = RedbCache::in_memory().unwrap();

        cache
            .set("key", &"original".to_string(), Duration::from_secs(60))
            .await
            .unwrap();
        cache
            .set("key", &"updated".to_string(), Duration::from_secs(60))
            .await
            .unwrap();

        let result: Option<String> = cache.get("key").await.unwrap();
        assert_eq!(result, Some("updated".to_string()));
    }

    #[tokio::test]
    async fn test_get_bytes_and_set_bytes_directly() {
        let cache = RedbCache::in_memory().unwrap();
        let data = b"raw binary data";

        cache
            .set_bytes("binary_key", data.to_vec(), Duration::from_secs(60))
            .await
            .unwrap();

        let result = cache.get_bytes("binary_key").await.unwrap();
        assert_eq!(result, Some(data.to_vec()));
    }

    #[tokio::test]
    async fn test_invalidate_nonexistent_key() {
        let cache = RedbCache::in_memory().unwrap();

        // Invalidating a nonexistent key should not error
        cache.invalidate("nonexistent").await.unwrap();
    }

    #[tokio::test]
    async fn test_invalidate_pattern_no_matches() {
        let cache = RedbCache::in_memory().unwrap();
        cache
            .set("other_key", &"value".to_string(), Duration::from_secs(60))
            .await
            .unwrap();

        let count = cache.invalidate_pattern("nomatch:*").await.unwrap();
        assert_eq!(count, 0);

        // Original key should still exist
        assert!(cache.exists("other_key").await.unwrap());
    }

    #[tokio::test]
    async fn test_file_based_cache() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test_cache.redb");

        // Create and use file-based cache
        {
            let cache = RedbCache::new(&db_path).unwrap();
            cache
                .set("persistent_key", &42i32, Duration::from_secs(3600))
                .await
                .unwrap();
        }

        // Reopen and verify data persists
        {
            let cache = RedbCache::new(&db_path).unwrap();
            let result: Option<i32> = cache.get("persistent_key").await.unwrap();
            assert_eq!(result, Some(42));
        }
    }

    #[test]
    fn test_cache_entry_encode_decode() {
        let entry = CacheEntry {
            data: vec![1, 2, 3, 4, 5],
            expires_at: 1000,
        };

        let config = bincode::config::standard();
        let encoded = bincode::encode_to_vec(&entry, config).unwrap();
        let (decoded, _): (CacheEntry, _) = bincode::decode_from_slice(&encoded, config).unwrap();

        assert_eq!(decoded.data, entry.data);
        assert_eq!(decoded.expires_at, entry.expires_at);
    }

    #[test]
    fn test_is_expired() {
        let expired_entry = CacheEntry {
            data: vec![],
            expires_at: 0, // Already expired (Unix epoch)
        };
        assert!(RedbCache::is_expired(&expired_entry));

        let future_entry = CacheEntry {
            data: vec![],
            expires_at: u64::MAX, // Far in the future
        };
        assert!(!RedbCache::is_expired(&future_entry));
    }

    #[test]
    fn test_now_timestamp_reasonable() {
        let now = RedbCache::now_timestamp();
        // Should be after year 2020 (timestamp > 1577836800)
        assert!(now > 1_577_836_800);
    }

    #[tokio::test]
    async fn test_entry_count_matches_stats() {
        let cache = RedbCache::in_memory().unwrap();

        for i in 0..10 {
            cache
                .set(&format!("key_{i}"), &i, Duration::from_secs(60))
                .await
                .unwrap();
        }

        let count = cache.entry_count();
        let stats = cache.stats();

        assert_eq!(count, 10);
        assert_eq!(stats.entries, count);
    }

    #[tokio::test]
    async fn test_expired_entry_increments_miss() {
        let cache = RedbCache::in_memory().unwrap();

        // Set with very short TTL
        cache
            .set("key", &"value".to_string(), Duration::from_millis(1))
            .await
            .unwrap();

        // Wait for expiration
        tokio::time::sleep(Duration::from_millis(50)).await;

        // Get should return None and count as miss
        let result: Option<String> = cache.get("key").await.unwrap();
        assert!(result.is_none());

        let stats = cache.stats();
        assert_eq!(stats.misses, 1);
        assert_eq!(stats.hits, 0);
    }

    #[tokio::test]
    async fn test_multiple_patterns() {
        let cache = RedbCache::in_memory().unwrap();

        cache
            .set("user:1:name", &"Alice".to_string(), Duration::from_secs(60))
            .await
            .unwrap();
        cache
            .set(
                "user:1:email",
                &"alice@test.com".to_string(),
                Duration::from_secs(60),
            )
            .await
            .unwrap();
        cache
            .set("user:2:name", &"Bob".to_string(), Duration::from_secs(60))
            .await
            .unwrap();
        cache
            .set("session:abc", &"data".to_string(), Duration::from_secs(60))
            .await
            .unwrap();

        // Invalidate all user:1 entries
        let count = cache.invalidate_pattern("user:1:*").await.unwrap();
        assert_eq!(count, 2);

        // user:2 and session should remain
        assert!(cache.exists("user:2:name").await.unwrap());
        assert!(cache.exists("session:abc").await.unwrap());
    }

    #[tokio::test]
    #[allow(clippy::items_after_statements)]
    async fn test_complex_data_serialization() {
        let cache = RedbCache::in_memory().unwrap();

        #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
        struct ComplexData {
            nested: Vec<TestData>,
            map: std::collections::HashMap<String, i32>,
        }

        let mut map = std::collections::HashMap::new();
        map.insert("a".to_string(), 1);
        map.insert("b".to_string(), 2);

        let complex = ComplexData {
            nested: vec![
                TestData {
                    value: "x".to_string(),
                    count: 10,
                },
                TestData {
                    value: "y".to_string(),
                    count: 20,
                },
            ],
            map,
        };

        cache
            .set("complex", &complex, Duration::from_secs(60))
            .await
            .unwrap();

        let result: Option<ComplexData> = cache.get("complex").await.unwrap();
        assert_eq!(result, Some(complex));
    }
}
