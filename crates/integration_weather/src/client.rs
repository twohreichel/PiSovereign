//! Open-Meteo weather client
//!
//! HTTP client for the Open-Meteo Weather API.

use async_trait::async_trait;
use chrono::{DateTime, NaiveDate, TimeZone, Utc};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tracing::{debug, instrument};

use crate::models::{ApiResponse, CurrentWeather, DailyForecast, Forecast, WeatherCondition};

/// Weather client errors
#[derive(Debug, Error)]
pub enum WeatherError {
    /// Connection to the weather service failed
    #[error("Connection failed: {0}")]
    ConnectionFailed(String),

    /// Request to the weather service failed
    #[error("Request failed: {0}")]
    RequestFailed(String),

    /// Failed to parse response from weather service
    #[error("Parse error: {0}")]
    ParseError(String),

    /// Invalid coordinates provided
    #[error("Invalid coordinates: latitude must be -90 to 90, longitude must be -180 to 180")]
    InvalidCoordinates,

    /// Service is temporarily unavailable
    #[error("Service unavailable: {0}")]
    ServiceUnavailable(String),

    /// Rate limit exceeded
    #[error("Rate limit exceeded")]
    RateLimitExceeded,
}

/// Weather service configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WeatherConfig {
    /// Open-Meteo API base URL (default: <https://api.open-meteo.com/v1>)
    #[serde(default = "default_base_url")]
    pub base_url: String,

    /// Connection timeout in seconds (default: 30)
    #[serde(default = "default_timeout")]
    pub timeout_secs: u64,

    /// Number of forecast days (1-16, default: 7)
    #[serde(default = "default_forecast_days")]
    pub forecast_days: u8,

    /// Cache TTL in minutes (default: 30)
    #[serde(default = "default_cache_ttl")]
    pub cache_ttl_minutes: u32,
}

fn default_base_url() -> String {
    "https://api.open-meteo.com/v1".to_string()
}

const fn default_timeout() -> u64 {
    30
}

const fn default_forecast_days() -> u8 {
    7
}

const fn default_cache_ttl() -> u32 {
    30
}

impl Default for WeatherConfig {
    fn default() -> Self {
        Self {
            base_url: default_base_url(),
            timeout_secs: default_timeout(),
            forecast_days: default_forecast_days(),
            cache_ttl_minutes: default_cache_ttl(),
        }
    }
}

/// Weather client trait for fetching weather data
#[async_trait]
pub trait WeatherClient: Send + Sync {
    /// Get current weather for a location
    async fn get_current(
        &self,
        latitude: f64,
        longitude: f64,
    ) -> Result<CurrentWeather, WeatherError>;

    /// Get weather forecast for a location
    async fn get_forecast(
        &self,
        latitude: f64,
        longitude: f64,
        days: u8,
    ) -> Result<Forecast, WeatherError>;

    /// Check if the weather service is healthy
    async fn is_healthy(&self) -> bool;
}

/// Open-Meteo HTTP client implementation
#[derive(Debug)]
pub struct OpenMeteoClient {
    client: Client,
    config: WeatherConfig,
}

impl OpenMeteoClient {
    /// Create a new Open-Meteo client with the given configuration
    ///
    /// # Errors
    ///
    /// Returns an error if the HTTP client cannot be initialized.
    pub fn new(config: WeatherConfig) -> Result<Self, WeatherError> {
        let client = Client::builder()
            .timeout(std::time::Duration::from_secs(config.timeout_secs))
            .build()
            .map_err(|e| WeatherError::ConnectionFailed(e.to_string()))?;

        Ok(Self { client, config })
    }

    /// Create a new client with default configuration
    ///
    /// # Errors
    ///
    /// Returns an error if the HTTP client cannot be initialized.
    pub fn with_defaults() -> Result<Self, WeatherError> {
        Self::new(WeatherConfig::default())
    }

    /// Validate coordinates
    fn validate_coordinates(latitude: f64, longitude: f64) -> Result<(), WeatherError> {
        if !(-90.0..=90.0).contains(&latitude) || !(-180.0..=180.0).contains(&longitude) {
            return Err(WeatherError::InvalidCoordinates);
        }
        Ok(())
    }

