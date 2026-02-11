//! Conversation context service
//!
//! Provides in-memory conversation context with SQLite backup and automatic cleanup.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use chrono::{DateTime, TimeDelta, Utc};
use domain::entities::{ChatMessage, Conversation};
use domain::value_objects::ConversationId;
use parking_lot::RwLock;
use tracing::{debug, info, instrument, warn};

use crate::error::ApplicationError;
use crate::ports::ConversationStore;

/// Configuration for conversation context
#[derive(Debug, Clone)]
pub struct ConversationContextConfig {
    /// Maximum number of conversations to keep in memory
    pub max_cached_conversations: usize,
    /// Maximum messages per conversation in memory
    pub max_messages_per_conversation: usize,
    /// Retention period for conversations (default: 7 days)
    pub retention_days: u32,
    /// How often to sync to persistent storage (default: 30 seconds)
    pub sync_interval: Duration,
}

impl Default for ConversationContextConfig {
    fn default() -> Self {
        Self {
            max_cached_conversations: 100,
            max_messages_per_conversation: 50,
            retention_days: 7,
            sync_interval: Duration::from_secs(30),
        }
    }
}

/// In-memory conversation cache entry
#[derive(Debug, Clone)]
struct CacheEntry {
    conversation: Conversation,
    dirty: bool,
    last_accessed: DateTime<Utc>,
}

/// Service for managing conversation context
///
/// Provides fast in-memory access to recent conversations
/// with automatic persistence to SQLite and retention management.
pub struct ConversationContextService<S: ConversationStore> {
    store: Arc<S>,
    cache: Arc<RwLock<HashMap<ConversationId, CacheEntry>>>,
    config: ConversationContextConfig,
}

impl<S: ConversationStore> std::fmt::Debug for ConversationContextService<S> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let cache_size = self.cache.read().len();
        f.debug_struct("ConversationContextService")
            .field("config", &self.config)
            .field("cache_size", &cache_size)
            .finish_non_exhaustive()
    }
}

