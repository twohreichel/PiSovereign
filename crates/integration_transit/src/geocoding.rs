//! Nominatim geocoding client
//!
//! Converts free-form address strings to geographic coordinates using
//! the [Nominatim](https://nominatim.openstreetmap.org) API (OpenStreetMap).
//!
//! Implements rate limiting (max 1 request/second per Nominatim usage policy)
//! and result caching (24h TTL) to minimize API calls.

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use domain::value_objects::GeoLocation;
use moka::future::Cache;
use reqwest::Client;
use serde::Deserialize;
use thiserror::Error;
use tokio::sync::Mutex;
use tokio::time::Instant;
use tracing::{debug, instrument};

use serde::Serialize;

/// Configuration for the Nominatim geocoding service
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NominatimConfig {
    /// Base URL for the Nominatim API
    #[serde(default = "default_geocoding_base_url")]
    pub base_url: String,

    /// Connection timeout in seconds
    #[serde(default = "default_geocoding_timeout_secs")]
    pub timeout_secs: u64,

    /// Cache TTL in hours (0 to disable)
    #[serde(default = "default_cache_ttl_hours")]
    pub cache_ttl_hours: u64,

    /// Country code filter (e.g., "de" for Germany)
    #[serde(default = "default_country_filter")]
    pub country_filter: String,
}

fn default_geocoding_base_url() -> String {
    "https://nominatim.openstreetmap.org".to_string()
}

const fn default_geocoding_timeout_secs() -> u64 {
    5
}

const fn default_cache_ttl_hours() -> u64 {
    24
}

fn default_country_filter() -> String {
    "de".to_string()
}

impl Default for NominatimConfig {
    fn default() -> Self {
        Self {
            base_url: default_geocoding_base_url(),
            timeout_secs: default_geocoding_timeout_secs(),
            cache_ttl_hours: default_cache_ttl_hours(),
            country_filter: default_country_filter(),
        }
    }
}

impl NominatimConfig {
    /// Create a configuration suitable for testing
    #[must_use]
    pub fn for_testing() -> Self {
        Self {
            timeout_secs: 5,
            cache_ttl_hours: 0,
            ..Default::default()
        }
    }
}

/// Errors that can occur during geocoding
#[derive(Debug, Error)]
pub enum GeocodingError {
    /// Connection to geocoding service failed
    #[error("Geocoding connection failed: {0}")]
    ConnectionFailed(String),

    /// Request to geocoding service failed
    #[error("Geocoding request failed: {0}")]
    RequestFailed(String),

    /// Failed to parse geocoding response
    #[error("Geocoding parse error: {0}")]
    ParseError(String),

    /// Address could not be resolved to coordinates
    #[error("Address not found: {0}")]
    AddressNotFound(String),

    /// Rate limit exceeded (max 1 req/sec for Nominatim)
    #[error("Geocoding rate limit exceeded")]
    RateLimitExceeded,

    /// Request timeout
    #[error("Geocoding request timed out")]
    Timeout,
}

/// Trait for geocoding clients
#[async_trait]
pub trait GeocodingClient: Send + Sync {
    /// Convert a free-form address to geographic coordinates
    async fn geocode(&self, address: &str) -> Result<GeoLocation, GeocodingError>;

    /// Convert coordinates to a human-readable address
    async fn reverse_geocode(
        &self,
        latitude: f64,
        longitude: f64,
    ) -> Result<String, GeocodingError>;
}

/// Nominatim-based geocoding client with rate limiting and caching
#[derive(Debug)]
pub struct NominatimGeocodingClient {
    client: Client,
    config: NominatimConfig,
    cache: Cache<String, (f64, f64)>,
    last_request: Arc<Mutex<Instant>>,
}

impl NominatimGeocodingClient {
    /// Create a new Nominatim geocoding client
    ///
    /// # Errors
    ///
    /// Returns an error if the HTTP client cannot be initialized.
    pub fn new(config: &NominatimConfig) -> Result<Self, GeocodingError> {
        let client = Client::builder()
            .timeout(Duration::from_secs(config.timeout_secs))
            .user_agent("PiSovereign/1.0 (https://github.com/andreasreichel/PiSovereign)")
            .build()
            .map_err(|e| GeocodingError::ConnectionFailed(e.to_string()))?;

        let cache_ttl = if config.cache_ttl_hours > 0 {
            Duration::from_secs(config.cache_ttl_hours * 3600)
        } else {
            Duration::from_secs(1) // Minimal TTL when "disabled"
        };

        let cache = Cache::builder()
            .max_capacity(1000)
            .time_to_live(cache_ttl)
            .build();

        Ok(Self {
            client,
            config: config.clone(),
            cache,
            last_request: Arc::new(Mutex::new(Instant::now() - Duration::from_secs(2))),
        })
    }

    /// Enforce Nominatim's rate limit (max 1 request per second)
    async fn rate_limit(&self) {
        let mut last = self.last_request.lock().await;
        let elapsed = last.elapsed();
        if elapsed < Duration::from_millis(1100) {
            let wait = Duration::from_millis(1100).saturating_sub(elapsed);
            debug!(?wait, "Rate limiting geocoding request");
            tokio::time::sleep(wait).await;
        }
        *last = Instant::now();
    }
}

