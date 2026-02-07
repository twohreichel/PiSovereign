//! Web search configuration

use serde::{Deserialize, Serialize};

/// Configuration for web search services
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebSearchConfig {
    /// Brave Search API key (optional, enables Brave as primary provider)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub brave_api_key: Option<String>,

    /// Brave Search API base URL
    #[serde(default = "default_brave_base_url")]
    pub brave_base_url: String,

    /// DuckDuckGo API base URL (for HTML scraping fallback)
    #[serde(default = "default_duckduckgo_base_url")]
    pub duckduckgo_base_url: String,

    /// Connection timeout in seconds
    #[serde(default = "default_timeout_secs")]
    pub timeout_secs: u64,

    /// Maximum number of results to return
    #[serde(default = "default_max_results")]
    pub max_results: usize,

    /// Cache TTL in minutes (0 to disable caching)
    #[serde(default = "default_cache_ttl_minutes")]
    pub cache_ttl_minutes: u32,

    /// Daily rate limit for searches (0 for unlimited)
    #[serde(default = "default_rate_limit_daily")]
    pub rate_limit_daily: u32,

    /// Enable DuckDuckGo fallback when Brave fails
    #[serde(default = "default_fallback_enabled")]
    pub fallback_enabled: bool,

    /// Safe search level: "off", "moderate", "strict"
    #[serde(default = "default_safe_search")]
    pub safe_search: String,

    /// Preferred result language (ISO 639-1 code, e.g., "en", "de")
    #[serde(default = "default_result_language")]
    pub result_language: String,

    /// Preferred result country (ISO 3166-1 alpha-2 code, e.g., "US", "DE")
    #[serde(default = "default_result_country")]
    pub result_country: String,
}

fn default_brave_base_url() -> String {
    "https://api.search.brave.com/res/v1".to_string()
}

fn default_duckduckgo_base_url() -> String {
    "https://api.duckduckgo.com".to_string()
}

const fn default_timeout_secs() -> u64 {
    30
}

const fn default_max_results() -> usize {
    5
}

const fn default_cache_ttl_minutes() -> u32 {
    15
}

const fn default_rate_limit_daily() -> u32 {
    100
}

const fn default_fallback_enabled() -> bool {
    true
}

fn default_safe_search() -> String {
    "moderate".to_string()
}

fn default_result_language() -> String {
    "de".to_string()
}

fn default_result_country() -> String {
    "DE".to_string()
}

impl Default for WebSearchConfig {
    fn default() -> Self {
        Self {
            brave_api_key: None,
            brave_base_url: default_brave_base_url(),
            duckduckgo_base_url: default_duckduckgo_base_url(),
            timeout_secs: default_timeout_secs(),
            max_results: default_max_results(),
            cache_ttl_minutes: default_cache_ttl_minutes(),
            rate_limit_daily: default_rate_limit_daily(),
            fallback_enabled: default_fallback_enabled(),
            safe_search: default_safe_search(),
            result_language: default_result_language(),
            result_country: default_result_country(),
        }
    }
}

impl WebSearchConfig {
    /// Create a configuration for testing (no API key, short timeout)
    #[must_use]
    pub fn for_testing() -> Self {
        Self {
            brave_api_key: None,
            timeout_secs: 5,
            max_results: 3,
            cache_ttl_minutes: 0,
            rate_limit_daily: 0,
            fallback_enabled: true,
            ..Default::default()
        }
    }

    /// Check if caching is enabled
    #[must_use]
    pub const fn caching_enabled(&self) -> bool {
        self.cache_ttl_minutes > 0
    }

    /// Check if rate limiting is enabled
    #[must_use]
    pub const fn rate_limiting_enabled(&self) -> bool {
        self.rate_limit_daily > 0
    }

    /// Validate the configuration
    ///
    /// # Errors
    ///
    /// Returns an error if the configuration is invalid.
    pub fn validate(&self) -> Result<(), String> {
        if self.max_results == 0 {
            return Err("max_results must be greater than 0".to_string());
        }

        if self.max_results > 20 {
            return Err("max_results must be 20 or less".to_string());
        }

        if self.timeout_secs == 0 {
            return Err("timeout_secs must be greater than 0".to_string());
        }

        let valid_safe_search = ["off", "moderate", "strict"];
        if !valid_safe_search.contains(&self.safe_search.as_str()) {
            return Err(format!(
                "safe_search must be one of: {}",
                valid_safe_search.join(", ")
            ));
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = WebSearchConfig::default();
        assert!(config.brave_api_key.is_none());
        assert!(config.fallback_enabled);
        assert_eq!(config.max_results, 5);
        assert_eq!(config.timeout_secs, 30);
        assert_eq!(config.cache_ttl_minutes, 15);
        assert_eq!(config.safe_search, "moderate");
    }

    #[test]
    fn test_testing_config() {
        let config = WebSearchConfig::for_testing();
        assert!(config.brave_api_key.is_none());
        assert_eq!(config.timeout_secs, 5);
        assert_eq!(config.max_results, 3);
        assert_eq!(config.cache_ttl_minutes, 0);
        assert!(!config.caching_enabled());
    }

    #[test]
    fn test_caching_enabled() {
        let mut config = WebSearchConfig::default();
        assert!(config.caching_enabled());

        config.cache_ttl_minutes = 0;
        assert!(!config.caching_enabled());
    }

    #[test]
    fn test_rate_limiting_enabled() {
        let mut config = WebSearchConfig::default();
        assert!(config.rate_limiting_enabled());

        config.rate_limit_daily = 0;
        assert!(!config.rate_limiting_enabled());
    }

    #[test]
    fn test_validation_success() {
        let config = WebSearchConfig::default();
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_validation_invalid_max_results() {
        let config = WebSearchConfig {
            max_results: 0,
            ..Default::default()
        };
        assert!(config.validate().is_err());

        let config = WebSearchConfig {
            max_results: 21,
            ..Default::default()
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_validation_invalid_timeout() {
        let config = WebSearchConfig {
            timeout_secs: 0,
            ..Default::default()
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_validation_invalid_safe_search() {
        let config = WebSearchConfig {
            safe_search: "invalid".to_string(),
            ..Default::default()
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_serialization() {
        let config = WebSearchConfig::default();
        let json = serde_json::to_string(&config).unwrap();
        let deserialized: WebSearchConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.max_results, config.max_results);
    }
}
