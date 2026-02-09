//! Weather data models
//!
//! Types for representing weather data from Open-Meteo API.

use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};

/// Weather condition derived from WMO weather codes
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WeatherCondition {
    /// Clear sky (WMO 0)
    ClearSky,
    /// Mainly clear (WMO 1)
    MainlyClear,
    /// Partly cloudy (WMO 2)
    PartlyCloudy,
    /// Overcast (WMO 3)
    Overcast,
    /// Fog (WMO 45, 48)
    Fog,
    /// Drizzle (WMO 51, 53, 55)
    Drizzle,
    /// Freezing drizzle (WMO 56, 57)
    FreezingDrizzle,
    /// Rain (WMO 61, 63, 65)
    Rain,
    /// Freezing rain (WMO 66, 67)
    FreezingRain,
    /// Snow (WMO 71, 73, 75)
    Snow,
    /// Snow grains (WMO 77)
    SnowGrains,
    /// Rain showers (WMO 80, 81, 82)
    RainShowers,
    /// Snow showers (WMO 85, 86)
    SnowShowers,
    /// Thunderstorm (WMO 95)
    Thunderstorm,
    /// Thunderstorm with hail (WMO 96, 99)
    ThunderstormWithHail,
    /// Unknown condition
    Unknown,
}

impl WeatherCondition {
    /// Convert WMO weather code to `WeatherCondition`
    ///
    /// See: <https://open-meteo.com/en/docs> for WMO code reference
    #[must_use]
    pub const fn from_wmo_code(code: u8) -> Self {
        match code {
            0 => Self::ClearSky,
            1 => Self::MainlyClear,
            2 => Self::PartlyCloudy,
            3 => Self::Overcast,
            45 | 48 => Self::Fog,
            51 | 53 | 55 => Self::Drizzle,
            56 | 57 => Self::FreezingDrizzle,
            61 | 63 | 65 => Self::Rain,
            66 | 67 => Self::FreezingRain,
            71 | 73 | 75 => Self::Snow,
            77 => Self::SnowGrains,
            80..=82 => Self::RainShowers,
            85 | 86 => Self::SnowShowers,
            95 => Self::Thunderstorm,
            96 | 99 => Self::ThunderstormWithHail,
            _ => Self::Unknown,
        }
    }

    /// Get a human-readable description of the weather condition
    #[must_use]
    pub const fn description(&self) -> &'static str {
        match self {
            Self::ClearSky => "Clear sky",
            Self::MainlyClear => "Mainly clear",
            Self::PartlyCloudy => "Partly cloudy",
            Self::Overcast => "Overcast",
            Self::Fog => "Fog",
            Self::Drizzle => "Drizzle",
            Self::FreezingDrizzle => "Freezing drizzle",
            Self::Rain => "Rain",
            Self::FreezingRain => "Freezing rain",
            Self::Snow => "Snow",
            Self::SnowGrains => "Snow grains",
            Self::RainShowers => "Rain showers",
            Self::SnowShowers => "Snow showers",
            Self::Thunderstorm => "Thunderstorm",
            Self::ThunderstormWithHail => "Thunderstorm with hail",
            Self::Unknown => "Unknown",
        }
    }

    /// Get an emoji representation of the weather condition
    #[must_use]
    pub const fn emoji(&self) -> &'static str {
        match self {
            Self::ClearSky => "☀️",
            Self::MainlyClear => "🌤️",
            Self::PartlyCloudy => "⛅",
            Self::Overcast => "☁️",
            Self::Fog => "🌫️",
            Self::Drizzle | Self::Rain | Self::RainShowers => "🌧️",
            Self::FreezingDrizzle | Self::FreezingRain => "🌨️",
            Self::Snow | Self::SnowGrains | Self::SnowShowers => "❄️",
            Self::Thunderstorm | Self::ThunderstormWithHail => "⛈️",
            Self::Unknown => "❓",
        }
    }
}

impl std::fmt::Display for WeatherCondition {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.description())
    }
}

