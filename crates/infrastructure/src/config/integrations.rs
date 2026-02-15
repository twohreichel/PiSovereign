//! Integration configurations: Weather, Web Search, CalDAV, Proton Mail, Transit.

use secrecy::{ExposeSecret, SecretString};
use serde::{Deserialize, Serialize};

use super::default_true;

// ==============================
// Weather Configuration
// ==============================

/// Weather service configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WeatherConfig {
    /// Open-Meteo API base URL
    #[serde(default = "default_weather_base_url")]
    pub base_url: String,

    /// Connection timeout in seconds
    #[serde(default = "default_weather_timeout")]
    pub timeout_secs: u64,

    /// Number of forecast days (1-16)
    #[serde(default = "default_forecast_days")]
    pub forecast_days: u8,

    /// Cache TTL in minutes
    #[serde(default = "default_cache_ttl_minutes")]
    pub cache_ttl_minutes: u32,

    /// Default location for weather when user profile has no location
    ///
    /// Configured as inline table: `{ latitude = 52.52, longitude = 13.405 }`
    #[serde(default)]
    pub default_location: Option<GeoLocationConfig>,
}

/// Geographic location configuration (latitude/longitude pair)
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct GeoLocationConfig {
    /// Latitude (-90.0 to 90.0)
    pub latitude: f64,
    /// Longitude (-180.0 to 180.0)
    pub longitude: f64,
}

impl GeoLocationConfig {
    /// Convert to domain `GeoLocation` value object
    ///
    /// Returns `None` if coordinates are invalid.
    #[must_use]
    pub fn to_geo_location(&self) -> Option<domain::GeoLocation> {
        domain::GeoLocation::new(self.latitude, self.longitude).ok()
    }
}

fn default_weather_base_url() -> String {
    "https://api.open-meteo.com/v1".to_string()
}

const fn default_weather_timeout() -> u64 {
    30
}

const fn default_forecast_days() -> u8 {
    7
}

const fn default_cache_ttl_minutes() -> u32 {
    30
}

impl Default for WeatherConfig {
    fn default() -> Self {
        Self {
            base_url: default_weather_base_url(),
            timeout_secs: default_weather_timeout(),
            forecast_days: default_forecast_days(),
            cache_ttl_minutes: default_cache_ttl_minutes(),
            default_location: None,
        }
    }
}

// ==============================
// Web Search Configuration
// ==============================

/// Web search service configuration
///
/// Configures web search integration using Brave Search (primary) and DuckDuckGo (fallback).
/// Get your Brave API key at: <https://brave.com/search/api/>
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebSearchAppConfig {
    /// Brave Search API key (required for Brave, optional if using DuckDuckGo only)
    ///
    /// Obtain from <https://brave.com/search/api/>
    #[serde(default)]
    pub api_key: Option<String>,

    /// Maximum number of search results to return (1-10)
    #[serde(default = "default_websearch_max_results")]
    pub max_results: u32,

    /// Connection timeout in seconds
    #[serde(default = "default_websearch_timeout")]
    pub timeout_secs: u64,

    /// Enable DuckDuckGo fallback when Brave fails or returns no results
    #[serde(default = "default_true")]
    pub fallback_enabled: bool,

    /// Safe search level: "off", "moderate", or "strict"
    #[serde(default = "default_safe_search")]
    pub safe_search: String,

    /// Country code for search results (e.g., "DE", "US", "GB")
    #[serde(default)]
    pub country: Option<String>,

    /// Language code for search results (e.g., "de", "en", "fr")
    #[serde(default)]
    pub language: Option<String>,

    /// Rate limit: maximum requests per minute (0 = unlimited)
    #[serde(default)]
    pub rate_limit_rpm: Option<u32>,

    /// Cache TTL in minutes for search results
    #[serde(default = "default_websearch_cache_ttl")]
    pub cache_ttl_minutes: u32,
}

const fn default_websearch_max_results() -> u32 {
    5
}

const fn default_websearch_timeout() -> u64 {
    30
}

fn default_safe_search() -> String {
    "moderate".to_string()
}

const fn default_websearch_cache_ttl() -> u32 {
    30
}

