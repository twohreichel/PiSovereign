//! Memory/Knowledge storage, Embedding, and Reminder configurations.

use serde::{Deserialize, Serialize};

use super::default_true;

// ==============================
// Memory/Knowledge Storage Configuration
// ==============================

/// Memory storage configuration for AI knowledge persistence
///
/// Enables storage of AI interactions, facts, and context for RAG-based
/// retrieval and self-improvement.
#[allow(clippy::struct_excessive_bools)] // Configuration needs multiple boolean flags
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryAppConfig {
    /// Enable memory storage (default: true)
    #[serde(default = "default_true")]
    pub enabled: bool,

    /// Enable RAG context retrieval (default: true)
    #[serde(default = "default_true")]
    pub enable_rag: bool,

    /// Enable automatic learning from interactions (default: true)
    #[serde(default = "default_true")]
    pub enable_learning: bool,

    /// Number of memories to retrieve for RAG context (default: 5)
    #[serde(default = "default_rag_limit")]
    pub rag_limit: usize,

    /// Minimum similarity threshold for RAG retrieval (0.0-1.0, default: 0.5)
    #[serde(default = "default_rag_threshold")]
    pub rag_threshold: f32,

    /// Similarity threshold for memory deduplication (0.0-1.0, default: 0.85)
    #[serde(default = "default_merge_threshold")]
    pub merge_threshold: f32,

    /// Minimum importance score to keep memories (default: 0.1)
    #[serde(default = "default_min_importance")]
    pub min_importance: f32,

    /// Decay factor for memory importance over time (default: 0.95)
    #[serde(default = "default_decay_factor")]
    pub decay_factor: f32,

    /// Enable content encryption (default: true)
    #[serde(default = "default_true")]
    pub enable_encryption: bool,

    /// Path to encryption key file (generated if not exists)
    #[serde(default = "default_encryption_key_path")]
    pub encryption_key_path: String,

    /// Embedding model configuration
    #[serde(default)]
    pub embedding: EmbeddingAppConfig,
}

impl Default for MemoryAppConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            enable_rag: true,
            enable_learning: true,
            rag_limit: default_rag_limit(),
            rag_threshold: default_rag_threshold(),
            merge_threshold: default_merge_threshold(),
            min_importance: default_min_importance(),
            decay_factor: default_decay_factor(),
            enable_encryption: true,
            encryption_key_path: default_encryption_key_path(),
            embedding: EmbeddingAppConfig::default(),
        }
    }
}

/// Embedding model configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbeddingAppConfig {
    /// Embedding model name (default: nomic-embed-text)
    #[serde(default = "default_embedding_model")]
    pub model: String,

    /// Embedding dimension (default: 384 for nomic-embed-text)
    #[serde(default = "default_embedding_dimension")]
    pub dimension: usize,

    /// Request timeout in milliseconds (default: 30000)
    #[serde(default = "default_embedding_timeout")]
    pub timeout_ms: u64,
}

impl Default for EmbeddingAppConfig {
    fn default() -> Self {
        Self {
            model: default_embedding_model(),
            dimension: default_embedding_dimension(),
            timeout_ms: default_embedding_timeout(),
        }
    }
}

impl MemoryAppConfig {
    /// Convert to `MemoryServiceConfig`
    #[must_use]
    pub const fn to_memory_service_config(&self) -> application::MemoryServiceConfig {
        application::MemoryServiceConfig {
            rag_limit: self.rag_limit,
            rag_threshold: self.rag_threshold,
            merge_threshold: self.merge_threshold,
            min_importance: self.min_importance,
            decay_factor: self.decay_factor,
            enable_encryption: self.enable_encryption,
        }
    }

    /// Convert to `MemoryEnhancedChatConfig`
    #[must_use]
    pub const fn to_enhanced_chat_config(
        &self,
        system_prompt: Option<String>,
    ) -> application::MemoryEnhancedChatConfig {
        application::MemoryEnhancedChatConfig {
            enable_rag: self.enable_rag,
            enable_learning: self.enable_learning,
            system_prompt,
            min_learning_length: 20,
            default_importance: 0.5,
        }
    }

    /// Convert embedding config to `ai_core::EmbeddingConfig`
    #[must_use]
    pub fn to_embedding_config(&self, base_url: &str) -> ai_core::EmbeddingConfig {
        ai_core::EmbeddingConfig {
            base_url: base_url.to_string(),
            model: self.embedding.model.clone(),
            dimensions: self.embedding.dimension,
            timeout_ms: self.embedding.timeout_ms,
        }
    }
}

// Default value functions for memory config
const fn default_rag_limit() -> usize {
    5
}

const fn default_rag_threshold() -> f32 {
    0.5
}

const fn default_merge_threshold() -> f32 {
    0.85
}

const fn default_min_importance() -> f32 {
    0.1
}

const fn default_decay_factor() -> f32 {
    0.95
}

fn default_encryption_key_path() -> String {
    "memory_encryption.key".to_string()
}

fn default_embedding_model() -> String {
    "nomic-embed-text".to_string()
}

const fn default_embedding_dimension() -> usize {
    384
}

const fn default_embedding_timeout() -> u64 {
    30000
}

// ==============================
// Reminder Configuration
// ==============================

/// Reminder system configuration
///
/// Configures the reminder system behavior including snooze limits,
/// notification timing, and CalDAV sync settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReminderAppConfig {
    /// Maximum number of snoozes allowed per reminder (default: 5)
    #[serde(default = "default_max_snooze")]
    pub max_snooze: u8,

    /// Default snooze duration in minutes (default: 15)
    #[serde(default = "default_snooze_minutes")]
    pub default_snooze_minutes: u32,

    /// How far in advance to create reminders from CalDAV events (minutes)
    #[serde(default = "default_caldav_reminder_lead_time")]
    pub caldav_reminder_lead_time_minutes: u32,

    /// Interval for checking due reminders (seconds, default: 60)
    #[serde(default = "default_reminder_check_interval")]
    pub check_interval_secs: u64,

    /// CalDAV sync interval (minutes, default: 15)
    #[serde(default = "default_caldav_sync_interval")]
    pub caldav_sync_interval_minutes: u32,

    /// Morning briefing time (HH:MM format, default: "07:00")
    #[serde(default = "default_morning_briefing_time")]
    pub morning_briefing_time: String,

    /// Enable morning briefing (default: true)
    #[serde(default = "default_true")]
    pub morning_briefing_enabled: bool,
}

const fn default_max_snooze() -> u8 {
    5
}

const fn default_snooze_minutes() -> u32 {
    15
}

const fn default_caldav_reminder_lead_time() -> u32 {
    30
}

const fn default_reminder_check_interval() -> u64 {
    60
}

const fn default_caldav_sync_interval() -> u32 {
    15
}

fn default_morning_briefing_time() -> String {
    "07:00".to_string()
}

impl Default for ReminderAppConfig {
    fn default() -> Self {
        Self {
            max_snooze: default_max_snooze(),
            default_snooze_minutes: default_snooze_minutes(),
            caldav_reminder_lead_time_minutes: default_caldav_reminder_lead_time(),
            check_interval_secs: default_reminder_check_interval(),
            caldav_sync_interval_minutes: default_caldav_sync_interval(),
            morning_briefing_time: default_morning_briefing_time(),
            morning_briefing_enabled: true,
        }
    }
}