    /// Build the API URL for a forecast request
    fn build_forecast_url(&self, latitude: f64, longitude: f64, days: u8) -> String {
        let days = days.clamp(1, 16);
        format!(
            "{}/forecast?latitude={}&longitude={}&current={}&daily={}&timezone=auto&forecast_days={}",
            self.config.base_url,
            latitude,
            longitude,
            "temperature_2m,relative_humidity_2m,apparent_temperature,weather_code,\
             wind_speed_10m,wind_direction_10m,wind_gusts_10m,precipitation,cloud_cover,\
             surface_pressure",
            "weather_code,temperature_2m_max,temperature_2m_min,apparent_temperature_max,\
             apparent_temperature_min,sunrise,sunset,uv_index_max,precipitation_sum,\
             precipitation_probability_max,rain_sum,snowfall_sum,wind_speed_10m_max,\
             wind_gusts_10m_max,wind_direction_10m_dominant",
            days
        )
    }

    /// Parse current weather from API response
    fn parse_current_weather(
        data: &crate::models::WeatherData,
    ) -> Result<CurrentWeather, WeatherError> {
        let time = Self::parse_datetime(&data.time)?;

        Ok(CurrentWeather {
            time,
            temperature: data.temperature_2m,
            apparent_temperature: data.apparent_temperature,
            humidity: data.relative_humidity_2m,
            condition: WeatherCondition::from_wmo_code(data.weather_code),
            weather_code: data.weather_code,
            wind_speed: data.wind_speed_10m,
            wind_direction: data.wind_direction_10m,
            wind_gusts: data.wind_gusts_10m,
            precipitation: data.precipitation,
            cloud_cover: data.cloud_cover,
            pressure: data.surface_pressure,
            visibility: data.visibility,
        })
    }

    /// Parse daily forecasts from API response
    fn parse_daily_forecasts(
        daily_data: &crate::models::DailyData,
    ) -> Result<Vec<DailyForecast>, WeatherError> {
        let mut forecasts = Vec::with_capacity(daily_data.time.len());

        for i in 0..daily_data.time.len() {
            let forecast_date = NaiveDate::parse_from_str(&daily_data.time[i], "%Y-%m-%d")
                .map_err(|e| WeatherError::ParseError(format!("Invalid date: {e}")))?;

            let sunrise = Self::parse_datetime(&daily_data.sunrise[i])?;
            let sunset = Self::parse_datetime(&daily_data.sunset[i])?;

            let precipitation_probability = daily_data
                .precipitation_probability_max
                .as_ref()
                .and_then(|p| p.get(i).copied());

            forecasts.push(DailyForecast {
                date: forecast_date,
                condition: WeatherCondition::from_wmo_code(daily_data.weather_code[i]),
                weather_code: daily_data.weather_code[i],
                temperature_max: daily_data.temperature_2m_max[i],
                temperature_min: daily_data.temperature_2m_min[i],
                apparent_temperature_max: daily_data.apparent_temperature_max[i],
                apparent_temperature_min: daily_data.apparent_temperature_min[i],
                sunrise,
                sunset,
                uv_index_max: daily_data.uv_index_max[i],
                precipitation_sum: daily_data.precipitation_sum[i],
                precipitation_probability,
                rain_sum: daily_data.rain_sum[i],
                snowfall_sum: daily_data.snowfall_sum[i],
                wind_speed_max: daily_data.wind_speed_10m_max[i],
                wind_gusts_max: daily_data.wind_gusts_10m_max[i],
                wind_direction_dominant: daily_data.wind_direction_10m_dominant[i],
            });
        }

        Ok(forecasts)
    }

    /// Parse datetime string to `DateTime<Utc>`
    fn parse_datetime(s: &str) -> Result<DateTime<Utc>, WeatherError> {
        // Try ISO 8601 format first (2026-02-05T14:00)
        if let Ok(dt) = chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M") {
            return Ok(Utc.from_utc_datetime(&dt));
        }

        // Try with seconds
        if let Ok(dt) = chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S") {
            return Ok(Utc.from_utc_datetime(&dt));
        }

        // Try RFC 3339
        if let Ok(dt) = DateTime::parse_from_rfc3339(s) {
            return Ok(dt.with_timezone(&Utc));
        }

        Err(WeatherError::ParseError(format!(
            "Invalid datetime format: {s}"
        )))
    }
}

