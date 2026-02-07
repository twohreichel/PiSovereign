//! Weather adapter - Implements WeatherPort using integration_weather

use application::error::ApplicationError;
use application::ports::{CurrentWeather, DailyForecast, WeatherCondition, WeatherPort};
use async_trait::async_trait;
use domain::value_objects::GeoLocation;
use integration_weather::{
    CurrentWeather as IntegrationCurrent, DailyForecast as IntegrationDaily, OpenMeteoClient,
    WeatherClient, WeatherCondition as IntegrationCondition, WeatherConfig, WeatherError,
};
use tracing::{debug, instrument};

use super::{CircuitBreaker, CircuitBreakerConfig};

/// Adapter for weather services using Open-Meteo API
pub struct WeatherAdapter {
    client: OpenMeteoClient,
    circuit_breaker: Option<CircuitBreaker>,
}

impl std::fmt::Debug for WeatherAdapter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WeatherAdapter")
            .field("client", &"OpenMeteoClient")
            .field(
                "circuit_breaker",
                &self.circuit_breaker.as_ref().map(CircuitBreaker::name),
            )
            .finish()
    }
}

impl WeatherAdapter {
    /// Create a new adapter with default configuration
    ///
    /// # Errors
    ///
    /// Returns an error if the HTTP client fails to initialize.
    pub fn new() -> Result<Self, ApplicationError> {
        let client = OpenMeteoClient::with_defaults()
            .map_err(|e| ApplicationError::Internal(e.to_string()))?;
        Ok(Self {
            client,
            circuit_breaker: None,
        })
    }

    /// Create with custom configuration
    ///
    /// # Errors
    ///
    /// Returns an error if the HTTP client fails to initialize.
    pub fn with_config(config: WeatherConfig) -> Result<Self, ApplicationError> {
        let client =
            OpenMeteoClient::new(config).map_err(|e| ApplicationError::Internal(e.to_string()))?;
        Ok(Self {
            client,
            circuit_breaker: None,
        })
    }

    /// Enable circuit breaker with default configuration
    #[must_use]
    pub fn with_circuit_breaker(mut self) -> Self {
        self.circuit_breaker = Some(CircuitBreaker::new("weather"));
        self
    }

    /// Enable circuit breaker with custom configuration
    #[must_use]
    pub fn with_circuit_breaker_config(mut self, config: CircuitBreakerConfig) -> Self {
        self.circuit_breaker = Some(CircuitBreaker::with_config("weather", config));
        self
    }

    /// Check circuit and return error if open
    fn check_circuit(&self) -> Result<(), ApplicationError> {
        if let Some(ref cb) = self.circuit_breaker {
            if cb.is_open() {
                return Err(ApplicationError::ExternalService(
                    "Weather service circuit breaker is open".into(),
                ));
            }
        }
        Ok(())
    }

    /// Map integration weather error to application error
    fn map_error(err: WeatherError) -> ApplicationError {
        match err {
            WeatherError::ConnectionFailed(e) | WeatherError::RequestFailed(e) => {
                ApplicationError::ExternalService(e)
            },
            WeatherError::ParseError(e) | WeatherError::ServiceUnavailable(e) => {
                ApplicationError::Internal(e)
            },
            WeatherError::InvalidCoordinates => {
                ApplicationError::InvalidOperation("Invalid coordinates".into())
            },
            WeatherError::RateLimitExceeded => ApplicationError::RateLimited,
        }
    }

    /// Convert integration weather condition to application weather condition
    const fn map_condition(condition: IntegrationCondition) -> WeatherCondition {
        match condition {
            IntegrationCondition::ClearSky => WeatherCondition::ClearSky,
            IntegrationCondition::MainlyClear => WeatherCondition::MainlyClear,
            IntegrationCondition::PartlyCloudy => WeatherCondition::PartlyCloudy,
            IntegrationCondition::Overcast => WeatherCondition::Overcast,
            IntegrationCondition::Fog => WeatherCondition::Fog,
            IntegrationCondition::Drizzle | IntegrationCondition::FreezingDrizzle => {
                WeatherCondition::Drizzle
            },
            IntegrationCondition::Rain | IntegrationCondition::RainShowers => {
                WeatherCondition::ModerateRain
            },
            IntegrationCondition::FreezingRain => WeatherCondition::HeavyRain,
            IntegrationCondition::Snow
            | IntegrationCondition::SnowGrains
            | IntegrationCondition::SnowShowers => WeatherCondition::Snow,
            IntegrationCondition::Thunderstorm | IntegrationCondition::ThunderstormWithHail => {
                WeatherCondition::Thunderstorm
            },
            IntegrationCondition::Unknown => WeatherCondition::Unknown,
        }
    }

    /// Convert integration current weather to application current weather
    fn map_current(current: &IntegrationCurrent) -> CurrentWeather {
        CurrentWeather {
            temperature: f64::from(current.temperature),
            apparent_temperature: f64::from(current.apparent_temperature),
            humidity: current.humidity,
            wind_speed: f64::from(current.wind_speed),
            condition: Self::map_condition(current.condition),
            observed_at: current.time,
        }
    }

