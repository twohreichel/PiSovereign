//! Cache implementations
//!
//! Provides caching adapters for the application layer:
//! - `MokaCache`: High-performance in-memory cache with TTL support
//! - `SledCache`: Embedded persistent cache for durability
//! - `MultiLayerCache`: Combines L1 (Moka) and L2 (Sled) with write-through

mod moka_cache;
mod multi_layer_cache;
mod sled_cache;

pub use moka_cache::MokaCache;
pub use multi_layer_cache::MultiLayerCache;
pub use sled_cache::SledCache;

/// Generate a cache key from components using blake3 hash
///
/// This ensures consistent key generation across the application
/// and handles variable-length inputs efficiently.
#[must_use]
pub fn generate_cache_key(prefix: &str, components: &[&str]) -> String {
    let mut hasher = blake3::Hasher::new();
    for component in components {
        hasher.update(component.as_bytes());
        hasher.update(b"|"); // Separator to avoid collisions
    }
    let hash = hasher.finalize();
    format!("{}:{}", prefix, hash.to_hex())
}

/// Generate a cache key for LLM requests
///
/// Includes prompt, model, and temperature to ensure cache hits
/// only occur for semantically equivalent requests.
#[must_use]
pub fn llm_cache_key(prompt: &str, model: &str, temperature: f32) -> String {
    // Quantize temperature to avoid floating point comparison issues
    let temp_str = format!("{temperature:.2}");
    generate_cache_key("llm", &[prompt, model, &temp_str])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generate_cache_key_is_deterministic() {
        let key1 = generate_cache_key("test", &["a", "b", "c"]);
        let key2 = generate_cache_key("test", &["a", "b", "c"]);
        assert_eq!(key1, key2);
    }

    #[test]
    fn generate_cache_key_differs_for_different_inputs() {
        let key1 = generate_cache_key("test", &["a", "b"]);
        let key2 = generate_cache_key("test", &["a", "c"]);
        assert_ne!(key1, key2);
    }

    #[test]
    fn generate_cache_key_differs_for_different_prefixes() {
        let key1 = generate_cache_key("prefix1", &["a"]);
        let key2 = generate_cache_key("prefix2", &["a"]);
        assert_ne!(key1, key2);
    }

    #[test]
    fn generate_cache_key_starts_with_prefix() {
        let key = generate_cache_key("myprefix", &["data"]);
        assert!(key.starts_with("myprefix:"));
    }

    #[test]
    fn llm_cache_key_is_deterministic() {
        let key1 = llm_cache_key("Hello", "gpt-4", 0.7);
        let key2 = llm_cache_key("Hello", "gpt-4", 0.7);
        assert_eq!(key1, key2);
    }

    #[test]
    fn llm_cache_key_differs_for_temperature() {
        let key1 = llm_cache_key("Hello", "gpt-4", 0.7);
        let key2 = llm_cache_key("Hello", "gpt-4", 0.8);
        assert_ne!(key1, key2);
    }

    #[test]
    fn llm_cache_key_differs_for_model() {
        let key1 = llm_cache_key("Hello", "gpt-4", 0.7);
        let key2 = llm_cache_key("Hello", "gpt-3.5", 0.7);
        assert_ne!(key1, key2);
    }

    #[test]
    fn llm_cache_key_quantizes_temperature() {
        // These should be the same due to quantization to 2 decimal places
        let key1 = llm_cache_key("Hello", "gpt-4", 0.700);
        let key2 = llm_cache_key("Hello", "gpt-4", 0.7001);
        assert_eq!(key1, key2);
    }
}
