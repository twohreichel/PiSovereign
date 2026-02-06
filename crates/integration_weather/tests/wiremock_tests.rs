//! Integration tests for weather service using wiremock
//!
//! These tests verify the weather client's behavior against a mock HTTP server,
//! ensuring proper handling of various response scenarios.

use integration_weather::{OpenMeteoClient, WeatherClient, WeatherConfig, WeatherError};
use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{method, path, query_param},
};

/// Sample Open-Meteo API response for testing
fn sample_weather_response() -> serde_json::Value {
    serde_json::json!({
        "latitude": 52.52,
        "longitude": 13.405,
        "generationtime_ms": 0.123,
        "utc_offset_seconds": 3600,
        "timezone": "Europe/Berlin",
        "timezone_abbreviation": "CET",
        "elevation": 38.0,
        "current_units": {
            "time": "iso8601",
            "temperature_2m": "°C",
            "relative_humidity_2m": "%",
            "apparent_temperature": "°C",
            "weather_code": "wmo code",
            "wind_speed_10m": "km/h",
            "wind_direction_10m": "°",
            "wind_gusts_10m": "km/h",
            "precipitation": "mm",
            "cloud_cover": "%",
            "surface_pressure": "hPa"
        },
        "current": {
            "time": "2024-01-15T12:00",
            "temperature_2m": 5.5,
            "relative_humidity_2m": 75,
            "apparent_temperature": 2.0,
            "weather_code": 3,
            "wind_speed_10m": 12.5,
            "wind_direction_10m": 225,
            "wind_gusts_10m": 25.0,
            "precipitation": 0.0,
            "cloud_cover": 80,
            "surface_pressure": 1013.25
        },
        "daily_units": {
            "time": "iso8601",
            "weather_code": "wmo code",
            "temperature_2m_max": "°C",
            "temperature_2m_min": "°C",
            "apparent_temperature_max": "°C",
            "apparent_temperature_min": "°C",
            "sunrise": "iso8601",
            "sunset": "iso8601",
            "uv_index_max": "",
            "precipitation_sum": "mm",
            "precipitation_probability_max": "%",
            "rain_sum": "mm",
            "snowfall_sum": "cm",
            "wind_speed_10m_max": "km/h",
            "wind_gusts_10m_max": "km/h",
            "wind_direction_10m_dominant": "°"
        },
        "daily": {
            "time": ["2024-01-15", "2024-01-16", "2024-01-17"],
            "weather_code": [3, 61, 2],
            "temperature_2m_max": [8.0, 6.0, 10.0],
            "temperature_2m_min": [2.0, 1.0, 3.0],
            "apparent_temperature_max": [5.0, 3.0, 7.0],
            "apparent_temperature_min": [-1.0, -2.0, 0.0],
            "sunrise": ["2024-01-15T07:15", "2024-01-16T07:14", "2024-01-17T07:13"],
            "sunset": ["2024-01-15T16:30", "2024-01-16T16:32", "2024-01-17T16:34"],
            "uv_index_max": [1.0, 0.5, 2.0],
            "precipitation_sum": [0.0, 5.5, 0.0],
            "precipitation_probability_max": [10, 80, 5],
            "rain_sum": [0.0, 5.5, 0.0],
            "snowfall_sum": [0.0, 0.0, 0.0],
            "wind_speed_10m_max": [15.0, 20.0, 12.0],
            "wind_gusts_10m_max": [30.0, 40.0, 25.0],
            "wind_direction_10m_dominant": [225, 270, 180]
        }
    })
}

/// Create a test client configured to use the mock server
///
/// # Panics
///
/// Panics if the client cannot be created (should not happen in tests).
fn create_test_client(mock_server: &MockServer) -> OpenMeteoClient {
    let config = WeatherConfig {
        base_url: mock_server.uri(),
        timeout_secs: 5,
        ..Default::default()
    };
    #[allow(clippy::expect_used)]
    OpenMeteoClient::new(config).expect("Failed to create client")
}

/// Setup a mock for the /forecast endpoint with the given response
async fn setup_forecast_mock(mock_server: &MockServer, response: ResponseTemplate) {
    Mock::given(method("GET"))
        .and(path("/forecast"))
        .respond_with(response)
        .mount(mock_server)
        .await;
}

// ============================================================================
// Success scenarios
// ============================================================================

#[tokio::test]
async fn test_get_current_weather_success() {
    let mock_server = MockServer::start().await;

    setup_forecast_mock(
        &mock_server,
        ResponseTemplate::new(200).set_body_json(sample_weather_response()),
    )
    .await;

    let client = create_test_client(&mock_server);
    let result = client.get_current(52.52, 13.405).await;

    assert!(result.is_ok(), "Expected success, got: {result:?}");

    let weather = result.unwrap();
    assert!((weather.temperature - 5.5).abs() < 0.1);
    assert_eq!(weather.humidity, 75);
    assert!((weather.wind_speed - 12.5).abs() < 0.1);
}