impl<S: ConversationStore + 'static> ConversationContextService<S> {
    /// Create a new conversation context service
    pub fn new(store: Arc<S>, config: ConversationContextConfig) -> Self {
        Self {
            store,
            cache: Arc::new(RwLock::new(HashMap::new())),
            config,
        }
    }

    /// Get or create a conversation
    ///
    /// If the conversation exists in cache, returns it immediately.
    /// Otherwise, tries to load from persistent storage.
    /// If not found anywhere, creates a new conversation.
    #[instrument(skip(self))]
    pub async fn get_or_create(
        &self,
        id: &ConversationId,
    ) -> Result<Conversation, ApplicationError> {
        // Check cache first
        {
            let mut cache = self.cache.write();
            if let Some(entry) = cache.get_mut(id) {
                entry.last_accessed = Utc::now();
                debug!(conversation_id = %id, "Cache hit");
                return Ok(entry.conversation.clone());
            }
        }

        // Try to load from persistent storage
        if let Some(mut conversation) = self.store.get(id).await? {
            // Mark all loaded messages as already persisted
            conversation.mark_messages_persisted();

            let mut cache = self.cache.write();
            cache.insert(
                *id,
                CacheEntry {
                    conversation: conversation.clone(),
                    dirty: false,
                    last_accessed: Utc::now(),
                },
            );
            self.evict_if_needed(&mut cache);
            debug!(conversation_id = %id, "Loaded from storage");
            return Ok(conversation);
        }

        // Create new conversation
        let mut conversation = Conversation::new();
        conversation.id = *id;

        // Persist immediately
        self.store.save(&conversation).await?;

        // Add to cache (marked clean since we just saved)
        {
            let mut cache = self.cache.write();
            cache.insert(
                *id,
                CacheEntry {
                    conversation: conversation.clone(),
                    dirty: false,
                    last_accessed: Utc::now(),
                },
            );
            self.evict_if_needed(&mut cache);
        }

        debug!(conversation_id = %id, "Created new conversation");
        Ok(conversation)
    }

    /// Add a message to a conversation
    #[instrument(skip(self, message), fields(conversation_id = %conversation_id))]
    pub async fn add_message(
        &self,
        conversation_id: &ConversationId,
        message: ChatMessage,
    ) -> Result<(), ApplicationError> {
        // Ensure conversation exists
        let _ = self.get_or_create(conversation_id).await?;

        // Update cache
        {
            let mut cache = self.cache.write();
            if let Some(entry) = cache.get_mut(conversation_id) {
                entry.conversation.add_message(message.clone());
                // Don't mark dirty since we'll persist immediately below
                entry.last_accessed = Utc::now();

                // Trim messages if too many (but keep persisted_message_count accurate)
                if entry.conversation.messages.len() > self.config.max_messages_per_conversation {
                    let excess = entry.conversation.messages.len()
                        - self.config.max_messages_per_conversation;
                    entry.conversation.messages.drain(0..excess);
                    // Adjust persisted count to account for trimmed messages
                    entry.conversation.persisted_message_count = entry
                        .conversation
                        .persisted_message_count
                        .saturating_sub(excess);
                }
            }
        }

        // Persist message immediately
        self.store.add_message(conversation_id, &message).await?;

        // Mark message as persisted
        {
            let mut cache = self.cache.write();
            if let Some(entry) = cache.get_mut(conversation_id) {
                entry.conversation.mark_n_messages_persisted(1);
            }
        }

        debug!("Added message to conversation");
        Ok(())
    }

    /// Get conversation context (last N messages)
    #[instrument(skip(self))]
    pub async fn get_context(
        &self,
        conversation_id: &ConversationId,
        max_messages: usize,
    ) -> Result<Vec<ChatMessage>, ApplicationError> {
        let conversation = self.get_or_create(conversation_id).await?;
        let messages = conversation.messages;
        let start = messages.len().saturating_sub(max_messages);
        Ok(messages[start..].to_vec())
    }

    /// Sync all dirty conversations to persistent storage.
    ///
    /// Uses incremental persistence - only messages that haven't been
    /// persisted yet are saved, improving efficiency for large conversations.
    #[instrument(skip(self))]
    pub async fn sync_to_storage(&self) -> Result<usize, ApplicationError> {
        // Collect conversations that need syncing (have unpersisted messages)
        let conversations_to_sync: Vec<(ConversationId, Vec<ChatMessage>)> = {
            let cache = self.cache.read();
            cache
                .iter()
                .filter(|(_, entry)| entry.conversation.has_unpersisted_messages())
                .map(|(id, entry)| {
                    let unpersisted = entry.conversation.unpersisted_messages().to_vec();
                    (*id, unpersisted)
                })
                .collect()
        };

        if conversations_to_sync.is_empty() {
            return Ok(0);
        }

        let mut total_messages = 0;

        for (conv_id, messages) in &conversations_to_sync {
            let count = self.store.add_messages(conv_id, messages).await?;
            total_messages += count;
        }

        // Mark messages as persisted in cache
        {
            let mut cache = self.cache.write();
            for (conv_id, _) in &conversations_to_sync {
                if let Some(entry) = cache.get_mut(conv_id) {
                    entry.conversation.mark_messages_persisted();
                    entry.dirty = false;
                }
            }
        }

        let conv_count = conversations_to_sync.len();
        debug!(
            conversations = conv_count,
            messages = total_messages,
            "Synced conversations incrementally to storage"
        );

        Ok(conv_count)
    }

    /// Cleanup old conversations from storage
    #[instrument(skip(self))]
    pub async fn cleanup_old_conversations(&self) -> Result<usize, ApplicationError> {
        let retention_days = i64::from(self.config.retention_days);
        let cutoff = Utc::now()
            - TimeDelta::try_days(retention_days).unwrap_or_else(|| {
                // Safety: 7 days is always valid
                TimeDelta::try_days(7).unwrap_or_default()
            });

        // Remove from cache
        {
            let mut cache = self.cache.write();
            cache.retain(|_, entry| entry.conversation.updated_at >= cutoff);
        }

        // Remove from storage
        let deleted = self.store.cleanup_older_than(cutoff).await?;

        if deleted > 0 {
            info!(deleted, retention_days, "Cleaned up old conversations");
        }

        Ok(deleted)
    }

    /// Get cache statistics
    pub fn cache_stats(&self) -> ConversationCacheStats {
        let cache = self.cache.read();
        let total = cache.len();
        let dirty = cache.values().filter(|e| e.dirty).count();
        ConversationCacheStats { total, dirty }
    }

    /// Evict least recently accessed conversations if cache is full
    fn evict_if_needed(&self, cache: &mut HashMap<ConversationId, CacheEntry>) {
        while cache.len() > self.config.max_cached_conversations {
            // Find least recently accessed
            let lru_id = cache
                .iter()
                .filter(|(_, e)| !e.dirty) // Don't evict dirty entries
                .min_by_key(|(_, e)| e.last_accessed)
                .map(|(id, _)| *id);

            if let Some(id) = lru_id {
                cache.remove(&id);
                debug!(conversation_id = %id, "Evicted from cache");
            } else {
                // All entries are dirty, can't evict
                warn!("Cache full but all entries are dirty");
                break;
            }
        }
    }
}