impl Default for WebSearchAppConfig {
    fn default() -> Self {
        Self {
            api_key: None,
            max_results: default_websearch_max_results(),
            timeout_secs: default_websearch_timeout(),
            fallback_enabled: true,
            safe_search: default_safe_search(),
            country: None,
            language: None,
            rate_limit_rpm: None,
            cache_ttl_minutes: default_websearch_cache_ttl(),
        }
    }
}

impl WebSearchAppConfig {
    /// Convert to `integration_websearch` config
    #[must_use]
    pub fn to_websearch_config(&self) -> integration_websearch::WebSearchConfig {
        let mut config = integration_websearch::WebSearchConfig::default();
        config.brave_api_key.clone_from(&self.api_key);
        config.max_results = self.max_results as usize;
        config.timeout_secs = self.timeout_secs;
        config.fallback_enabled = self.fallback_enabled;
        config.safe_search.clone_from(&self.safe_search);
        config.cache_ttl_minutes = self.cache_ttl_minutes;
        if let Some(ref country) = self.country {
            config.result_country.clone_from(country);
        }
        if let Some(ref language) = self.language {
            config.result_language.clone_from(language);
        }
        if let Some(rpm) = self.rate_limit_rpm {
            // Convert RPM to daily rate (approximate)
            config.rate_limit_daily = rpm * 60 * 24;
        }
        config
    }
}

// ==============================
// CalDAV Configuration
// ==============================

/// CalDAV calendar server configuration
#[derive(Clone, Serialize, Deserialize)]
pub struct CalDavAppConfig {
    /// CalDAV server URL (e.g., <https://cal.example.com>)
    pub server_url: String,

    /// Username for authentication
    pub username: String,

    /// Password for authentication (sensitive - uses `SecretString`)
    #[serde(skip_serializing)]
    pub password: SecretString,

    /// Default calendar path (optional)
    #[serde(default)]
    pub calendar_path: Option<String>,

    /// Verify TLS certificates (default: true)
    #[serde(default = "default_true")]
    pub verify_certs: bool,

    /// Connection timeout in seconds (default: 30)
    #[serde(default = "default_caldav_timeout")]
    pub timeout_secs: u64,
}

impl std::fmt::Debug for CalDavAppConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CalDavAppConfig")
            .field("server_url", &self.server_url)
            .field("username", &self.username)
            .field("password", &"[REDACTED]")
            .field("calendar_path", &self.calendar_path)
            .field("verify_certs", &self.verify_certs)
            .field("timeout_secs", &self.timeout_secs)
            .finish()
    }
}

const fn default_caldav_timeout() -> u64 {
    30
}

impl CalDavAppConfig {
    /// Convert to `integration_caldav`'s `CalDavConfig`
    #[must_use]
    pub fn to_caldav_config(&self) -> integration_caldav::CalDavConfig {
        integration_caldav::CalDavConfig {
            server_url: self.server_url.clone(),
            username: self.username.clone(),
            password: self.password.expose_secret().to_string(),
            calendar_path: self.calendar_path.clone(),
            verify_certs: self.verify_certs,
            timeout_secs: self.timeout_secs,
        }
    }

    /// Get the password as a string reference
    #[must_use]
    pub fn password_str(&self) -> &str {
        self.password.expose_secret()
    }
}

// ==============================
// CardDAV Configuration
// ==============================

/// CardDAV contact server configuration
///
/// Shares the same server credentials as CalDAV when both are pointed
/// at the same DAV server (e.g., Baikal). Can also be configured
/// independently.
#[derive(Clone, Serialize, Deserialize)]
pub struct CardDavAppConfig {
    /// CardDAV server URL (e.g., <https://dav.example.com/dav.php>)
    pub server_url: String,

    /// Username for authentication
    pub username: String,

    /// Password for authentication (sensitive - uses `SecretString`)
    #[serde(skip_serializing)]
    pub password: SecretString,

    /// Default addressbook path (optional, auto-discovered if not set)
    #[serde(default)]
    pub addressbook_path: Option<String>,

    /// Verify TLS certificates (default: true)
    #[serde(default = "default_true")]
    pub verify_certs: bool,