/// Current weather conditions
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CurrentWeather {
    /// Observation time
    pub time: DateTime<Utc>,
    /// Temperature in Celsius
    pub temperature: f32,
    /// Apparent (feels like) temperature in Celsius
    pub apparent_temperature: f32,
    /// Relative humidity percentage (0-100)
    pub humidity: u8,
    /// Weather condition
    pub condition: WeatherCondition,
    /// WMO weather code
    pub weather_code: u8,
    /// Wind speed in km/h
    pub wind_speed: f32,
    /// Wind direction in degrees (0-360)
    pub wind_direction: u16,
    /// Wind gusts in km/h
    pub wind_gusts: f32,
    /// Precipitation in mm
    pub precipitation: f32,
    /// Cloud cover percentage (0-100)
    pub cloud_cover: u8,
    /// Surface pressure in hPa
    pub pressure: f32,
    /// Visibility in meters
    pub visibility: Option<f32>,
}

impl CurrentWeather {
    /// Get a formatted summary of current conditions
    #[must_use]
    pub fn summary(&self) -> String {
        format!(
            "{} {} {:.1}°C (feels like {:.1}°C), humidity {}%, wind {:.1} km/h",
            self.condition.emoji(),
            self.condition.description(),
            self.temperature,
            self.apparent_temperature,
            self.humidity,
            self.wind_speed
        )
    }
}

/// Daily weather forecast
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DailyForecast {
    /// Forecast date
    pub date: NaiveDate,
    /// Dominant weather condition
    pub condition: WeatherCondition,
    /// WMO weather code
    pub weather_code: u8,
    /// Maximum temperature in Celsius
    pub temperature_max: f32,
    /// Minimum temperature in Celsius
    pub temperature_min: f32,
    /// Maximum apparent temperature in Celsius
    pub apparent_temperature_max: f32,
    /// Minimum apparent temperature in Celsius
    pub apparent_temperature_min: f32,
    /// Sunrise time (UTC)
    pub sunrise: DateTime<Utc>,
    /// Sunset time (UTC)
    pub sunset: DateTime<Utc>,
    /// Maximum UV index
    pub uv_index_max: f32,
    /// Total precipitation in mm
    pub precipitation_sum: f32,
    /// Precipitation probability percentage (0-100)
    pub precipitation_probability: Option<u8>,
    /// Total rain in mm
    pub rain_sum: f32,
    /// Total snowfall in cm
    pub snowfall_sum: f32,
    /// Maximum wind speed in km/h
    pub wind_speed_max: f32,
    /// Maximum wind gusts in km/h
    pub wind_gusts_max: f32,
    /// Dominant wind direction in degrees
    pub wind_direction_dominant: u16,
}

impl DailyForecast {
    /// Get a formatted summary of the daily forecast
    #[must_use]
    pub fn summary(&self) -> String {
        format!(
            "{} {} {:.0}°C/{:.0}°C, precip {:.1}mm, UV {:.1}",
            self.condition.emoji(),
            self.condition.description(),
            self.temperature_max,
            self.temperature_min,
            self.precipitation_sum,
            self.uv_index_max
        )
    }
}

/// Complete weather forecast
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Forecast {
    /// Current weather conditions
    pub current: CurrentWeather,
    /// Daily forecasts
    pub daily: Vec<DailyForecast>,
    /// Latitude of the location
    pub latitude: f64,
    /// Longitude of the location
    pub longitude: f64,
    /// Timezone of the location
    pub timezone: String,
    /// Timezone abbreviation
    pub timezone_abbreviation: String,
    /// Elevation in meters
    pub elevation: f32,
}

impl Forecast {
    /// Get today's forecast
    #[must_use]
    pub fn today(&self) -> Option<&DailyForecast> {
        self.daily.first()
    }

    /// Get tomorrow's forecast
    #[must_use]
    pub fn tomorrow(&self) -> Option<&DailyForecast> {
        self.daily.get(1)
    }

    /// Get the next N days of forecasts
    #[must_use]
    pub fn next_days(&self, n: usize) -> &[DailyForecast] {
        let end = n.min(self.daily.len());
        &self.daily[..end]
    }
}

/// Weather data units
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WeatherUnits {
    pub time: String,
    pub temperature_2m: String,
    pub relative_humidity_2m: String,
    pub apparent_temperature: String,
    pub weather_code: String,
    pub wind_speed_10m: String,
    pub wind_direction_10m: String,
    pub wind_gusts_10m: String,
    pub precipitation: String,
    pub cloud_cover: String,
    pub surface_pressure: String,
}