#[tokio::test]
async fn test_get_forecast_success() {
    let mock_server = MockServer::start().await;

    setup_forecast_mock(
        &mock_server,
        ResponseTemplate::new(200).set_body_json(sample_weather_response()),
    )
    .await;

    let client = create_test_client(&mock_server);
    let result = client.get_forecast(52.52, 13.405, 3).await;

    assert!(result.is_ok(), "Expected success, got: {result:?}");

    let forecast = result.unwrap();
    assert_eq!(forecast.daily.len(), 3);
    assert!((forecast.daily[0].temperature_max - 8.0).abs() < 0.1);
    assert!((forecast.daily[0].temperature_min - 2.0).abs() < 0.1);
}

#[tokio::test]
async fn test_health_check_success() {
    let mock_server = MockServer::start().await;

    setup_forecast_mock(
        &mock_server,
        ResponseTemplate::new(200).set_body_json(sample_weather_response()),
    )
    .await;

    let client = create_test_client(&mock_server);
    let is_healthy = client.is_healthy().await;

    assert!(is_healthy, "Expected health check to succeed");
}

// ============================================================================
// Error handling scenarios
// ============================================================================

#[tokio::test]
async fn test_server_error_returns_service_unavailable() {
    let mock_server = MockServer::start().await;

    setup_forecast_mock(
        &mock_server,
        ResponseTemplate::new(500).set_body_string("Internal Server Error"),
    )
    .await;

    let client = create_test_client(&mock_server);
    let result = client.get_current(52.52, 13.405).await;

    assert!(result.is_err());
    assert!(
        matches!(result, Err(WeatherError::ServiceUnavailable(_))),
        "Expected ServiceUnavailable, got: {result:?}"
    );
}

#[tokio::test]
async fn test_rate_limit_error() {
    let mock_server = MockServer::start().await;

    setup_forecast_mock(
        &mock_server,
        ResponseTemplate::new(429).set_body_string("Rate limit exceeded"),
    )
    .await;

    let client = create_test_client(&mock_server);
    let result = client.get_current(52.52, 13.405).await;

    assert!(result.is_err());
    assert!(
        matches!(result, Err(WeatherError::RateLimitExceeded)),
        "Expected RateLimitExceeded, got: {result:?}"
    );
}

#[tokio::test]
async fn test_invalid_json_response() {
    let mock_server = MockServer::start().await;

    setup_forecast_mock(
        &mock_server,
        ResponseTemplate::new(200).set_body_string("not valid json"),
    )
    .await;

    let client = create_test_client(&mock_server);
    let result = client.get_current(52.52, 13.405).await;

    assert!(result.is_err());
    assert!(
        matches!(result, Err(WeatherError::ParseError(_))),
        "Expected ParseError, got: {result:?}"
    );
}

#[tokio::test]
async fn test_health_check_fails_on_server_error() {
    let mock_server = MockServer::start().await;

    setup_forecast_mock(
        &mock_server,
        ResponseTemplate::new(500).set_body_string("Internal Server Error"),
    )
    .await;

    let client = create_test_client(&mock_server);
    let is_healthy = client.is_healthy().await;

    assert!(!is_healthy, "Expected health check to fail");
}

// ============================================================================
// Input validation scenarios
// ============================================================================

#[tokio::test]
async fn test_invalid_coordinates_latitude() {
    let mock_server = MockServer::start().await;

    // No need to setup mock - validation should fail before request
    let client = create_test_client(&mock_server);
    let result = client.get_current(91.0, 13.405).await;

    assert!(result.is_err());
    assert!(
        matches!(result, Err(WeatherError::InvalidCoordinates)),
        "Expected InvalidCoordinates, got: {result:?}"
    );
}

#[tokio::test]
async fn test_invalid_coordinates_longitude() {
    let mock_server = MockServer::start().await;

    let client = create_test_client(&mock_server);
    let result = client.get_current(52.52, 181.0).await;

    assert!(result.is_err());
    assert!(
        matches!(result, Err(WeatherError::InvalidCoordinates)),
        "Expected InvalidCoordinates, got: {result:?}"
    );
}

// ============================================================================
// Query parameter verification
// ============================================================================

#[tokio::test]
async fn test_request_contains_correct_query_params() {
    let mock_server = MockServer::start().await;

    // Verify specific query parameters are sent
    Mock::given(method("GET"))
        .and(path("/forecast"))
        .and(query_param("latitude", "52.52"))
        .and(query_param("longitude", "13.405"))
        .and(query_param("timezone", "auto"))
        .respond_with(ResponseTemplate::new(200).set_body_json(sample_weather_response()))
        .expect(1)
        .mount(&mock_server)
        .await;

    let client = create_test_client(&mock_server);
    let result = client.get_current(52.52, 13.405).await;

    assert!(result.is_ok(), "Expected success, got: {result:?}");
}

#[tokio::test]
async fn test_forecast_days_parameter() {
    let mock_server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/forecast"))
        .and(query_param("forecast_days", "5"))
        .respond_with(ResponseTemplate::new(200).set_body_json(sample_weather_response()))
        .expect(1)
        .mount(&mock_server)
        .await;

    let client = create_test_client(&mock_server);
    let result = client.get_forecast(52.52, 13.405, 5).await;

    assert!(result.is_ok(), "Expected success, got: {result:?}");
}