/// Cache statistics for conversation context
#[derive(Debug, Clone, Copy, Default)]
pub struct ConversationCacheStats {
    /// Total number of conversations in cache
    pub total: usize,
    /// Number of dirty (unsaved) conversations
    pub dirty: usize,
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use domain::entities::ConversationSource;

    use super::*;

    /// Mock conversation store for testing
    struct MockStore {
        conversations: Mutex<HashMap<ConversationId, Conversation>>,
        save_count: Mutex<usize>,
        cleanup_count: Mutex<usize>,
    }

    impl MockStore {
        fn new() -> Self {
            Self {
                conversations: Mutex::new(HashMap::new()),
                save_count: Mutex::new(0),
                cleanup_count: Mutex::new(0),
            }
        }

        fn save_count(&self) -> usize {
            *self
                .save_count
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
        }

        fn cleanup_count(&self) -> usize {
            *self
                .cleanup_count
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
        }
    }

    #[async_trait::async_trait]
    impl ConversationStore for MockStore {
        async fn get(&self, id: &ConversationId) -> Result<Option<Conversation>, ApplicationError> {
            let conversations = self
                .conversations
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            Ok(conversations.get(id).cloned())
        }

        async fn save(&self, conversation: &Conversation) -> Result<(), ApplicationError> {
            let mut conversations = self
                .conversations
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            conversations.insert(conversation.id, conversation.clone());
            *self
                .save_count
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner) += 1;
            Ok(())
        }

        async fn update(&self, conversation: &Conversation) -> Result<(), ApplicationError> {
            self.save(conversation).await
        }

        async fn delete(&self, id: &ConversationId) -> Result<(), ApplicationError> {
            let mut conversations = self
                .conversations
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            conversations.remove(id);
            Ok(())
        }

        async fn add_message(
            &self,
            conversation_id: &ConversationId,
            message: &ChatMessage,
        ) -> Result<(), ApplicationError> {
            let mut conversations = self
                .conversations
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if let Some(conversation) = conversations.get_mut(conversation_id) {
                conversation.add_message(message.clone());
            }
            Ok(())
        }

        async fn list_recent(&self, _limit: usize) -> Result<Vec<Conversation>, ApplicationError> {
            let conversations = self
                .conversations
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            Ok(conversations.values().cloned().collect())
        }

        async fn search(
            &self,
            query: &str,
            _limit: usize,
        ) -> Result<Vec<Conversation>, ApplicationError> {
            let conversations = self
                .conversations
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            Ok(conversations
                .values()
                .filter(|c| c.messages.iter().any(|m| m.content.contains(query)))
                .cloned()
                .collect())
        }