/// Raw weather data from API (current)
#[derive(Debug, Clone, Deserialize)]
pub struct WeatherData {
    pub time: String,
    pub temperature_2m: f32,
    pub relative_humidity_2m: u8,
    pub apparent_temperature: f32,
    pub weather_code: u8,
    pub wind_speed_10m: f32,
    pub wind_direction_10m: u16,
    pub wind_gusts_10m: f32,
    pub precipitation: f32,
    pub cloud_cover: u8,
    pub surface_pressure: f32,
    #[serde(default)]
    pub visibility: Option<f32>,
}

/// Raw daily data from API
#[derive(Debug, Clone, Deserialize)]
pub struct DailyData {
    pub time: Vec<String>,
    pub weather_code: Vec<u8>,
    pub temperature_2m_max: Vec<f32>,
    pub temperature_2m_min: Vec<f32>,
    pub apparent_temperature_max: Vec<f32>,
    pub apparent_temperature_min: Vec<f32>,
    pub sunrise: Vec<String>,
    pub sunset: Vec<String>,
    pub uv_index_max: Vec<f32>,
    pub precipitation_sum: Vec<f32>,
    #[serde(default)]
    pub precipitation_probability_max: Option<Vec<u8>>,
    pub rain_sum: Vec<f32>,
    pub snowfall_sum: Vec<f32>,
    pub wind_speed_10m_max: Vec<f32>,
    pub wind_gusts_10m_max: Vec<f32>,
    pub wind_direction_10m_dominant: Vec<u16>,
}

/// Raw API response
#[derive(Debug, Clone, Deserialize)]
pub struct ApiResponse {
    pub latitude: f64,
    pub longitude: f64,
    pub timezone: String,
    pub timezone_abbreviation: String,
    pub elevation: f32,
    pub current: Option<WeatherData>,
    pub daily: Option<DailyData>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_wmo_code_clear() {
        assert_eq!(
            WeatherCondition::from_wmo_code(0),
            WeatherCondition::ClearSky
        );
        assert_eq!(
            WeatherCondition::from_wmo_code(1),
            WeatherCondition::MainlyClear
        );
        assert_eq!(
            WeatherCondition::from_wmo_code(2),
            WeatherCondition::PartlyCloudy
        );
        assert_eq!(
            WeatherCondition::from_wmo_code(3),
            WeatherCondition::Overcast
        );
    }

    #[test]
    fn test_wmo_code_precipitation() {
        assert_eq!(
            WeatherCondition::from_wmo_code(51),
            WeatherCondition::Drizzle
        );
        assert_eq!(WeatherCondition::from_wmo_code(61), WeatherCondition::Rain);
        assert_eq!(WeatherCondition::from_wmo_code(71), WeatherCondition::Snow);
        assert_eq!(
            WeatherCondition::from_wmo_code(95),
            WeatherCondition::Thunderstorm
        );
    }

    #[test]
    fn test_wmo_code_fog() {
        assert_eq!(WeatherCondition::from_wmo_code(45), WeatherCondition::Fog);
        assert_eq!(WeatherCondition::from_wmo_code(48), WeatherCondition::Fog);
    }

    #[test]
    fn test_wmo_code_unknown() {
        assert_eq!(
            WeatherCondition::from_wmo_code(100),
            WeatherCondition::Unknown
        );
        assert_eq!(
            WeatherCondition::from_wmo_code(255),
            WeatherCondition::Unknown
        );
    }

    #[test]
    fn test_weather_condition_description() {
        assert_eq!(WeatherCondition::ClearSky.description(), "Clear sky");
        assert_eq!(WeatherCondition::Rain.description(), "Rain");
        assert_eq!(WeatherCondition::Thunderstorm.description(), "Thunderstorm");
    }

    #[test]
    fn test_weather_condition_emoji() {
        assert_eq!(WeatherCondition::ClearSky.emoji(), "☀️");
        assert_eq!(WeatherCondition::Rain.emoji(), "🌧️");
        assert_eq!(WeatherCondition::Snow.emoji(), "❄️");
        assert_eq!(WeatherCondition::Thunderstorm.emoji(), "⛈️");
    }