#[async_trait]
impl GeocodingClient for NominatimGeocodingClient {
    #[instrument(skip(self))]
    async fn geocode(&self, address: &str) -> Result<GeoLocation, GeocodingError> {
        let address = address.trim();
        if address.is_empty() {
            return Err(GeocodingError::AddressNotFound(
                "Address must not be empty".to_string(),
            ));
        }

        // Check cache first
        let cache_key = address.to_lowercase();
        if let Some((lat, lon)) = self.cache.get(&cache_key).await {
            debug!(%address, "Geocoding cache hit");
            return GeoLocation::new(lat, lon)
                .map_err(|e| GeocodingError::ParseError(e.to_string()));
        }

        self.rate_limit().await;

        let url = format!("{}/search", self.config.base_url);
        let mut params = vec![
            ("q", address.to_string()),
            ("format", "jsonv2".to_string()),
            ("limit", "1".to_string()),
            ("accept-language", "de,en".to_string()),
        ];

        if !self.config.country_filter.is_empty() {
            params.push(("countrycodes", self.config.country_filter.clone()));
        }

        debug!(%address, "Geocoding address");

        let response = self
            .client
            .get(&url)
            .query(&params)
            .send()
            .await
            .map_err(|e| {
                if e.is_timeout() {
                    GeocodingError::Timeout
                } else {
                    GeocodingError::ConnectionFailed(e.to_string())
                }
            })?;

        if !response.status().is_success() {
            return Err(GeocodingError::RequestFailed(format!(
                "HTTP {}",
                response.status()
            )));
        }

        let results: Vec<NominatimResult> = response
            .json()
            .await
            .map_err(|e| GeocodingError::ParseError(e.to_string()))?;

        let result = results
            .first()
            .ok_or_else(|| GeocodingError::AddressNotFound(address.to_string()))?;

        let lat: f64 = result
            .lat
            .parse()
            .map_err(|_| GeocodingError::ParseError("Invalid latitude".to_string()))?;
        let lon: f64 = result
            .lon
            .parse()
            .map_err(|_| GeocodingError::ParseError("Invalid longitude".to_string()))?;

        // Cache the result
        self.cache.insert(cache_key, (lat, lon)).await;
        debug!(%address, %lat, %lon, "Geocoded address");

        GeoLocation::new(lat, lon).map_err(|e| GeocodingError::ParseError(e.to_string()))
    }

    #[instrument(skip(self))]
    async fn reverse_geocode(
        &self,
        latitude: f64,
        longitude: f64,
    ) -> Result<String, GeocodingError> {
        self.rate_limit().await;

        let url = format!("{}/reverse", self.config.base_url);
        let params = [
            ("lat", latitude.to_string()),
            ("lon", longitude.to_string()),
            ("format", "jsonv2".to_string()),
            ("accept-language", "de,en".to_string()),
        ];

        debug!(%latitude, %longitude, "Reverse geocoding");

        let response = self
            .client
            .get(&url)
            .query(&params)
            .send()
            .await
            .map_err(|e| {
                if e.is_timeout() {
                    GeocodingError::Timeout
                } else {
                    GeocodingError::ConnectionFailed(e.to_string())
                }
            })?;

        if !response.status().is_success() {
            return Err(GeocodingError::RequestFailed(format!(
                "HTTP {}",
                response.status()
            )));
        }

        let result: NominatimResult = response
            .json()
            .await
            .map_err(|e| GeocodingError::ParseError(e.to_string()))?;

        result
            .display_name
            .ok_or_else(|| GeocodingError::AddressNotFound(format!("{latitude},{longitude}")))
    }
}

/// Raw Nominatim API response
#[derive(Debug, Deserialize)]
struct NominatimResult {
    lat: String,
    lon: String,
    display_name: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_nominatim_config_default() {
        let config = NominatimConfig::default();
        assert_eq!(config.base_url, "https://nominatim.openstreetmap.org");
        assert_eq!(config.timeout_secs, 5);
        assert_eq!(config.cache_ttl_hours, 24);
        assert_eq!(config.country_filter, "de");
    }

    #[test]
    fn test_nominatim_config_for_testing() {
        let config = NominatimConfig::for_testing();
        assert_eq!(config.timeout_secs, 5);
        assert_eq!(config.cache_ttl_hours, 0);
    }

    #[test]
    fn test_geocoding_error_display() {
        let err = GeocodingError::AddressNotFound("Berlin Hbf".to_string());
        assert!(err.to_string().contains("Berlin Hbf"));

        let err = GeocodingError::Timeout;
        assert!(err.to_string().contains("timed out"));
    }

    #[test]
    fn test_nominatim_result_parsing() {
        let json = r#"[{"lat": "52.52", "lon": "13.37", "display_name": "Berlin"}]"#;
        let results: Vec<NominatimResult> = serde_json::from_str(json).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].lat, "52.52");
        assert_eq!(results[0].lon, "13.37");
        assert_eq!(results[0].display_name.as_deref(), Some("Berlin"));
    }

    #[test]
    fn test_nominatim_empty_result() {
        let json = r"[]";
        let results: Vec<NominatimResult> = serde_json::from_str(json).unwrap();
        assert!(results.is_empty());
    }

    #[test]
    fn test_nominatim_config_serialization() {
        let config = NominatimConfig::default();
        let json = serde_json::to_string(&config).unwrap();
        let deserialized: NominatimConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.base_url, config.base_url);
        assert_eq!(deserialized.country_filter, config.country_filter);
    }
}
