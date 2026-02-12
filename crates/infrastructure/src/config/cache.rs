//! Cache configuration with TTL settings.

use serde::{Deserialize, Serialize};

use super::default_true;

/// Cache configuration with TTL settings per cache type
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheConfig {
    /// Whether caching is enabled
    #[serde(default = "default_true")]
    pub enabled: bool,

    /// Short TTL in seconds (for frequently changing data, default: 5 minutes)
    #[serde(default = "default_cache_ttl_short")]
    pub ttl_short_secs: u64,

    /// Medium TTL in seconds (for moderately stable data, default: 1 hour)
    #[serde(default = "default_cache_ttl_medium")]
    pub ttl_medium_secs: u64,

    /// Long TTL in seconds (for stable data, default: 24 hours)
    #[serde(default = "default_cache_ttl_long")]
    pub ttl_long_secs: u64,

    /// TTL for dynamic LLM responses in seconds (high temperature, default: 1 hour)
    #[serde(default = "default_cache_ttl_medium")]
    pub ttl_llm_dynamic_secs: u64,

    /// TTL for stable LLM responses in seconds (low temperature, default: 24 hours)
    #[serde(default = "default_cache_ttl_long")]
    pub ttl_llm_stable_secs: u64,

    /// Maximum number of entries in L1 (in-memory) cache
    #[serde(default = "default_l1_max_entries")]
    pub l1_max_entries: u64,
}

const fn default_cache_ttl_short() -> u64 {
    5 * 60 // 5 minutes
}

const fn default_cache_ttl_medium() -> u64 {
    60 * 60 // 1 hour
}

const fn default_cache_ttl_long() -> u64 {
    24 * 60 * 60 // 24 hours
}

const fn default_l1_max_entries() -> u64 {
    10_000
}

impl Default for CacheConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            ttl_short_secs: default_cache_ttl_short(),
            ttl_medium_secs: default_cache_ttl_medium(),
            ttl_long_secs: default_cache_ttl_long(),
            ttl_llm_dynamic_secs: default_cache_ttl_medium(),
            ttl_llm_stable_secs: default_cache_ttl_long(),
            l1_max_entries: default_l1_max_entries(),
        }
    }
}

impl CacheConfig {
    /// Get the short TTL as a Duration
    #[must_use]
    pub const fn ttl_short(&self) -> std::time::Duration {
        std::time::Duration::from_secs(self.ttl_short_secs)
    }

    /// Get the medium TTL as a Duration
    #[must_use]
    pub const fn ttl_medium(&self) -> std::time::Duration {
        std::time::Duration::from_secs(self.ttl_medium_secs)
    }

    /// Get the long TTL as a Duration
    #[must_use]
    pub const fn ttl_long(&self) -> std::time::Duration {
        std::time::Duration::from_secs(self.ttl_long_secs)
    }

    /// Get the LLM dynamic TTL as a Duration
    #[must_use]
    pub const fn ttl_llm_dynamic(&self) -> std::time::Duration {
        std::time::Duration::from_secs(self.ttl_llm_dynamic_secs)
    }

    /// Get the LLM stable TTL as a Duration
    #[must_use]
    pub const fn ttl_llm_stable(&self) -> std::time::Duration {
        std::time::Duration::from_secs(self.ttl_llm_stable_secs)
    }
}
