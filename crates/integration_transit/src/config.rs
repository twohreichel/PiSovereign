//! Transit service configuration

use serde::{Deserialize, Serialize};

/// Configuration for the public transit service (transport.rest / HAFAS)
#[allow(clippy::struct_excessive_bools)] // Configuration needs multiple boolean flags
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransitConfig {
    /// Base URL for the transport.rest API
    #[serde(default = "default_base_url")]
    pub base_url: String,

    /// Connection timeout in seconds
    #[serde(default = "default_timeout_secs")]
    pub timeout_secs: u64,

    /// Maximum number of journey results to return
    #[serde(default = "default_max_results")]
    pub max_results: u8,

    /// Cache TTL in minutes (0 to disable caching)
    #[serde(default = "default_cache_ttl_minutes")]
    pub cache_ttl_minutes: u32,

    /// Automatically include transit info in location-based reminders
    #[serde(default = "default_include_in_reminders")]
    pub include_in_reminders: bool,

    /// Include bus connections
    #[serde(default = "default_true")]
    pub products_bus: bool,

    /// Include S-Bahn connections
    #[serde(default = "default_true")]
    pub products_suburban: bool,

    /// Include U-Bahn connections
    #[serde(default = "default_true")]
    pub products_subway: bool,

    /// Include tram connections
    #[serde(default = "default_true")]
    pub products_tram: bool,

    /// Include regional train connections (RB/RE)
    #[serde(default = "default_true")]
    pub products_regional: bool,

    /// Include national train connections (ICE/IC)
    #[serde(default = "default_false")]
    pub products_national: bool,

    /// Include national express connections (ICE)
    #[serde(default = "default_false")]
    pub products_national_express: bool,
}

fn default_base_url() -> String {
    "https://v6.db.transport.rest".to_string()
}

const fn default_timeout_secs() -> u64 {
    10
}

const fn default_max_results() -> u8 {
    3
}

const fn default_cache_ttl_minutes() -> u32 {
    5
}

const fn default_include_in_reminders() -> bool {
    true
}

const fn default_true() -> bool {
    true
}

const fn default_false() -> bool {
    false
}

impl Default for TransitConfig {
    fn default() -> Self {
        Self {
            base_url: default_base_url(),
            timeout_secs: default_timeout_secs(),
            max_results: default_max_results(),
            cache_ttl_minutes: default_cache_ttl_minutes(),
            include_in_reminders: default_include_in_reminders(),
            products_bus: true,
            products_suburban: true,
            products_subway: true,
            products_tram: true,
            products_regional: true,
            products_national: false,
            products_national_express: false,
        }
    }
}

impl TransitConfig {
    /// Create a configuration suitable for testing
    #[must_use]
    pub fn for_testing() -> Self {
        Self {
            timeout_secs: 5,
            max_results: 2,
            cache_ttl_minutes: 0,
            ..Default::default()
        }
    }

    /// Check if caching is enabled
    #[must_use]
    pub const fn caching_enabled(&self) -> bool {
        self.cache_ttl_minutes > 0
    }

    /// Validate the configuration
    ///
    /// # Errors
    ///
    /// Returns an error if the configuration is invalid.
    pub fn validate(&self) -> Result<(), String> {
        if self.base_url.is_empty() {
            return Err("base_url must not be empty".to_string());
        }

        if self.timeout_secs == 0 {
            return Err("timeout_secs must be greater than 0".to_string());
        }

        if self.max_results == 0 {
            return Err("max_results must be greater than 0".to_string());
        }

        if self.max_results > 10 {
            return Err("max_results must be 10 or less".to_string());
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = TransitConfig::default();
        assert_eq!(config.base_url, "https://v6.db.transport.rest");
        assert_eq!(config.timeout_secs, 10);
        assert_eq!(config.max_results, 3);
        assert_eq!(config.cache_ttl_minutes, 5);
        assert!(config.include_in_reminders);
        assert!(config.products_bus);
        assert!(config.products_suburban);
        assert!(config.products_subway);
        assert!(config.products_tram);
        assert!(config.products_regional);
        assert!(!config.products_national);
        assert!(!config.products_national_express);
    }

    #[test]
    fn test_testing_config() {
        let config = TransitConfig::for_testing();
        assert_eq!(config.timeout_secs, 5);
        assert_eq!(config.max_results, 2);
        assert!(!config.caching_enabled());
    }

    #[test]
    fn test_caching_enabled() {
        let mut config = TransitConfig::default();
        assert!(config.caching_enabled());

        config.cache_ttl_minutes = 0;
        assert!(!config.caching_enabled());
    }

    #[test]
    fn test_validation_success() {
        let config = TransitConfig::default();
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_validation_empty_base_url() {
        let config = TransitConfig {
            base_url: String::new(),
            ..Default::default()
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_validation_zero_timeout() {
        let config = TransitConfig {
            timeout_secs: 0,
            ..Default::default()
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_validation_zero_max_results() {
        let config = TransitConfig {
            max_results: 0,
            ..Default::default()
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_validation_too_many_results() {
        let config = TransitConfig {
            max_results: 11,
            ..Default::default()
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_serialization_roundtrip() {
        let config = TransitConfig::default();
        let json = serde_json::to_string(&config).unwrap();
        let deserialized: TransitConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.max_results, config.max_results);
        assert_eq!(deserialized.base_url, config.base_url);
    }
}
