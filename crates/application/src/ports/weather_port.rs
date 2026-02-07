//! Weather service port
//!
//! Defines the interface for weather data retrieval.

use async_trait::async_trait;
use chrono::{DateTime, NaiveDate, Utc};
use domain::value_objects::GeoLocation;
#[cfg(test)]
use mockall::automock;
use serde::{Deserialize, Serialize};

use crate::error::ApplicationError;

/// Current weather conditions
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CurrentWeather {
    /// Temperature in Celsius
    pub temperature: f64,
    /// Apparent/feels-like temperature in Celsius
    pub apparent_temperature: f64,
    /// Relative humidity in percent (0-100)
    pub humidity: u8,
    /// Wind speed in km/h
    pub wind_speed: f64,
    /// Weather condition description
    pub condition: WeatherCondition,
    /// When this data was observed
    pub observed_at: DateTime<Utc>,
}

/// Weather forecast for a specific day
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DailyForecast {
    /// The date of the forecast
    pub date: NaiveDate,
    /// Maximum temperature in Celsius
    pub temperature_max: f64,
    /// Minimum temperature in Celsius
    pub temperature_min: f64,
    /// Weather condition
    pub condition: WeatherCondition,
    /// Precipitation probability (0-100)
    pub precipitation_probability: u8,
    /// Expected precipitation in mm
    pub precipitation_sum: f64,
    /// Sunrise time (UTC)
    pub sunrise: Option<DateTime<Utc>>,
    /// Sunset time (UTC)
    pub sunset: Option<DateTime<Utc>>,
}

/// Weather conditions
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WeatherCondition {
    /// Clear sky
    ClearSky,
    /// Mainly clear
    MainlyClear,
    /// Partly cloudy
    PartlyCloudy,
    /// Overcast
    Overcast,
    /// Foggy
    Fog,
    /// Drizzle
    Drizzle,
    /// Light rain
    LightRain,
    /// Moderate rain
    ModerateRain,
    /// Heavy rain
    HeavyRain,
    /// Snow
    Snow,
    /// Thunderstorm
    Thunderstorm,
    /// Unknown condition
    Unknown,
}

impl WeatherCondition {
    /// Get a human-readable description
    #[must_use]
    pub const fn description(&self) -> &'static str {
        match self {
            Self::ClearSky => "Clear sky",
            Self::MainlyClear => "Mainly clear",
            Self::PartlyCloudy => "Partly cloudy",
            Self::Overcast => "Overcast",
            Self::Fog => "Foggy",
            Self::Drizzle => "Light drizzle",
            Self::LightRain => "Light rain",
            Self::ModerateRain => "Moderate rain",
            Self::HeavyRain => "Heavy rain",
            Self::Snow => "Snow",
            Self::Thunderstorm => "Thunderstorm",
            Self::Unknown => "Unknown",
        }
    }

    /// Get an emoji representation
    #[must_use]
    pub const fn emoji(&self) -> &'static str {
        match self {
            Self::ClearSky => "☀️",
            Self::MainlyClear => "🌤️",
            Self::PartlyCloudy => "⛅",
            Self::Overcast => "☁️",
            Self::Fog => "🌫️",
            Self::Drizzle | Self::LightRain | Self::ModerateRain | Self::HeavyRain => "🌧️",
            Self::Snow => "❄️",
            Self::Thunderstorm => "⛈️",
            Self::Unknown => "❓",
        }
    }
}

impl std::fmt::Display for WeatherCondition {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.description())
    }
}

/// Port for weather service operations
#[allow(clippy::struct_field_names)] // automock generates struct with `get_*` prefixes
#[cfg_attr(test, automock)]
#[async_trait]
pub trait WeatherPort: Send + Sync {
    /// Get current weather for a location
    async fn get_current_weather(
        &self,
        location: &GeoLocation,
    ) -> Result<CurrentWeather, ApplicationError>;

    /// Get weather forecast for upcoming days
    ///
    /// # Arguments
    /// * `location` - Geographic location
    /// * `days` - Number of days to forecast (typically 1-7)
    async fn get_forecast(
        &self,
        location: &GeoLocation,
        days: u8,
    ) -> Result<Vec<DailyForecast>, ApplicationError>;

    /// Check if the weather service is available
    async fn is_available(&self) -> bool;

    /// Get both current weather and forecast in a single call
    async fn get_weather_summary(
        &self,
        location: &GeoLocation,
        forecast_days: u8,
    ) -> Result<(CurrentWeather, Vec<DailyForecast>), ApplicationError> {
        let current = self.get_current_weather(location).await?;
        let forecast = self.get_forecast(location, forecast_days).await?;
        Ok((current, forecast))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn _assert_object_safe(_: &dyn WeatherPort) {}

    #[test]
    fn trait_is_send_sync() {
        fn assert_send_sync<T: Send + Sync + ?Sized>() {}
        assert_send_sync::<dyn WeatherPort>();
    }

    #[test]
    fn weather_condition_display() {
        assert_eq!(WeatherCondition::ClearSky.to_string(), "Clear sky");
        assert_eq!(WeatherCondition::Thunderstorm.description(), "Thunderstorm");
    }

    #[test]
    fn weather_condition_emoji() {
        assert_eq!(WeatherCondition::ClearSky.emoji(), "☀️");
        assert_eq!(WeatherCondition::Snow.emoji(), "❄️");
    }
}