    #[test]
    fn test_weather_condition_display() {
        assert_eq!(format!("{}", WeatherCondition::ClearSky), "Clear sky");
        assert_eq!(
            format!("{}", WeatherCondition::PartlyCloudy),
            "Partly cloudy"
        );
    }

    #[test]
    fn test_current_weather_summary() {
        let weather = CurrentWeather {
            time: Utc::now(),
            temperature: 20.5,
            apparent_temperature: 19.0,
            humidity: 65,
            condition: WeatherCondition::PartlyCloudy,
            weather_code: 2,
            wind_speed: 15.0,
            wind_direction: 180,
            wind_gusts: 25.0,
            precipitation: 0.0,
            cloud_cover: 50,
            pressure: 1013.25,
            visibility: Some(10000.0),
        };

        let summary = weather.summary();
        assert!(summary.contains("⛅"));
        assert!(summary.contains("Partly cloudy"));
        assert!(summary.contains("20.5°C"));
        assert!(summary.contains("19.0°C"));
        assert!(summary.contains("65%"));
        assert!(summary.contains("15.0 km/h"));
    }

    #[test]
    fn test_daily_forecast_summary() {
        let forecast = DailyForecast {
            date: NaiveDate::from_ymd_opt(2026, 2, 5).expect("valid date"),
            condition: WeatherCondition::Rain,
            weather_code: 61,
            temperature_max: 15.0,
            temperature_min: 8.0,
            apparent_temperature_max: 14.0,
            apparent_temperature_min: 6.0,
            sunrise: Utc::now(),
            sunset: Utc::now(),
            uv_index_max: 3.5,
            precipitation_sum: 5.2,
            precipitation_probability: Some(80),
            rain_sum: 5.2,
            snowfall_sum: 0.0,
            wind_speed_max: 20.0,
            wind_gusts_max: 35.0,
            wind_direction_dominant: 270,
        };

        let summary = forecast.summary();
        assert!(summary.contains("🌧️"));
        assert!(summary.contains("Rain"));
        assert!(summary.contains("15°C/8°C"));
        assert!(summary.contains("5.2mm"));
        assert!(summary.contains("UV 3.5"));
    }

    #[test]
    fn test_forecast_today_tomorrow() {
        let today = DailyForecast {
            date: NaiveDate::from_ymd_opt(2026, 2, 5).expect("valid date"),
            condition: WeatherCondition::ClearSky,
            weather_code: 0,
            temperature_max: 10.0,
            temperature_min: 2.0,
            apparent_temperature_max: 9.0,
            apparent_temperature_min: 0.0,
            sunrise: Utc::now(),
            sunset: Utc::now(),
            uv_index_max: 2.0,
            precipitation_sum: 0.0,
            precipitation_probability: Some(10),
            rain_sum: 0.0,
            snowfall_sum: 0.0,
            wind_speed_max: 10.0,
            wind_gusts_max: 15.0,
            wind_direction_dominant: 90,
        };

        let tomorrow = DailyForecast {
            date: NaiveDate::from_ymd_opt(2026, 2, 6).expect("valid date"),
            ..today
        };

        let forecast = Forecast {
            current: CurrentWeather {
                time: Utc::now(),
                temperature: 5.0,
                apparent_temperature: 3.0,
                humidity: 80,
                condition: WeatherCondition::ClearSky,
                weather_code: 0,
                wind_speed: 10.0,
                wind_direction: 90,
                wind_gusts: 15.0,
                precipitation: 0.0,
                cloud_cover: 0,
                pressure: 1020.0,
                visibility: None,
            },
            daily: vec![today.clone(), tomorrow],
            latitude: 52.52,
            longitude: 13.41,
            timezone: "Europe/Berlin".to_string(),
            timezone_abbreviation: "CET".to_string(),
            elevation: 38.0,
        };

        assert!(forecast.today().is_some());
        assert_eq!(forecast.today().map(|d| d.date), Some(today.date));
        assert!(forecast.tomorrow().is_some());
        assert_eq!(forecast.next_days(1).len(), 1);
        assert_eq!(forecast.next_days(5).len(), 2);
    }

    // Additional tests for improved coverage