    /// Convert integration daily forecast to application daily forecast
    fn map_daily(daily: &IntegrationDaily) -> DailyForecast {
        DailyForecast {
            date: daily.date,
            temperature_max: f64::from(daily.temperature_max),
            temperature_min: f64::from(daily.temperature_min),
            condition: Self::map_condition(daily.condition),
            precipitation_probability: daily.precipitation_probability.unwrap_or(0),
            precipitation_sum: f64::from(daily.precipitation_sum),
            sunrise: Some(daily.sunrise),
            sunset: Some(daily.sunset),
        }
    }
}

#[async_trait]
impl WeatherPort for WeatherAdapter {
    #[instrument(skip(self), fields(lat = location.latitude(), lon = location.longitude()))]
    async fn get_current_weather(
        &self,
        location: &GeoLocation,
    ) -> Result<CurrentWeather, ApplicationError> {
        self.check_circuit()?;

        let result = self
            .client
            .get_current(location.latitude(), location.longitude())
            .await
            .map_err(Self::map_error);

        match &result {
            Ok(current) => {
                debug!(
                    temperature = current.temperature,
                    condition = %current.condition,
                    "Retrieved current weather"
                );
            },
            Err(e) => {
                debug!(error = %e, "Failed to get current weather");
            },
        }

        result.map(|c| Self::map_current(&c))
    }

    #[instrument(skip(self), fields(lat = location.latitude(), lon = location.longitude(), days))]
    async fn get_forecast(
        &self,
        location: &GeoLocation,
        days: u8,
    ) -> Result<Vec<DailyForecast>, ApplicationError> {
        self.check_circuit()?;

        let result = self
            .client
            .get_forecast(location.latitude(), location.longitude(), days)
            .await
            .map_err(Self::map_error);

        match &result {
            Ok(forecast) => {
                debug!(days = forecast.daily.len(), "Retrieved weather forecast");
            },
            Err(e) => {
                debug!(error = %e, "Failed to get weather forecast");
            },
        }

        result.map(|f| f.daily.iter().map(Self::map_daily).collect())
    }

    #[instrument(skip(self))]
    async fn is_available(&self) -> bool {
        // Check circuit breaker first
        if let Some(ref cb) = self.circuit_breaker {
            if cb.is_open() {
                return false;
            }
            // Use circuit breaker to wrap the health check
            let result = cb
                .call(|| async { self.client.get_current(52.52, 13.405).await })
                .await;
            return result.is_ok();
        }

        // No circuit breaker, just check directly
        // Try a lightweight health check - use Berlin coordinates as a reference point
        self.client.get_current(52.52, 13.405).await.is_ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_creates_adapter() {
        let adapter = WeatherAdapter::new();
        assert!(adapter.is_ok());
        assert!(adapter.unwrap().circuit_breaker.is_none());
    }

    #[test]
    fn with_circuit_breaker() {
        let adapter = WeatherAdapter::new().unwrap().with_circuit_breaker();
        assert!(adapter.circuit_breaker.is_some());
    }

    #[test]
    fn debug_impl() {
        let adapter = WeatherAdapter::new().unwrap();
        let debug_str = format!("{adapter:?}");
        assert!(debug_str.contains("WeatherAdapter"));
    }

    #[test]
    fn map_condition_clear() {
        assert_eq!(
            WeatherAdapter::map_condition(IntegrationCondition::ClearSky),
            WeatherCondition::ClearSky
        );
    }

    #[test]
    fn map_condition_rain() {
        assert_eq!(
            WeatherAdapter::map_condition(IntegrationCondition::Rain),
            WeatherCondition::ModerateRain
        );
    }

    #[test]
    fn map_condition_thunderstorm() {
        assert_eq!(
            WeatherAdapter::map_condition(IntegrationCondition::Thunderstorm),
            WeatherCondition::Thunderstorm
        );
    }

    #[test]
    fn map_condition_unknown() {
        assert_eq!(
            WeatherAdapter::map_condition(IntegrationCondition::Unknown),
            WeatherCondition::Unknown
        );
    }

    #[test]
    fn map_error_connection_failed() {
        let err = WeatherError::ConnectionFailed("timeout".into());
        let app_err = WeatherAdapter::map_error(err);
        assert!(matches!(app_err, ApplicationError::ExternalService(_)));
    }

    #[test]
    fn map_error_rate_limited() {
        let err = WeatherError::RateLimitExceeded;
        let app_err = WeatherAdapter::map_error(err);
        assert!(matches!(app_err, ApplicationError::RateLimited));
    }

    #[test]
    fn map_error_invalid_coords() {
        let err = WeatherError::InvalidCoordinates;
        let app_err = WeatherAdapter::map_error(err);
        assert!(matches!(app_err, ApplicationError::InvalidOperation(_)));
    }

    #[test]
    fn trait_is_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<WeatherAdapter>();
    }
}