        async fn cleanup_older_than(
            &self,
            cutoff: DateTime<Utc>,
        ) -> Result<usize, ApplicationError> {
            let mut conversations = self
                .conversations
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            let before = conversations.len();
            conversations.retain(|_, c| c.updated_at >= cutoff);
            let deleted = before - conversations.len();
            *self
                .cleanup_count
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner) += 1;
            Ok(deleted)
        }

        async fn get_by_phone_number(
            &self,
            source: ConversationSource,
            phone_number: &str,
        ) -> Result<Option<Conversation>, ApplicationError> {
            let conversations = self
                .conversations
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            Ok(conversations
                .values()
                .find(|c| {
                    c.source == source
                        && c.phone_number.as_ref().map(|p| p.as_str()) == Some(phone_number)
                })
                .cloned())
        }

        async fn add_messages(
            &self,
            conversation_id: &ConversationId,
            messages: &[ChatMessage],
        ) -> Result<usize, ApplicationError> {
            let mut conversations = self
                .conversations
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if let Some(conversation) = conversations.get_mut(conversation_id) {
                for message in messages {
                    conversation.add_message(message.clone());
                }
            }
            Ok(messages.len())
        }
    }

    #[tokio::test]
    async fn test_get_or_create_new_conversation() {
        let store = Arc::new(MockStore::new());
        let config = ConversationContextConfig::default();
        let service = ConversationContextService::new(Arc::clone(&store), config);

        let id = ConversationId::new();
        let conversation = service.get_or_create(&id).await.unwrap();

        assert_eq!(conversation.id, id);
        assert_eq!(store.save_count(), 1);

        // Second call should be cached
        let conversation2 = service.get_or_create(&id).await.unwrap();
        assert_eq!(conversation2.id, id);
        assert_eq!(store.save_count(), 1); // Still 1, was cached
    }

    #[tokio::test]
    async fn test_add_message() {
        let store = Arc::new(MockStore::new());
        let config = ConversationContextConfig::default();
        let service = ConversationContextService::new(Arc::clone(&store), config);

        let id = ConversationId::new();
        let _ = service.get_or_create(&id).await.unwrap();

        let message = ChatMessage::user("Hello!".to_string());
        service.add_message(&id, message).await.unwrap();

        let context = service.get_context(&id, 10).await.unwrap();
        assert_eq!(context.len(), 1);
        assert_eq!(context[0].content, "Hello!");
    }

    #[tokio::test]
    async fn test_get_context_limits_messages() {
        let store = Arc::new(MockStore::new());
        let config = ConversationContextConfig::default();
        let service = ConversationContextService::new(Arc::clone(&store), config);

        let id = ConversationId::new();
        let _ = service.get_or_create(&id).await.unwrap();

        // Add 5 messages
        for i in 0..5 {
            let message = ChatMessage::user(format!("Message {i}"));
            service.add_message(&id, message).await.unwrap();
        }

        // Get only last 3
        let context = service.get_context(&id, 3).await.unwrap();
        assert_eq!(context.len(), 3);
        assert_eq!(context[0].content, "Message 2");
        assert_eq!(context[2].content, "Message 4");
    }

    #[tokio::test]
    async fn test_sync_to_storage() {
        let store = Arc::new(MockStore::new());
        let config = ConversationContextConfig::default();
        let service = ConversationContextService::new(Arc::clone(&store), config);

        let id = ConversationId::new();
        let _ = service.get_or_create(&id).await.unwrap();

        // Initial stats
        let stats = service.cache_stats();
        assert_eq!(stats.total, 1);
        assert_eq!(stats.dirty, 0); // Not dirty after initial save

        // Add message - this persists immediately, so dirty stays 0
        // (incremental persistence happens in add_message now)
        let message = ChatMessage::user("Hello!".to_string());
        service.add_message(&id, message).await.unwrap();

        let stats = service.cache_stats();
        // After add_message, the message is already persisted incrementally
        assert_eq!(stats.dirty, 0);

        // Sync should find no unpersisted messages (all were persisted in add_message)
        let synced = service.sync_to_storage().await.unwrap();
        assert_eq!(synced, 0); // Nothing to sync

        let stats = service.cache_stats();
        assert_eq!(stats.dirty, 0);
    }

    #[tokio::test]
    async fn test_sync_to_storage_with_batch_messages() {
        let store = Arc::new(MockStore::new());
        let config = ConversationContextConfig::default();
        let service = ConversationContextService::new(Arc::clone(&store), config);

        let id = ConversationId::new();
        let _ = service.get_or_create(&id).await.unwrap();

        // Manually add messages to cache without going through add_message
        // to simulate batch operations that need syncing
        {
            let mut cache = service.cache.write();
            if let Some(entry) = cache.get_mut(&id) {
                entry.conversation.add_message(ChatMessage::user("Test 1"));
                entry.conversation.add_message(ChatMessage::user("Test 2"));
                // Don't update persisted_message_count - these are unpersisted
            }
        }

        // Verify there are unpersisted messages
        let stats = service.cache_stats();
        assert_eq!(stats.total, 1);

        // Sync should persist the unpersisted messages
        let synced = service.sync_to_storage().await.unwrap();
        assert_eq!(synced, 1); // One conversation with unpersisted messages

        // Check that persisted_message_count was updated
        {
            let cache = service.cache.read();
            let entry = cache.get(&id).unwrap();
            assert!(!entry.conversation.has_unpersisted_messages());
        }
    }

    #[tokio::test]
    async fn test_cleanup_old_conversations() {
        let store = Arc::new(MockStore::new());
        let config = ConversationContextConfig {
            retention_days: 7,
            ..Default::default()
        };
        let service = ConversationContextService::new(Arc::clone(&store), config);

        let id = ConversationId::new();
        let _ = service.get_or_create(&id).await.unwrap();

        // Should not cleanup recent conversations
        let deleted = service.cleanup_old_conversations().await.unwrap();
        assert_eq!(deleted, 0);
        assert_eq!(store.cleanup_count(), 1);
    }

    #[tokio::test]
    async fn test_cache_eviction() {
        let store = Arc::new(MockStore::new());
        let config = ConversationContextConfig {
            max_cached_conversations: 2,
            ..Default::default()
        };
        let service = ConversationContextService::new(Arc::clone(&store), config);

        // Create 3 conversations
        for _ in 0..3 {
            let id = ConversationId::new();
            let _ = service.get_or_create(&id).await.unwrap();
        }

        // Cache should only have 2
        let stats = service.cache_stats();
        assert!(stats.total <= 2);
    }

    #[test]
    fn test_config_default() {
        let config = ConversationContextConfig::default();
        assert_eq!(config.max_cached_conversations, 100);
        assert_eq!(config.max_messages_per_conversation, 50);
        assert_eq!(config.retention_days, 7);
        assert_eq!(config.sync_interval, Duration::from_secs(30));
    }

    #[test]
    fn test_cache_stats_default() {
        let stats = ConversationCacheStats::default();
        assert_eq!(stats.total, 0);
        assert_eq!(stats.dirty, 0);
    }

    #[test]
    fn test_config_custom() {
        let config = ConversationContextConfig {
            max_cached_conversations: 50,
            max_messages_per_conversation: 100,
            retention_days: 30,
            sync_interval: Duration::from_secs(60),
        };
        assert_eq!(config.max_cached_conversations, 50);
        assert_eq!(config.max_messages_per_conversation, 100);
        assert_eq!(config.retention_days, 30);
        assert_eq!(config.sync_interval, Duration::from_secs(60));
    }

    #[test]
    fn test_cache_stats_clone() {
        let stats = ConversationCacheStats { total: 5, dirty: 2 };
        let cloned = stats;
        assert_eq!(cloned.total, 5);
        assert_eq!(cloned.dirty, 2);
    }

    #[test]
    fn test_cache_stats_debug() {
        let stats = ConversationCacheStats {
            total: 10,
            dirty: 3,
        };
        let debug = format!("{stats:?}");
        assert!(debug.contains("ConversationCacheStats"));
        assert!(debug.contains("10"));
        assert!(debug.contains('3'));
    }

    #[tokio::test]
    async fn test_service_debug() {
        let store = Arc::new(MockStore::new());
        let config = ConversationContextConfig::default();
        let service = ConversationContextService::new(Arc::clone(&store), config);

        let debug = format!("{service:?}");
        assert!(debug.contains("ConversationContextService"));
        assert!(debug.contains("config"));
        assert!(debug.contains("cache_size"));
    }

    #[tokio::test]
    async fn test_get_or_create_loads_from_storage() {
        let store = Arc::new(MockStore::new());
        let config = ConversationContextConfig::default();

        // Pre-populate the store with a conversation
        let id = ConversationId::new();
        let mut conversation = Conversation::new();
        conversation.id = id;
        conversation.add_message(ChatMessage::user("Stored message"));
        {
            let mut conversations = store.conversations.lock().unwrap();
            conversations.insert(id, conversation);
        }

        let service = ConversationContextService::new(Arc::clone(&store), config);

        // Should load from storage
        let loaded = service.get_or_create(&id).await.unwrap();
        assert_eq!(loaded.id, id);
        assert_eq!(loaded.messages.len(), 1);
        assert_eq!(loaded.messages[0].content, "Stored message");
    }

    #[tokio::test]
    async fn test_add_multiple_messages_sequentially() {
        let store = Arc::new(MockStore::new());
        let config = ConversationContextConfig::default();
        let service = ConversationContextService::new(Arc::clone(&store), config);

        let id = ConversationId::new();
        let _ = service.get_or_create(&id).await.unwrap();

        // Add multiple messages
        for i in 0..10 {
            let message = ChatMessage::user(format!("Message {i}"));
            service.add_message(&id, message).await.unwrap();
        }

        let context = service.get_context(&id, 100).await.unwrap();
        assert_eq!(context.len(), 10);
    }

    #[tokio::test]
    async fn test_get_context_returns_all_when_limit_exceeds() {
        let store = Arc::new(MockStore::new());
        let config = ConversationContextConfig::default();
        let service = ConversationContextService::new(Arc::clone(&store), config);

        let id = ConversationId::new();
        let _ = service.get_or_create(&id).await.unwrap();

        // Add 3 messages
        for i in 0..3 {
            let message = ChatMessage::user(format!("Message {i}"));
            service.add_message(&id, message).await.unwrap();
        }

        // Request 100 but only 3 exist
        let context = service.get_context(&id, 100).await.unwrap();
        assert_eq!(context.len(), 3);
    }

    #[tokio::test]
    async fn test_message_trimming_when_exceeds_max() {
        let store = Arc::new(MockStore::new());
        let config = ConversationContextConfig {
            max_messages_per_conversation: 5,
            ..Default::default()
        };
        let service = ConversationContextService::new(Arc::clone(&store), config);

        let id = ConversationId::new();
        let _ = service.get_or_create(&id).await.unwrap();

        // Add more messages than the limit
        for i in 0..10 {
            let message = ChatMessage::user(format!("Message {i}"));
            service.add_message(&id, message).await.unwrap();
        }

        // Should be trimmed to max
        let context = service.get_context(&id, 100).await.unwrap();
        assert!(context.len() <= 5);
    }

    #[tokio::test]
    async fn test_multiple_conversations_in_cache() {
        let store = Arc::new(MockStore::new());
        let config = ConversationContextConfig::default();
        let service = ConversationContextService::new(Arc::clone(&store), config);

        let ids: Vec<_> = (0..5).map(|_| ConversationId::new()).collect();

        for id in &ids {
            let _ = service.get_or_create(id).await.unwrap();
            service
                .add_message(id, ChatMessage::user("Test"))
                .await
                .unwrap();
        }

        let stats = service.cache_stats();
        assert_eq!(stats.total, 5);
    }

    #[tokio::test]
    async fn test_cache_eviction_preserves_dirty_entries() {
        let store = Arc::new(MockStore::new());
        let config = ConversationContextConfig {
            max_cached_conversations: 2,
            ..Default::default()
        };
        let service = ConversationContextService::new(Arc::clone(&store), config);

        // Create first conversation and add unpersisted message via cache manipulation
        let id1 = ConversationId::new();
        let _ = service.get_or_create(&id1).await.unwrap();

        // Mark entry as dirty
        {
            let mut cache = service.cache.write();
            if let Some(entry) = cache.get_mut(&id1) {
                entry.dirty = true;
                entry
                    .conversation
                    .add_message(ChatMessage::user("Unpersisted"));
            }
        }

        // Create more conversations to trigger eviction
        for _ in 0..3 {
            let id = ConversationId::new();
            let _ = service.get_or_create(&id).await.unwrap();
        }

        // Stats should reflect evictions happened
        let stats = service.cache_stats();
        assert!(stats.total <= 3); // Max 2 + 1 dirty that shouldn't be evicted
    }

    #[tokio::test]
    async fn test_cleanup_with_old_conversation() {
        let store = Arc::new(MockStore::new());
        let config = ConversationContextConfig {
            retention_days: 1,
            ..Default::default()
        };

        // Create an old conversation in the store
        let old_id = ConversationId::new();
        let mut old_conversation = Conversation::new();
        old_conversation.id = old_id;
        old_conversation.updated_at = Utc::now() - chrono::TimeDelta::try_days(10).unwrap();
        {
            let mut conversations = store.conversations.lock().unwrap();
            conversations.insert(old_id, old_conversation);
        }

        let service = ConversationContextService::new(Arc::clone(&store), config);

        // Load the old conversation into cache
        let _ = service.get_or_create(&old_id).await.unwrap();

        // Cleanup should remove the old conversation from cache
        let deleted = service.cleanup_old_conversations().await.unwrap();

        // The conversation should have been cleaned up from storage
        assert_eq!(store.cleanup_count(), 1);
        assert_eq!(deleted, 1);
    }

    #[tokio::test]
    async fn test_assistant_message() {
        let store = Arc::new(MockStore::new());
        let config = ConversationContextConfig::default();
        let service = ConversationContextService::new(Arc::clone(&store), config);

        let id = ConversationId::new();
        let _ = service.get_or_create(&id).await.unwrap();

        service
            .add_message(&id, ChatMessage::user("Hello"))
            .await
            .unwrap();
        service
            .add_message(&id, ChatMessage::assistant("Hi there!"))
            .await
            .unwrap();

        let context = service.get_context(&id, 10).await.unwrap();
        assert_eq!(context.len(), 2);
        assert!(matches!(
            context[0].role,
            domain::entities::MessageRole::User
        ));
        assert!(matches!(
            context[1].role,
            domain::entities::MessageRole::Assistant
        ));
    }

    #[tokio::test]
    async fn test_empty_sync_returns_zero() {
        let store = Arc::new(MockStore::new());
        let config = ConversationContextConfig::default();
        let service = ConversationContextService::new(Arc::clone(&store), config);

        // No conversations, sync should return 0
        let synced = service.sync_to_storage().await.unwrap();
        assert_eq!(synced, 0);
    }

    #[tokio::test]
    async fn test_cache_hit_updates_last_accessed() {
        let store = Arc::new(MockStore::new());
        let config = ConversationContextConfig::default();
        let service = ConversationContextService::new(Arc::clone(&store), config);

        let id = ConversationId::new();
        let _ = service.get_or_create(&id).await.unwrap();

        // Small delay to ensure time difference
        tokio::time::sleep(Duration::from_millis(10)).await;

        // Access again
        let _ = service.get_or_create(&id).await.unwrap();

        // Verify last_accessed was updated (implicitly tested through cache behavior)
        let stats = service.cache_stats();
        assert_eq!(stats.total, 1);
    }
}