#[async_trait]
impl WeatherClient for OpenMeteoClient {
    #[instrument(skip(self), fields(lat = %latitude, lon = %longitude))]
    async fn get_current(
        &self,
        latitude: f64,
        longitude: f64,
    ) -> Result<CurrentWeather, WeatherError> {
        Self::validate_coordinates(latitude, longitude)?;

        let url = format!(
            "{}/forecast?latitude={}&longitude={}&current={}&timezone=auto",
            self.config.base_url,
            latitude,
            longitude,
            "temperature_2m,relative_humidity_2m,apparent_temperature,weather_code,\
             wind_speed_10m,wind_direction_10m,wind_gusts_10m,precipitation,cloud_cover,\
             surface_pressure"
        );

        debug!(url = %url, "Fetching current weather");

        let response = self
            .client
            .get(&url)
            .send()
            .await
            .map_err(|e| WeatherError::RequestFailed(e.to_string()))?;

        let status = response.status();
        if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
            return Err(WeatherError::RateLimitExceeded);
        }
        if status.is_server_error() {
            return Err(WeatherError::ServiceUnavailable(format!("HTTP {status}")));
        }
        if !status.is_success() {
            return Err(WeatherError::RequestFailed(format!("HTTP {status}")));
        }

        let api_response: ApiResponse = response
            .json()
            .await
            .map_err(|e| WeatherError::ParseError(e.to_string()))?;

        let current_data = api_response.current.ok_or_else(|| {
            WeatherError::ParseError("No current weather data in response".to_string())
        })?;