    /// Connection timeout in seconds (default: 30)
    #[serde(default = "default_carddav_timeout")]
    pub timeout_secs: u64,
}

impl std::fmt::Debug for CardDavAppConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CardDavAppConfig")
            .field("server_url", &self.server_url)
            .field("username", &self.username)
            .field("password", &"[REDACTED]")
            .field("addressbook_path", &self.addressbook_path)
            .field("verify_certs", &self.verify_certs)
            .field("timeout_secs", &self.timeout_secs)
            .finish()
    }
}

const fn default_carddav_timeout() -> u64 {
    30
}

impl CardDavAppConfig {
    /// Convert to `integration_carddav`'s `CardDavConfig`
    #[must_use]
    pub fn to_carddav_config(&self) -> integration_carddav::CardDavConfig {
        integration_carddav::CardDavConfig {
            server_url: self.server_url.clone(),
            username: self.username.clone(),
            password: self.password.expose_secret().to_string(),
            addressbook_path: self.addressbook_path.clone(),
            verify_certs: self.verify_certs,
            timeout_secs: self.timeout_secs,
        }
    }

    /// Create a `CardDavAppConfig` from an existing `CalDavAppConfig`.
    ///
    /// This enables sharing credentials when CalDAV and CardDAV point to the
    /// same server (e.g., Baikal).
    #[must_use]
    pub fn from_caldav(caldav: &CalDavAppConfig) -> Self {
        Self {
            server_url: caldav.server_url.clone(),
            username: caldav.username.clone(),
            password: SecretString::from(caldav.password.expose_secret().to_string()),
            addressbook_path: None,
            verify_certs: caldav.verify_certs,
            timeout_secs: caldav.timeout_secs,
        }
    }

    /// Get the password as a string reference
    #[must_use]
    pub fn password_str(&self) -> &str {
        self.password.expose_secret()
    }
}

// ==============================
// Proton Mail Configuration
// ==============================

/// Proton Mail configuration (via Proton Bridge)
#[derive(Clone, Serialize, Deserialize)]
pub struct ProtonAppConfig {
    /// IMAP server host (default: 127.0.0.1)
    #[serde(default = "default_proton_host")]
    pub imap_host: String,

    /// IMAP server port (default: 1143 for STARTTLS)
    #[serde(default = "default_imap_port")]
    pub imap_port: u16,

    /// SMTP server host (default: 127.0.0.1)
    #[serde(default = "default_proton_host")]
    pub smtp_host: String,

    /// SMTP server port (default: 1025 for STARTTLS)
    #[serde(default = "default_smtp_port")]
    pub smtp_port: u16,

    /// Email address (Bridge account email)
    pub email: String,

    /// Bridge password (from Bridge UI, NOT Proton account password)
    /// Sensitive - uses `SecretString` for zeroization
    #[serde(skip_serializing)]
    pub password: SecretString,

    /// TLS configuration
    #[serde(default)]
    pub tls: ProtonTlsAppConfig,
}

impl std::fmt::Debug for ProtonAppConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ProtonAppConfig")
            .field("imap_host", &self.imap_host)
            .field("imap_port", &self.imap_port)
            .field("smtp_host", &self.smtp_host)
            .field("smtp_port", &self.smtp_port)
            .field("email", &self.email)
            .field("password", &"[REDACTED]")
            .field("tls", &self.tls)
            .finish()
    }
}

fn default_proton_host() -> String {
    "127.0.0.1".to_string()
}

const fn default_imap_port() -> u16 {
    1143
}

const fn default_smtp_port() -> u16 {
    1025
}

/// TLS configuration for Proton Bridge connections
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ProtonTlsAppConfig {
    /// Verify TLS certificates (default: true)
    #[serde(default = "default_true")]
    pub verify_certificates: bool,

    /// Minimum TLS version ("1.2" or "1.3")
    #[serde(default = "default_min_tls")]
    pub min_tls_version: String,

    /// Path to custom CA certificate (optional)
    #[serde(default)]
    pub ca_cert_path: Option<String>,
}

fn default_min_tls() -> String {
    "1.2".to_string()
}