    #[test]
    fn test_wmo_code_drizzle_variants() {
        assert_eq!(
            WeatherCondition::from_wmo_code(51),
            WeatherCondition::Drizzle
        );
        assert_eq!(
            WeatherCondition::from_wmo_code(53),
            WeatherCondition::Drizzle
        );
        assert_eq!(
            WeatherCondition::from_wmo_code(55),
            WeatherCondition::Drizzle
        );
    }

    #[test]
    fn test_wmo_code_freezing_drizzle() {
        assert_eq!(
            WeatherCondition::from_wmo_code(56),
            WeatherCondition::FreezingDrizzle
        );
        assert_eq!(
            WeatherCondition::from_wmo_code(57),
            WeatherCondition::FreezingDrizzle
        );
    }

    #[test]
    fn test_wmo_code_rain_variants() {
        assert_eq!(WeatherCondition::from_wmo_code(61), WeatherCondition::Rain);
        assert_eq!(WeatherCondition::from_wmo_code(63), WeatherCondition::Rain);
        assert_eq!(WeatherCondition::from_wmo_code(65), WeatherCondition::Rain);
    }

    #[test]
    fn test_wmo_code_freezing_rain() {
        assert_eq!(
            WeatherCondition::from_wmo_code(66),
            WeatherCondition::FreezingRain
        );
        assert_eq!(
            WeatherCondition::from_wmo_code(67),
            WeatherCondition::FreezingRain
        );
    }

    #[test]
    fn test_wmo_code_snow_variants() {
        assert_eq!(WeatherCondition::from_wmo_code(71), WeatherCondition::Snow);
        assert_eq!(WeatherCondition::from_wmo_code(73), WeatherCondition::Snow);
        assert_eq!(WeatherCondition::from_wmo_code(75), WeatherCondition::Snow);
    }

    #[test]
    fn test_wmo_code_snow_grains() {
        assert_eq!(
            WeatherCondition::from_wmo_code(77),
            WeatherCondition::SnowGrains
        );
    }

    #[test]
    fn test_wmo_code_rain_showers() {
        assert_eq!(
            WeatherCondition::from_wmo_code(80),
            WeatherCondition::RainShowers
        );
        assert_eq!(
            WeatherCondition::from_wmo_code(81),
            WeatherCondition::RainShowers
        );
        assert_eq!(
            WeatherCondition::from_wmo_code(82),
            WeatherCondition::RainShowers
        );
    }

    #[test]
    fn test_wmo_code_snow_showers() {
        assert_eq!(
            WeatherCondition::from_wmo_code(85),
            WeatherCondition::SnowShowers
        );
        assert_eq!(
            WeatherCondition::from_wmo_code(86),
            WeatherCondition::SnowShowers
        );
    }

    #[test]
    fn test_wmo_code_thunderstorm_with_hail() {
        assert_eq!(
            WeatherCondition::from_wmo_code(96),
            WeatherCondition::ThunderstormWithHail
        );
        assert_eq!(
            WeatherCondition::from_wmo_code(99),
            WeatherCondition::ThunderstormWithHail
        );
    }

    #[test]
    fn test_all_weather_condition_descriptions() {
        assert_eq!(WeatherCondition::MainlyClear.description(), "Mainly clear");
        assert_eq!(WeatherCondition::Overcast.description(), "Overcast");
        assert_eq!(WeatherCondition::Fog.description(), "Fog");
        assert_eq!(WeatherCondition::Drizzle.description(), "Drizzle");
        assert_eq!(
            WeatherCondition::FreezingDrizzle.description(),
            "Freezing drizzle"
        );
        assert_eq!(
            WeatherCondition::FreezingRain.description(),
            "Freezing rain"
        );
        assert_eq!(WeatherCondition::SnowGrains.description(), "Snow grains");
        assert_eq!(WeatherCondition::RainShowers.description(), "Rain showers");
        assert_eq!(WeatherCondition::SnowShowers.description(), "Snow showers");
        assert_eq!(
            WeatherCondition::ThunderstormWithHail.description(),
            "Thunderstorm with hail"
        );
        assert_eq!(WeatherCondition::Unknown.description(), "Unknown");
    }