        Self::parse_current_weather(&current_data)
    }

    #[instrument(skip(self), fields(lat = %latitude, lon = %longitude, days = %days))]
    async fn get_forecast(
        &self,
        latitude: f64,
        longitude: f64,
        days: u8,
    ) -> Result<Forecast, WeatherError> {
        Self::validate_coordinates(latitude, longitude)?;

        let url = self.build_forecast_url(latitude, longitude, days);
        debug!(url = %url, "Fetching weather forecast");

        let response = self
            .client
            .get(&url)
            .send()
            .await
            .map_err(|e| WeatherError::RequestFailed(e.to_string()))?;

        let status = response.status();
        if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
            return Err(WeatherError::RateLimitExceeded);
        }
        if status.is_server_error() {
            return Err(WeatherError::ServiceUnavailable(format!("HTTP {status}")));
        }
        if !status.is_success() {
            return Err(WeatherError::RequestFailed(format!("HTTP {status}")));
        }

        let api_response: ApiResponse = response
            .json()
            .await
            .map_err(|e| WeatherError::ParseError(e.to_string()))?;

        let current_data = api_response.current.ok_or_else(|| {
            WeatherError::ParseError("No current weather data in response".to_string())
        })?;

        let daily_data = api_response.daily.ok_or_else(|| {
            WeatherError::ParseError("No daily forecast data in response".to_string())
        })?;

        let current = Self::parse_current_weather(&current_data)?;
        let daily = Self::parse_daily_forecasts(&daily_data)?;

        Ok(Forecast {
            current,
            daily,
            latitude: api_response.latitude,
            longitude: api_response.longitude,
            timezone: api_response.timezone,
            timezone_abbreviation: api_response.timezone_abbreviation,
            elevation: api_response.elevation,
        })
    }

    async fn is_healthy(&self) -> bool {
        // Simple health check using Berlin coordinates
        self.get_current(52.52, 13.41).await.is_ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_defaults() {
        let config = WeatherConfig::default();
        assert_eq!(config.base_url, "https://api.open-meteo.com/v1");
        assert_eq!(config.timeout_secs, 30);
        assert_eq!(config.forecast_days, 7);
        assert_eq!(config.cache_ttl_minutes, 30);
    }

    #[test]
    fn test_validate_coordinates_valid() {
        assert!(OpenMeteoClient::validate_coordinates(0.0, 0.0).is_ok());
        assert!(OpenMeteoClient::validate_coordinates(90.0, 180.0).is_ok());
        assert!(OpenMeteoClient::validate_coordinates(-90.0, -180.0).is_ok());
        assert!(OpenMeteoClient::validate_coordinates(52.52, 13.41).is_ok());
    }

    #[test]
    fn test_validate_coordinates_invalid() {
        assert!(OpenMeteoClient::validate_coordinates(91.0, 0.0).is_err());
        assert!(OpenMeteoClient::validate_coordinates(-91.0, 0.0).is_err());
        assert!(OpenMeteoClient::validate_coordinates(0.0, 181.0).is_err());
        assert!(OpenMeteoClient::validate_coordinates(0.0, -181.0).is_err());
    }

    #[test]
    fn test_build_forecast_url() {
        let config = WeatherConfig::default();
        let client = OpenMeteoClient::new(config).expect("client creation should succeed");

        let url = client.build_forecast_url(52.52, 13.41, 7);
        assert!(url.contains("latitude=52.52"));
        assert!(url.contains("longitude=13.41"));
        assert!(url.contains("forecast_days=7"));
        assert!(url.contains("current="));
        assert!(url.contains("daily="));
    }

    #[test]
    fn test_build_forecast_url_clamps_days() {
        let config = WeatherConfig::default();
        let client = OpenMeteoClient::new(config).expect("client creation should succeed");

        // Days should be clamped to 16 max
        let url = client.build_forecast_url(52.52, 13.41, 20);
        assert!(url.contains("forecast_days=16"));

        // Days should be clamped to 1 min
        let url = client.build_forecast_url(52.52, 13.41, 0);
        assert!(url.contains("forecast_days=1"));
    }

    #[test]
    fn test_parse_datetime_iso() {
        let dt = OpenMeteoClient::parse_datetime("2026-02-05T14:00").expect("should parse");
        assert_eq!(dt.format("%Y-%m-%d %H:%M").to_string(), "2026-02-05 14:00");
    }

    #[test]
    fn test_parse_datetime_with_seconds() {
        let dt = OpenMeteoClient::parse_datetime("2026-02-05T14:00:00").expect("should parse");
        assert_eq!(
            dt.format("%Y-%m-%d %H:%M:%S").to_string(),
            "2026-02-05 14:00:00"
        );
    }

    #[test]
    fn test_parse_datetime_invalid() {
        assert!(OpenMeteoClient::parse_datetime("invalid").is_err());
        assert!(OpenMeteoClient::parse_datetime("2026-02-05").is_err());
    }

    #[test]
    fn test_parse_current_weather() {
        let data = crate::models::WeatherData {
            time: "2026-02-05T14:00".to_string(),
            temperature_2m: 10.5,
            relative_humidity_2m: 75,
            apparent_temperature: 8.2,
            weather_code: 3,
            wind_speed_10m: 15.0,
            wind_direction_10m: 180,
            wind_gusts_10m: 25.0,
            precipitation: 0.5,
            cloud_cover: 80,
            surface_pressure: 1013.25,
            visibility: Some(10000.0),
        };

        let weather = OpenMeteoClient::parse_current_weather(&data).expect("should parse");
        assert!((weather.temperature - 10.5).abs() < f32::EPSILON);
        assert_eq!(weather.humidity, 75);
        assert_eq!(weather.condition, WeatherCondition::Overcast);
        assert_eq!(weather.weather_code, 3);
    }

    #[test]
    fn test_weather_error_display() {
        let err = WeatherError::InvalidCoordinates;
        assert!(err.to_string().contains("latitude"));
        assert!(err.to_string().contains("longitude"));

        let err = WeatherError::RateLimitExceeded;
        assert!(err.to_string().contains("Rate limit"));
    }

    #[test]
    fn test_client_creation() {
        let client = OpenMeteoClient::with_defaults();
        assert!(client.is_ok());
    }

    #[test]
    fn test_config_serialization() {
        let config = WeatherConfig {
            base_url: "https://custom.api.com".to_string(),
            timeout_secs: 60,
            forecast_days: 14,
            cache_ttl_minutes: 60,
        };

        let json = serde_json::to_string(&config).expect("should serialize");
        let deserialized: WeatherConfig = serde_json::from_str(&json).expect("should deserialize");

        assert_eq!(deserialized.base_url, "https://custom.api.com");
        assert_eq!(deserialized.timeout_secs, 60);
        assert_eq!(deserialized.forecast_days, 14);
    }
}