impl ProtonAppConfig {
    /// Convert to `integration_proton`'s `ProtonConfig`
    #[must_use]
    pub fn to_proton_config(&self) -> integration_proton::ProtonConfig {
        integration_proton::ProtonConfig {
            imap_host: self.imap_host.clone(),
            imap_port: self.imap_port,
            smtp_host: self.smtp_host.clone(),
            smtp_port: self.smtp_port,
            email: self.email.clone(),
            password: self.password.expose_secret().to_string(),
            tls: integration_proton::TlsConfig {
                verify_certificates: Some(self.tls.verify_certificates),
                min_tls_version: self.tls.min_tls_version.clone(),
                ca_cert_path: self.tls.ca_cert_path.as_ref().map(std::path::PathBuf::from),
            },
        }
    }

    /// Get the password as a string reference
    #[must_use]
    pub fn password_str(&self) -> &str {
        self.password.expose_secret()
    }
}

// ==============================
// Transit Configuration
// ==============================

/// Public transit configuration for ÖPNV connections
///
/// Configures the `transport.rest` API integration for German public transit.
#[allow(clippy::struct_excessive_bools)] // Configuration needs multiple boolean flags
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransitAppConfig {
    /// Base URL for transport.rest API (default: v6.db.transport.rest)
    #[serde(default = "default_transit_base_url")]
    pub base_url: String,

    /// Request timeout in seconds (default: 10)
    #[serde(default = "default_transit_timeout")]
    pub timeout_secs: u64,

    /// Maximum number of journey results (default: 3)
    #[serde(default = "default_transit_max_results")]
    pub max_results: u8,

    /// Cache TTL in minutes (default: 5)
    #[serde(default = "default_transit_cache_ttl")]
    pub cache_ttl_minutes: u32,

    /// Include transit info in location-based reminders (default: true)
    #[serde(default = "default_true")]
    pub include_in_reminders: bool,

    /// Include bus connections (default: true)
    #[serde(default = "default_true")]
    pub products_bus: bool,

    /// Include S-Bahn connections (default: true)
    #[serde(default = "default_true")]
    pub products_suburban: bool,

    /// Include U-Bahn connections (default: true)
    #[serde(default = "default_true")]
    pub products_subway: bool,

    /// Include tram connections (default: true)
    #[serde(default = "default_true")]
    pub products_tram: bool,

    /// Include regional train connections (default: true)
    #[serde(default = "default_true")]
    pub products_regional: bool,

    /// Include national train connections (default: false)
    #[serde(default)]
    pub products_national: bool,

    /// User's home location for calculating routes (optional)
    #[serde(default)]
    pub home_location: Option<GeoLocationConfig>,
}

fn default_transit_base_url() -> String {
    "https://v6.db.transport.rest".to_string()
}

const fn default_transit_timeout() -> u64 {
    10
}

const fn default_transit_max_results() -> u8 {
    3
}

const fn default_transit_cache_ttl() -> u32 {
    5
}

impl Default for TransitAppConfig {
    fn default() -> Self {
        Self {
            base_url: default_transit_base_url(),
            timeout_secs: default_transit_timeout(),
            max_results: default_transit_max_results(),
            cache_ttl_minutes: default_transit_cache_ttl(),
            include_in_reminders: true,
            products_bus: true,
            products_suburban: true,
            products_subway: true,
            products_tram: true,
            products_regional: true,
            products_national: false,
            home_location: None,
        }
    }
}

impl TransitAppConfig {
    /// Convert to `integration_transit::TransitConfig`
    #[must_use]
    pub fn to_transit_config(&self) -> integration_transit::TransitConfig {
        integration_transit::TransitConfig {
            base_url: self.base_url.clone(),
            timeout_secs: self.timeout_secs,
            max_results: self.max_results,
            cache_ttl_minutes: self.cache_ttl_minutes,
            include_in_reminders: self.include_in_reminders,
            products_bus: self.products_bus,
            products_suburban: self.products_suburban,
            products_subway: self.products_subway,
            products_tram: self.products_tram,
            products_regional: self.products_regional,
            products_national: self.products_national,
            products_national_express: false,
        }
    }
}