    #[test]
    fn test_all_weather_condition_emojis() {
        assert_eq!(WeatherCondition::MainlyClear.emoji(), "🌤️");
        assert_eq!(WeatherCondition::Overcast.emoji(), "☁️");
        assert_eq!(WeatherCondition::Fog.emoji(), "🌫️");
        assert_eq!(WeatherCondition::Drizzle.emoji(), "🌧️");
        assert_eq!(WeatherCondition::RainShowers.emoji(), "🌧️");
        assert_eq!(WeatherCondition::FreezingDrizzle.emoji(), "🌨️");
        assert_eq!(WeatherCondition::FreezingRain.emoji(), "🌨️");
        assert_eq!(WeatherCondition::SnowGrains.emoji(), "❄️");
        assert_eq!(WeatherCondition::SnowShowers.emoji(), "❄️");
        assert_eq!(WeatherCondition::ThunderstormWithHail.emoji(), "⛈️");
        assert_eq!(WeatherCondition::Unknown.emoji(), "❓");
    }

    #[test]
    fn test_weather_condition_serialization() {
        let condition = WeatherCondition::ClearSky;
        let json = serde_json::to_string(&condition).unwrap();
        assert_eq!(json, "\"clear_sky\"");

        let parsed: WeatherCondition = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, WeatherCondition::ClearSky);
    }

    #[test]
    fn test_weather_condition_copy() {
        let condition = WeatherCondition::Rain;
        let copied = condition; // WeatherCondition is Copy
        assert_eq!(condition, copied);
    }

    #[test]
    fn test_weather_condition_debug() {
        let condition = WeatherCondition::Thunderstorm;
        let debug = format!("{condition:?}");
        assert!(debug.contains("Thunderstorm"));
    }

    #[test]
    fn test_forecast_empty_daily() {
        let forecast = Forecast {
            current: CurrentWeather {
                time: Utc::now(),
                temperature: 15.0,
                apparent_temperature: 14.0,
                humidity: 50,
                condition: WeatherCondition::ClearSky,
                weather_code: 0,
                wind_speed: 5.0,
                wind_direction: 45,
                wind_gusts: 10.0,
                precipitation: 0.0,
                cloud_cover: 0,
                pressure: 1015.0,
                visibility: Some(20000.0),
            },
            daily: vec![],
            latitude: 48.85,
            longitude: 2.35,
            timezone: "Europe/Paris".to_string(),
            timezone_abbreviation: "CET".to_string(),
            elevation: 35.0,
        };

        assert!(forecast.today().is_none());
        assert!(forecast.tomorrow().is_none());
        assert_eq!(forecast.next_days(3).len(), 0);
    }

    #[test]
    fn test_next_days_boundary() {
        let temps = [10.0_f32, 11.0, 12.0];
        let daily = (0..3)
            .map(|i| DailyForecast {
                date: NaiveDate::from_ymd_opt(2026, 2, 5 + i).expect("valid date"),
                condition: WeatherCondition::ClearSky,
                weather_code: 0,
                temperature_max: temps[i as usize],
                temperature_min: 2.0,
                apparent_temperature_max: 9.0,
                apparent_temperature_min: 0.0,
                sunrise: Utc::now(),
                sunset: Utc::now(),
                uv_index_max: 2.0,
                precipitation_sum: 0.0,
                precipitation_probability: None,
                rain_sum: 0.0,
                snowfall_sum: 0.0,
                wind_speed_max: 10.0,
                wind_gusts_max: 15.0,
                wind_direction_dominant: 90,
            })
            .collect();

        let forecast = Forecast {
            current: CurrentWeather {
                time: Utc::now(),
                temperature: 5.0,
                apparent_temperature: 3.0,
                humidity: 80,
                condition: WeatherCondition::ClearSky,
                weather_code: 0,
                wind_speed: 10.0,
                wind_direction: 90,
                wind_gusts: 15.0,
                precipitation: 0.0,
                cloud_cover: 0,
                pressure: 1020.0,
                visibility: None,
            },
            daily,
            latitude: 52.52,
            longitude: 13.41,
            timezone: "Europe/Berlin".to_string(),
            timezone_abbreviation: "CET".to_string(),
            elevation: 38.0,
        };

        assert_eq!(forecast.next_days(0).len(), 0);
        assert_eq!(forecast.next_days(1).len(), 1);
        assert_eq!(forecast.next_days(3).len(), 3);
        assert_eq!(forecast.next_days(10).len(), 3); // Capped at actual length
    }

    #[test]
    fn test_current_weather_without_visibility() {
        let weather = CurrentWeather {
            time: Utc::now(),
            temperature: 10.0,
            apparent_temperature: 8.0,
            humidity: 70,
            condition: WeatherCondition::Fog,
            weather_code: 45,
            wind_speed: 5.0,
            wind_direction: 0,
            wind_gusts: 8.0,
            precipitation: 0.0,
            cloud_cover: 100,
            pressure: 1000.0,
            visibility: None,
        };

        assert!(weather.visibility.is_none());
        let summary = weather.summary();
        assert!(summary.contains("Fog"));
    }

    #[test]
    fn test_daily_forecast_without_precipitation_probability() {
        let forecast = DailyForecast {
            date: NaiveDate::from_ymd_opt(2026, 3, 1).expect("valid date"),
            condition: WeatherCondition::Snow,
            weather_code: 71,
            temperature_max: 0.0,
            temperature_min: -5.0,
            apparent_temperature_max: -2.0,
            apparent_temperature_min: -10.0,
            sunrise: Utc::now(),
            sunset: Utc::now(),
            uv_index_max: 1.0,
            precipitation_sum: 10.0,
            precipitation_probability: None,
            rain_sum: 0.0,
            snowfall_sum: 8.0,
            wind_speed_max: 30.0,
            wind_gusts_max: 50.0,
            wind_direction_dominant: 180,
        };

        assert!(forecast.precipitation_probability.is_none());
        let summary = forecast.summary();
        assert!(summary.contains("Snow"));
        assert!(summary.contains("0°C/-5°C"));
    }

    #[test]
    fn test_current_weather_clone() {
        let weather = CurrentWeather {
            time: Utc::now(),
            temperature: 20.0,
            apparent_temperature: 18.0,
            humidity: 50,
            condition: WeatherCondition::ClearSky,
            weather_code: 0,
            wind_speed: 10.0,
            wind_direction: 90,
            wind_gusts: 15.0,
            precipitation: 0.0,
            cloud_cover: 10,
            pressure: 1015.0,
            visibility: Some(15000.0),
        };

        let cloned = weather.clone();
        assert!((weather.temperature - cloned.temperature).abs() < f32::EPSILON);
        assert_eq!(weather.condition, cloned.condition);
    }

    #[test]
    fn test_daily_forecast_clone() {
        let forecast = DailyForecast {
            date: NaiveDate::from_ymd_opt(2026, 4, 15).expect("valid date"),
            condition: WeatherCondition::MainlyClear,
            weather_code: 1,
            temperature_max: 22.0,
            temperature_min: 12.0,
            apparent_temperature_max: 21.0,
            apparent_temperature_min: 11.0,
            sunrise: Utc::now(),
            sunset: Utc::now(),
            uv_index_max: 5.0,
            precipitation_sum: 0.0,
            precipitation_probability: Some(5),
            rain_sum: 0.0,
            snowfall_sum: 0.0,
            wind_speed_max: 15.0,
            wind_gusts_max: 25.0,
            wind_direction_dominant: 270,
        };

        let cloned = forecast.clone();
        assert_eq!(forecast.date, cloned.date);
        assert_eq!(forecast.condition, cloned.condition);
    }

    #[test]
    fn test_forecast_clone() {
        let forecast = Forecast {
            current: CurrentWeather {
                time: Utc::now(),
                temperature: 15.0,
                apparent_temperature: 14.0,
                humidity: 60,
                condition: WeatherCondition::PartlyCloudy,
                weather_code: 2,
                wind_speed: 12.0,
                wind_direction: 135,
                wind_gusts: 20.0,
                precipitation: 0.0,
                cloud_cover: 40,
                pressure: 1012.0,
                visibility: Some(25000.0),
            },
            daily: vec![],
            latitude: 51.5,
            longitude: -0.1,
            timezone: "Europe/London".to_string(),
            timezone_abbreviation: "GMT".to_string(),
            elevation: 11.0,
        };

        let cloned = forecast.clone();
        assert!((forecast.latitude - cloned.latitude).abs() < f64::EPSILON);
        assert_eq!(forecast.timezone, cloned.timezone);
    }
}
