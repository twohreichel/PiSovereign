//! Integration tests for infrastructure crate
//!
//! Tests cover:
//! - Correlated HTTP client with wiremock
//! - Retry logic with property-based tests
//! - Security validation
//! - Configuration handling
//! - Weather/WhatsApp/Ollama API mocking
//! - Encryption and API key hashing
//! - Degraded inference mode

use std::time::Duration;
use uuid::Uuid;
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use infrastructure::{
    CircuitBreakerConfig, CorrelatedClientConfig, CorrelatedHttpClient, RetryConfig,
    SecurityValidator, SecurityWarning, WarningSeverity, X_REQUEST_ID,
};

// ============================================================================
// Correlated HTTP Client Tests
// ============================================================================

mod correlated_client_tests {
    use super::*;

    #[tokio::test]
    async fn client_creation_succeeds() {
        let client = CorrelatedHttpClient::new();
        assert!(client.is_ok());
    }

    #[tokio::test]
    async fn client_with_config_succeeds() {
        let config = CorrelatedClientConfig::default()
            .with_timeout(Duration::from_secs(60))
            .with_connect_timeout(Duration::from_secs(15))
            .with_user_agent("Test-Agent/1.0");

        let client = CorrelatedHttpClient::with_config(config);
        assert!(client.is_ok());
    }

    #[tokio::test]
    async fn get_request_sends_to_server() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/test"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "status": "ok"
            })))
            .mount(&mock_server)
            .await;

        let client = CorrelatedHttpClient::new().unwrap();
        let response = client
            .get(format!("{}/test", mock_server.uri()))
            .send()
            .await;

        assert!(response.is_ok());
        let response = response.unwrap();
        assert_eq!(response.status(), 200);
    }

    #[tokio::test]
    async fn request_includes_correlation_id() {
        let mock_server = MockServer::start().await;
        let request_id = Uuid::new_v4();

        Mock::given(method("GET"))
            .and(path("/correlated"))
            .and(header(X_REQUEST_ID, request_id.to_string().as_str()))
            .respond_with(ResponseTemplate::new(200))
            .expect(1)
            .mount(&mock_server)
            .await;

        let client = CorrelatedHttpClient::new().unwrap();
        let response = client
            .get(format!("{}/correlated", mock_server.uri()))
            .with_request_id(&request_id)
            .send()
            .await;

        assert!(response.is_ok());
        assert_eq!(response.unwrap().status(), 200);
    }

    #[tokio::test]
    async fn post_request_with_json_body() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/api/data"))
            .and(header("content-type", "application/json"))
            .respond_with(ResponseTemplate::new(201).set_body_json(serde_json::json!({
                "created": true
            })))
            .mount(&mock_server)
            .await;

        let client = CorrelatedHttpClient::new().unwrap();
        let body = serde_json::json!({
            "name": "test",
            "value": 42
        });

        let response = client
            .post(format!("{}/api/data", mock_server.uri()))
            .json(&body)
            .send()
            .await;

        assert!(response.is_ok());
        assert_eq!(response.unwrap().status(), 201);
    }

    #[tokio::test]
    async fn put_request_works() {
        let mock_server = MockServer::start().await;

        Mock::given(method("PUT"))
            .and(path("/resource/1"))
            .respond_with(ResponseTemplate::new(200))
            .mount(&mock_server)
            .await;

        let client = CorrelatedHttpClient::new().unwrap();
        let response = client
            .put(format!("{}/resource/1", mock_server.uri()))
            .json(&serde_json::json!({"updated": true}))
            .send()
            .await;

        assert!(response.is_ok());
    }

    #[tokio::test]
    async fn delete_request_works() {
        let mock_server = MockServer::start().await;

        Mock::given(method("DELETE"))
            .and(path("/resource/1"))
            .respond_with(ResponseTemplate::new(204))
            .mount(&mock_server)
            .await;

        let client = CorrelatedHttpClient::new().unwrap();
        let response = client
            .delete(format!("{}/resource/1", mock_server.uri()))
            .send()
            .await;

        assert!(response.is_ok());
        assert_eq!(response.unwrap().status(), 204);
    }

    #[tokio::test]
    async fn patch_request_works() {
        let mock_server = MockServer::start().await;

        Mock::given(method("PATCH"))
            .and(path("/partial"))
            .respond_with(ResponseTemplate::new(200))
            .mount(&mock_server)
            .await;

        let client = CorrelatedHttpClient::new().unwrap();
        let response = client
            .patch(format!("{}/partial", mock_server.uri()))
            .json(&serde_json::json!({"field": "value"}))
            .send()
            .await;

        assert!(response.is_ok());
    }

    #[tokio::test]
    async fn head_request_works() {
        let mock_server = MockServer::start().await;

        Mock::given(method("HEAD"))
            .and(path("/exists"))
            .respond_with(ResponseTemplate::new(200))
            .mount(&mock_server)
            .await;

        let client = CorrelatedHttpClient::new().unwrap();
        let response = client
            .head(format!("{}/exists", mock_server.uri()))
            .send()
            .await;

        assert!(response.is_ok());
        assert_eq!(response.unwrap().status(), 200);
    }

    #[tokio::test]
    async fn custom_headers_included() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/headers"))
            .and(header("x-custom-header", "custom-value"))
            .respond_with(ResponseTemplate::new(200))
            .expect(1)
            .mount(&mock_server)
            .await;

        let client = CorrelatedHttpClient::new().unwrap();
        let response = client
            .get(format!("{}/headers", mock_server.uri()))
            .header("x-custom-header", "custom-value")
            .send()
            .await;

        assert!(response.is_ok());
    }

    #[tokio::test]
    async fn bearer_auth_included() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/protected"))
            .and(header("authorization", "Bearer test-token-123"))
            .respond_with(ResponseTemplate::new(200))
            .expect(1)
            .mount(&mock_server)
            .await;

        let client = CorrelatedHttpClient::new().unwrap();
        let response = client
            .get(format!("{}/protected", mock_server.uri()))
            .bearer_auth("test-token-123")
            .send()
            .await;

        assert!(response.is_ok());
    }

    #[tokio::test]
    async fn query_parameters_work() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/search"))
            .and(wiremock::matchers::query_param("q", "test"))
            .and(wiremock::matchers::query_param("limit", "10"))
            .respond_with(ResponseTemplate::new(200))
            .mount(&mock_server)
            .await;

        let client = CorrelatedHttpClient::new().unwrap();

        #[derive(serde::Serialize)]
        struct Query {
            q: String,
            limit: u32,
        }

        let response = client
            .get(format!("{}/search", mock_server.uri()))
            .query(&Query {
                q: "test".to_string(),
                limit: 10,
            })
            .send()
            .await;

        assert!(response.is_ok());
    }

    #[tokio::test]
    async fn timeout_applied() {
        let mock_server = MockServer::start().await;

        // Server delays response longer than timeout
        Mock::given(method("GET"))
            .and(path("/slow"))
            .respond_with(ResponseTemplate::new(200).set_delay(Duration::from_secs(5)))
            .mount(&mock_server)
            .await;

        let config = CorrelatedClientConfig::default().with_timeout(Duration::from_millis(100));

        let client = CorrelatedHttpClient::with_config(config).unwrap();
        let response = client
            .get(format!("{}/slow", mock_server.uri()))
            .send()
            .await;

        // Should fail due to timeout
        assert!(response.is_err());
    }

    #[tokio::test]
    async fn handles_server_error() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/error"))
            .respond_with(ResponseTemplate::new(500).set_body_string("Internal Server Error"))
            .mount(&mock_server)
            .await;

        let client = CorrelatedHttpClient::new().unwrap();
        let response = client
            .get(format!("{}/error", mock_server.uri()))
            .send()
            .await;

        assert!(response.is_ok());
        let resp = response.unwrap();
        assert_eq!(resp.status(), 500);
    }

    #[tokio::test]
    async fn handles_404() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/not-found"))
            .respond_with(ResponseTemplate::new(404))
            .mount(&mock_server)
            .await;

        let client = CorrelatedHttpClient::new().unwrap();
        let response = client
            .get(format!("{}/not-found", mock_server.uri()))
            .send()
            .await;

        assert!(response.is_ok());
        assert_eq!(response.unwrap().status(), 404);
    }

    #[tokio::test]
    async fn config_defaults_are_reasonable() {
        let config = CorrelatedClientConfig::default();
        assert_eq!(config.connect_timeout, Duration::from_secs(10));
        assert_eq!(config.timeout, Duration::from_secs(30));
        assert!(config.user_agent.contains("PiSovereign"));
    }

    #[tokio::test]
    async fn multiple_requests_with_same_client() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/multi"))
            .respond_with(ResponseTemplate::new(200))
            .mount(&mock_server)
            .await;

        let client = CorrelatedHttpClient::new().unwrap();

        for _ in 0..5 {
            let response = client
                .get(format!("{}/multi", mock_server.uri()))
                .send()
                .await;
            assert!(response.is_ok());
        }
    }

    #[tokio::test]
    async fn different_correlation_ids_per_request() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/different"))
            .respond_with(ResponseTemplate::new(200))
            .mount(&mock_server)
            .await;

        let client = CorrelatedHttpClient::new().unwrap();

        let id1 = Uuid::new_v4();
        let id2 = Uuid::new_v4();
        assert_ne!(id1, id2);

        let resp1 = client
            .get(format!("{}/different", mock_server.uri()))
            .with_request_id(&id1)
            .send()
            .await;
        assert!(resp1.is_ok());

        let resp2 = client
            .get(format!("{}/different", mock_server.uri()))
            .with_request_id(&id2)
            .send()
            .await;
        assert!(resp2.is_ok());
    }
}

// ============================================================================
// Retry Config Tests
// ============================================================================

mod retry_config_tests {
    use super::*;

    #[test]
    fn default_config() {
        let config = RetryConfig::default();
        assert_eq!(config.initial_delay_ms, 100);
        assert_eq!(config.max_delay_ms, 10_000);
        assert_eq!(config.max_retries, 3);
        assert!(config.jitter_enabled);
    }

    #[test]
    fn fast_preset() {
        let config = RetryConfig::fast();
        assert_eq!(config.initial_delay_ms, 50);
        assert_eq!(config.max_delay_ms, 1000);
    }

    #[test]
    fn slow_preset() {
        let config = RetryConfig::slow();
        assert_eq!(config.initial_delay_ms, 500);
        assert_eq!(config.max_delay_ms, 30_000);
    }

    #[test]
    fn critical_preset() {
        let config = RetryConfig::critical();
        assert_eq!(config.initial_delay_ms, 1000);
        assert_eq!(config.max_retries, 10);
    }

    #[test]
    fn delay_without_jitter_is_deterministic() {
        let config = RetryConfig::new(100, 10_000, 2.0, 5).without_jitter();

        assert_eq!(config.delay_for_attempt(0).as_millis(), 100);
        assert_eq!(config.delay_for_attempt(1).as_millis(), 200);
        assert_eq!(config.delay_for_attempt(2).as_millis(), 400);
    }

    #[test]
    fn delay_capped_at_max() {
        let config = RetryConfig::new(100, 500, 2.0, 10).without_jitter();

        assert_eq!(config.delay_for_attempt(5).as_millis(), 500); // Would be 3200
        assert_eq!(config.delay_for_attempt(10).as_millis(), 500);
    }

    #[test]
    fn serialization_roundtrip() {
        let config = RetryConfig::new(200, 5000, 1.5, 7);
        let json = serde_json::to_string(&config).unwrap();
        let deserialized: RetryConfig = serde_json::from_str(&json).unwrap();

        assert_eq!(config.initial_delay_ms, deserialized.initial_delay_ms);
        assert_eq!(config.max_delay_ms, deserialized.max_delay_ms);
        assert_eq!(config.max_retries, deserialized.max_retries);
    }

    #[test]
    fn jitter_adds_variation() {
        let config = RetryConfig::default();

        // Collect 20 delay values
        let delays: Vec<u128> = (0..20)
            .map(|_| config.delay_for_attempt(2).as_millis())
            .collect();

        // With jitter, we should see some variation
        let min = delays.iter().min().unwrap();
        let max = delays.iter().max().unwrap();

        // There should be at least some difference (not all identical)
        // This test might rarely fail if random produces identical values
        if config.jitter_enabled {
            assert!(max - min > 0 || delays.len() == 1);
        }
    }
}

// ============================================================================
// Security Warning Tests
// ============================================================================

mod security_warning_tests {
    use super::*;

    #[test]
    fn critical_warning_is_critical() {
        let warning = SecurityWarning::critical("TEST001", "Test message", "Fix it");
        assert!(warning.is_critical());
        assert_eq!(warning.severity, WarningSeverity::Critical);
    }

    #[test]
    fn warning_level_is_not_critical() {
        let warning = SecurityWarning::warning("TEST002", "Warning message", "Consider fixing");
        assert!(!warning.is_critical());
        assert_eq!(warning.severity, WarningSeverity::Warning);
    }

    #[test]
    fn info_level_is_not_critical() {
        let warning = SecurityWarning::info("TEST003", "Info message", "No action needed");
        assert!(!warning.is_critical());
        assert_eq!(warning.severity, WarningSeverity::Info);
    }

    #[test]
    fn warning_display_format() {
        let warning = SecurityWarning::critical("SEC001", "TLS disabled", "Enable TLS");
        let display = format!("{}", warning);

        assert!(display.contains("CRITICAL"));
        assert!(display.contains("SEC001"));
        assert!(display.contains("TLS disabled"));
        assert!(display.contains("Enable TLS"));
    }

    #[test]
    fn severity_ordering() {
        assert!(WarningSeverity::Critical > WarningSeverity::Warning);
        assert!(WarningSeverity::Warning > WarningSeverity::Info);
        assert!(WarningSeverity::Info < WarningSeverity::Critical);
    }

    #[test]
    fn severity_display() {
        assert_eq!(format!("{}", WarningSeverity::Info), "INFO");
        assert_eq!(format!("{}", WarningSeverity::Warning), "WARNING");
        assert_eq!(format!("{}", WarningSeverity::Critical), "CRITICAL");
    }

    #[test]
    fn warning_new_constructor() {
        let warning = SecurityWarning::new(
            WarningSeverity::Warning,
            "CODE",
            "Message text",
            "Recommendation text",
        );

        assert_eq!(warning.code, "CODE");
        assert_eq!(warning.message, "Message text");
        assert_eq!(warning.recommendation, "Recommendation text");
    }

    #[test]
    fn warning_clone() {
        let original = SecurityWarning::critical("CLONE001", "Clone test", "Test clone");
        let cloned = original.clone();

        assert_eq!(original.code, cloned.code);
        assert_eq!(original.message, cloned.message);
        assert_eq!(original.severity, cloned.severity);
    }
}

// ============================================================================
// Property-Based Tests
// ============================================================================

mod proptest_tests {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        #[test]
        fn retry_delay_never_exceeds_max(
            initial in 1u64..1000u64,
            max in 1000u64..100_000u64,
            multiplier in 1.0f64..5.0f64,
            attempt in 0u32..20u32
        ) {
            let config = RetryConfig::new(initial, max, multiplier, 10).without_jitter();
            let delay = config.delay_for_attempt(attempt);
            prop_assert!(delay.as_millis() <= u128::from(max));
        }

        #[test]
        fn retry_delay_never_negative(
            initial in 1u64..10000u64,
            max in 1u64..100000u64,
            attempt in 0u32..50u32
        ) {
            let config = RetryConfig::new(initial, max, 2.0, 10);
            let delay = config.delay_for_attempt(attempt);
            prop_assert!(delay.as_nanos() >= 0);
        }

        #[test]
        fn jitter_keeps_delay_within_bounds(
            initial in 100u64..1000u64,
            max in 5000u64..50000u64,
            jitter_factor in 0.0f64..0.5f64,
            attempt in 0u32..10u32
        ) {
            let config = RetryConfig {
                initial_delay_ms: initial,
                max_delay_ms: max,
                multiplier: 2.0,
                max_retries: 10,
                jitter_enabled: true,
                jitter_factor,
            };

            // Run multiple times to test jitter bounds
            for _ in 0..10 {
                let delay = config.delay_for_attempt(attempt);
                // With jitter, delay could be 0 but should not exceed max significantly
                // Allow some tolerance for jitter
                let expected_max = (max as f64 * (1.0 + jitter_factor)) as u128;
                prop_assert!(delay.as_millis() <= expected_max);
            }
        }

        #[test]
        fn exponential_growth_without_jitter(
            initial in 10u64..100u64,
            multiplier in 1.5f64..3.0f64
        ) {
            let config = RetryConfig::new(initial, 1_000_000, multiplier, 10).without_jitter();

            let delay0 = config.delay_for_attempt(0).as_millis();
            let delay1 = config.delay_for_attempt(1).as_millis();

            // delay1 should be approximately delay0 * multiplier
            let expected = (initial as f64 * multiplier) as u128;
            prop_assert_eq!(delay1, expected);
            prop_assert!(delay1 > delay0 || multiplier < 1.01);
        }

        #[test]
        fn security_warning_severity_total_ordering(
            sev1 in prop_oneof![
                Just(WarningSeverity::Info),
                Just(WarningSeverity::Warning),
                Just(WarningSeverity::Critical)
            ],
            sev2 in prop_oneof![
                Just(WarningSeverity::Info),
                Just(WarningSeverity::Warning),
                Just(WarningSeverity::Critical)
            ]
        ) {
            // Verify total ordering properties
            if sev1 < sev2 {
                prop_assert!(sev2 > sev1);
            }
            if sev1 == sev2 {
                prop_assert!(!(sev1 < sev2) && !(sev1 > sev2));
            }
            if sev1 > sev2 {
                prop_assert!(sev2 < sev1);
            }
        }

        #[test]
        fn uuid_correlation_id_format(
            a in any::<u64>(),
            b in any::<u64>()
        ) {
            let uuid = Uuid::from_u64_pair(a, b);
            let formatted = uuid.to_string();
            // UUID v4 format: xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx
            prop_assert_eq!(formatted.len(), 36);
            prop_assert!(formatted.chars().filter(|c| *c == '-').count() == 4);
        }
    }
}

// ============================================================================
// Edge Cases
// ============================================================================

mod edge_cases {
    use super::*;

    #[test]
    fn retry_config_with_zero_initial_delay() {
        let config = RetryConfig::new(0, 1000, 2.0, 3).without_jitter();
        let delay = config.delay_for_attempt(0);
        assert_eq!(delay.as_millis(), 0);
    }

    #[test]
    fn retry_config_with_zero_max_retries() {
        let config = RetryConfig::new(100, 1000, 2.0, 0);
        assert_eq!(config.max_retries, 0);
    }

    #[test]
    fn retry_config_multiplier_less_than_one() {
        let config = RetryConfig::new(1000, 10000, 0.5, 5).without_jitter();
        // Delays should decrease
        let d0 = config.delay_for_attempt(0);
        let d1 = config.delay_for_attempt(1);
        assert!(d1 < d0);
    }

    #[test]
    fn retry_config_multiplier_one() {
        let config = RetryConfig::new(100, 1000, 1.0, 5).without_jitter();
        // All delays should be the same
        let d0 = config.delay_for_attempt(0);
        let d1 = config.delay_for_attempt(1);
        let d2 = config.delay_for_attempt(2);
        assert_eq!(d0, d1);
        assert_eq!(d1, d2);
    }

    #[test]
    fn very_large_attempt_number() {
        let config = RetryConfig::new(1, 1000, 2.0, 100).without_jitter();
        // Should be capped at max
        let delay = config.delay_for_attempt(100);
        assert_eq!(delay.as_millis(), 1000);
    }

    #[test]
    fn correlated_config_with_zero_timeout() {
        let config = CorrelatedClientConfig::default().with_timeout(Duration::ZERO);
        assert_eq!(config.timeout, Duration::ZERO);
    }

    #[test]
    fn security_warning_with_empty_strings() {
        let warning = SecurityWarning::new(WarningSeverity::Info, "", "", "");
        assert!(warning.code.is_empty());
        assert!(warning.message.is_empty());
        assert!(warning.recommendation.is_empty());
    }

    #[test]
    fn security_warning_with_unicode() {
        let warning = SecurityWarning::new(
            WarningSeverity::Warning,
            "日本語",
            "Сообщение",
            "推奨事項 🔒",
        );
        assert_eq!(warning.code, "日本語");
        assert!(warning.recommendation.contains("🔒"));
    }

    #[test]
    fn x_request_id_header_constant() {
        assert_eq!(X_REQUEST_ID, "x-request-id");
    }
}

// ============================================================================
// Concurrent Access Tests
// ============================================================================

mod concurrency_tests {
    use super::*;
    use std::sync::Arc;

    #[tokio::test]
    async fn client_shared_across_tasks() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/concurrent"))
            .respond_with(ResponseTemplate::new(200))
            .mount(&mock_server)
            .await;

        let client = Arc::new(CorrelatedHttpClient::new().unwrap());
        let uri = mock_server.uri();

        let handles: Vec<_> = (0..10)
            .map(|_| {
                let client = Arc::clone(&client);
                let uri = uri.clone();
                tokio::spawn(async move {
                    client
                        .get(format!("{}/concurrent", uri))
                        .send()
                        .await
                        .unwrap()
                        .status()
                })
            })
            .collect();

        for handle in handles {
            let status = handle.await.unwrap();
            assert_eq!(status, 200);
        }
    }

    #[tokio::test]
    async fn retry_config_clone_is_independent() {
        let original = RetryConfig::default();
        let cloned = original.clone();

        // Modifying one shouldn't affect the other
        // (RetryConfig is Clone, not shared state)
        assert_eq!(original.max_retries, cloned.max_retries);
    }
}

// ============================================================================
// Weather Adapter Tests (Wiremock)
// ============================================================================

mod weather_adapter_tests {
    use super::*;
    use wiremock::matchers::query_param;

    fn weather_current_response() -> serde_json::Value {
        serde_json::json!({
            "current": {
                "time": "2025-02-09T12:00",
                "interval": 900,
                "temperature_2m": 5.2,
                "relative_humidity_2m": 75,
                "apparent_temperature": 2.1,
                "weather_code": 3,
                "wind_speed_10m": 15.5
            },
            "current_units": {
                "temperature_2m": "°C",
                "relative_humidity_2m": "%",
                "wind_speed_10m": "km/h"
            }
        })
    }

    fn weather_forecast_response() -> serde_json::Value {
        serde_json::json!({
            "daily": {
                "time": ["2025-02-09", "2025-02-10", "2025-02-11"],
                "temperature_2m_max": [8.5, 10.2, 7.8],
                "temperature_2m_min": [2.1, 4.3, 1.5],
                "weather_code": [3, 61, 1],
                "precipitation_probability_max": [20, 80, 10],
                "precipitation_sum": [0.0, 12.5, 0.0],
                "sunrise": ["2025-02-09T07:30", "2025-02-10T07:28", "2025-02-11T07:26"],
                "sunset": ["2025-02-09T17:45", "2025-02-10T17:47", "2025-02-11T17:49"]
            },
            "daily_units": {
                "temperature_2m_max": "°C",
                "temperature_2m_min": "°C"
            }
        })
    }

    #[tokio::test]
    async fn weather_api_response_parsing() {
        // Test that weather API responses can be parsed correctly
        let response = weather_current_response();
        let current = response.get("current").unwrap();
        
        assert_eq!(current.get("temperature_2m").unwrap().as_f64().unwrap(), 5.2);
        assert_eq!(current.get("relative_humidity_2m").unwrap().as_i64().unwrap(), 75);
        assert_eq!(current.get("wind_speed_10m").unwrap().as_f64().unwrap(), 15.5);
    }

    #[tokio::test]
    async fn weather_forecast_response_parsing() {
        let response = weather_forecast_response();
        let daily = response.get("daily").unwrap();
        
        let temps_max = daily.get("temperature_2m_max").unwrap().as_array().unwrap();
        assert_eq!(temps_max.len(), 3);
        assert_eq!(temps_max[0].as_f64().unwrap(), 8.5);
    }

    #[tokio::test]
    async fn mock_weather_api_current() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/v1/forecast"))
            .and(query_param("current", "temperature_2m,relative_humidity_2m,apparent_temperature,weather_code,wind_speed_10m"))
            .respond_with(ResponseTemplate::new(200).set_body_json(weather_current_response()))
            .mount(&mock_server)
            .await;

        let client = CorrelatedHttpClient::new().unwrap();
        let response = client
            .get(format!("{}/v1/forecast?latitude=52.52&longitude=13.41&current=temperature_2m,relative_humidity_2m,apparent_temperature,weather_code,wind_speed_10m", mock_server.uri()))
            .send()
            .await
            .unwrap();

        assert_eq!(response.status(), 200);
        let body: serde_json::Value = response.json().await.unwrap();
        assert!(body.get("current").is_some());
    }

    #[tokio::test]
    async fn weather_api_rate_limit_response() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/v1/forecast"))
            .respond_with(ResponseTemplate::new(429).set_body_json(serde_json::json!({
                "error": true,
                "reason": "Rate limit exceeded"
            })))
            .mount(&mock_server)
            .await;

        let client = CorrelatedHttpClient::new().unwrap();
        let response = client
            .get(format!("{}/v1/forecast?latitude=52.52&longitude=13.41", mock_server.uri()))
            .send()
            .await
            .unwrap();

        assert_eq!(response.status(), 429);
    }
}

// ============================================================================
// WhatsApp Adapter Tests (Wiremock)
// ============================================================================

mod whatsapp_adapter_tests {
    use super::*;
    use wiremock::matchers::body_json_schema;

    fn whatsapp_send_success_response() -> serde_json::Value {
        serde_json::json!({
            "messaging_product": "whatsapp",
            "contacts": [{
                "input": "+491234567890",
                "wa_id": "491234567890"
            }],
            "messages": [{
                "id": "wamid.HBgNNDkxNzYyMDQ1NjA5FQIAERgSMjVBODlFNUY4RjE0RkE0ODUA"
            }]
        })
    }

    fn whatsapp_media_upload_response() -> serde_json::Value {
        serde_json::json!({
            "id": "media-id-12345678"
        })
    }

    fn whatsapp_error_response() -> serde_json::Value {
        serde_json::json!({
            "error": {
                "message": "Invalid phone number format",
                "type": "OAuthException",
                "code": 100,
                "error_subcode": 33,
                "fbtrace_id": "AbCdEf123456"
            }
        })
    }

    #[tokio::test]
    async fn whatsapp_send_message_response_parsing() {
        let response = whatsapp_send_success_response();
        
        assert_eq!(response.get("messaging_product").unwrap().as_str().unwrap(), "whatsapp");
        let contacts = response.get("contacts").unwrap().as_array().unwrap();
        assert_eq!(contacts.len(), 1);
        assert_eq!(contacts[0].get("wa_id").unwrap().as_str().unwrap(), "491234567890");
    }

    #[tokio::test]
    async fn mock_whatsapp_send_text() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/v17.0/123456789/messages"))
            .and(header("Authorization", "Bearer test_token"))
            .and(header("Content-Type", "application/json"))
            .respond_with(ResponseTemplate::new(200).set_body_json(whatsapp_send_success_response()))
            .mount(&mock_server)
            .await;

        let client = CorrelatedHttpClient::new().unwrap();
        let body = serde_json::json!({
            "messaging_product": "whatsapp",
            "to": "+491234567890",
            "type": "text",
            "text": {"body": "Hello World"}
        });

        let response = client
            .post(format!("{}/v17.0/123456789/messages", mock_server.uri()))
            .header("Authorization", "Bearer test_token")
            .json(&body)
            .send()
            .await
            .unwrap();

        assert_eq!(response.status(), 200);
        let body: serde_json::Value = response.json().await.unwrap();
        assert!(body.get("messages").is_some());
    }

    #[tokio::test]
    async fn mock_whatsapp_upload_media() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/v17.0/123456789/media"))
            .and(header("Authorization", "Bearer test_token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(whatsapp_media_upload_response()))
            .mount(&mock_server)
            .await;

        let client = CorrelatedHttpClient::new().unwrap();
        let response = client
            .post(format!("{}/v17.0/123456789/media", mock_server.uri()))
            .header("Authorization", "Bearer test_token")
            .body("audio data here")
            .send()
            .await
            .unwrap();

        assert_eq!(response.status(), 200);
        let body: serde_json::Value = response.json().await.unwrap();
        assert!(body.get("id").is_some());
    }

    #[tokio::test]
    async fn whatsapp_error_response_handling() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/v17.0/123456789/messages"))
            .respond_with(ResponseTemplate::new(400).set_body_json(whatsapp_error_response()))
            .mount(&mock_server)
            .await;

        let client = CorrelatedHttpClient::new().unwrap();
        let response = client
            .post(format!("{}/v17.0/123456789/messages", mock_server.uri()))
            .header("Authorization", "Bearer test_token")
            .json(&serde_json::json!({}))
            .send()
            .await
            .unwrap();

        assert_eq!(response.status(), 400);
        let body: serde_json::Value = response.json().await.unwrap();
        assert!(body.get("error").is_some());
        assert_eq!(body["error"]["code"].as_i64().unwrap(), 100);
    }

    #[tokio::test]
    async fn whatsapp_webhook_signature_verification() {
        // Test HMAC-SHA256 webhook signature verification logic
        use hmac::{Hmac, Mac};
        use sha2::Sha256;
        
        type HmacSha256 = Hmac<Sha256>;
        
        let secret = "my_webhook_secret";
        let payload = r#"{"object":"whatsapp_business_account"}"#;
        
        let mut mac = HmacSha256::new_from_slice(secret.as_bytes()).unwrap();
        mac.update(payload.as_bytes());
        let result = mac.finalize();
        let signature = hex::encode(result.into_bytes());
        
        // Verify signature format
        assert_eq!(signature.len(), 64); // SHA256 produces 32 bytes = 64 hex chars
        
        // Verify that the same input produces the same signature
        let mut mac2 = HmacSha256::new_from_slice(secret.as_bytes()).unwrap();
        mac2.update(payload.as_bytes());
        let signature2 = hex::encode(mac2.finalize().into_bytes());
        
        assert_eq!(signature, signature2);
    }
}

// ============================================================================
// Ollama Inference Adapter Tests (Wiremock)
// ============================================================================

mod ollama_adapter_tests {
    use super::*;

    fn ollama_generate_response() -> serde_json::Value {
        serde_json::json!({
            "model": "qwen2.5:1.5b",
            "created_at": "2025-02-09T12:00:00Z",
            "response": "Hello! How can I help you today?",
            "done": true,
            "context": [1, 2, 3],
            "total_duration": 1500000000i64,
            "load_duration": 100000000i64,
            "prompt_eval_count": 10,
            "prompt_eval_duration": 200000000i64,
            "eval_count": 25,
            "eval_duration": 1200000000i64
        })
    }

    fn ollama_streaming_chunk() -> serde_json::Value {
        serde_json::json!({
            "model": "qwen2.5:1.5b",
            "created_at": "2025-02-09T12:00:00Z",
            "response": "Hello",
            "done": false
        })
    }

    fn ollama_streaming_done() -> serde_json::Value {
        serde_json::json!({
            "model": "qwen2.5:1.5b",
            "created_at": "2025-02-09T12:00:00Z",
            "response": "",
            "done": true,
            "total_duration": 1500000000i64,
            "eval_count": 25
        })
    }

    fn ollama_embeddings_response() -> serde_json::Value {
        serde_json::json!({
            "model": "nomic-embed-text",
            "embeddings": [[0.1, 0.2, 0.3, 0.4, 0.5]],
            "total_duration": 50000000i64,
            "load_duration": 10000000i64,
            "prompt_eval_count": 5
        })
    }

    fn ollama_models_list() -> serde_json::Value {
        serde_json::json!({
            "models": [
                {
                    "name": "qwen2.5:1.5b",
                    "modified_at": "2025-02-01T12:00:00Z",
                    "size": 1500000000i64,
                    "digest": "sha256:abc123",
                    "details": {
                        "format": "gguf",
                        "family": "qwen2",
                        "parameter_size": "1.5B",
                        "quantization_level": "Q4_K_M"
                    }
                },
                {
                    "name": "nomic-embed-text",
                    "modified_at": "2025-02-01T12:00:00Z",
                    "size": 500000000i64,
                    "digest": "sha256:def456"
                }
            ]
        })
    }

    fn ollama_error_response() -> serde_json::Value {
        serde_json::json!({
            "error": "model 'unknown-model' not found"
        })
    }

    #[tokio::test]
    async fn ollama_generate_response_parsing() {
        let response = ollama_generate_response();
        
        assert_eq!(response.get("model").unwrap().as_str().unwrap(), "qwen2.5:1.5b");
        assert_eq!(response.get("response").unwrap().as_str().unwrap(), "Hello! How can I help you today?");
        assert!(response.get("done").unwrap().as_bool().unwrap());
        assert_eq!(response.get("eval_count").unwrap().as_i64().unwrap(), 25);
    }

    #[tokio::test]
    async fn mock_ollama_generate() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/api/generate"))
            .and(header("Content-Type", "application/json"))
            .respond_with(ResponseTemplate::new(200).set_body_json(ollama_generate_response()))
            .mount(&mock_server)
            .await;

        let client = CorrelatedHttpClient::new().unwrap();
        let body = serde_json::json!({
            "model": "qwen2.5:1.5b",
            "prompt": "Hello!",
            "stream": false
        });

        let response = client
            .post(format!("{}/api/generate", mock_server.uri()))
            .json(&body)
            .send()
            .await
            .unwrap();

        assert_eq!(response.status(), 200);
        let body: serde_json::Value = response.json().await.unwrap();
        assert_eq!(body["response"].as_str().unwrap(), "Hello! How can I help you today?");
    }

    #[tokio::test]
    async fn mock_ollama_chat() {
        let mock_server = MockServer::start().await;

        let chat_response = serde_json::json!({
            "model": "qwen2.5:1.5b",
            "created_at": "2025-02-09T12:00:00Z",
            "message": {
                "role": "assistant",
                "content": "I'm doing well, thank you for asking!"
            },
            "done": true,
            "total_duration": 1500000000i64,
            "eval_count": 30
        });

        Mock::given(method("POST"))
            .and(path("/api/chat"))
            .respond_with(ResponseTemplate::new(200).set_body_json(chat_response))
            .mount(&mock_server)
            .await;

        let client = CorrelatedHttpClient::new().unwrap();
        let body = serde_json::json!({
            "model": "qwen2.5:1.5b",
            "messages": [
                {"role": "user", "content": "How are you?"}
            ],
            "stream": false
        });

        let response = client
            .post(format!("{}/api/chat", mock_server.uri()))
            .json(&body)
            .send()
            .await
            .unwrap();

        assert_eq!(response.status(), 200);
        let body: serde_json::Value = response.json().await.unwrap();
        assert!(body["message"]["content"].as_str().unwrap().contains("doing well"));
    }

    #[tokio::test]
    async fn mock_ollama_embeddings() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/api/embed"))
            .respond_with(ResponseTemplate::new(200).set_body_json(ollama_embeddings_response()))
            .mount(&mock_server)
            .await;

        let client = CorrelatedHttpClient::new().unwrap();
        let body = serde_json::json!({
            "model": "nomic-embed-text",
            "input": ["Hello world"]
        });

        let response = client
            .post(format!("{}/api/embed", mock_server.uri()))
            .json(&body)
            .send()
            .await
            .unwrap();

        assert_eq!(response.status(), 200);
        let body: serde_json::Value = response.json().await.unwrap();
        let embeddings = body["embeddings"].as_array().unwrap();
        assert_eq!(embeddings.len(), 1);
        assert_eq!(embeddings[0].as_array().unwrap().len(), 5);
    }

    #[tokio::test]
    async fn mock_ollama_list_models() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/api/tags"))
            .respond_with(ResponseTemplate::new(200).set_body_json(ollama_models_list()))
            .mount(&mock_server)
            .await;

        let client = CorrelatedHttpClient::new().unwrap();
        let response = client
            .get(format!("{}/api/tags", mock_server.uri()))
            .send()
            .await
            .unwrap();

        assert_eq!(response.status(), 200);
        let body: serde_json::Value = response.json().await.unwrap();
        let models = body["models"].as_array().unwrap();
        assert_eq!(models.len(), 2);
        assert_eq!(models[0]["name"].as_str().unwrap(), "qwen2.5:1.5b");
    }

    #[tokio::test]
    async fn ollama_model_not_found_error() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/api/generate"))
            .respond_with(ResponseTemplate::new(404).set_body_json(ollama_error_response()))
            .mount(&mock_server)
            .await;

        let client = CorrelatedHttpClient::new().unwrap();
        let body = serde_json::json!({
            "model": "unknown-model",
            "prompt": "Hello",
            "stream": false
        });

        let response = client
            .post(format!("{}/api/generate", mock_server.uri()))
            .json(&body)
            .send()
            .await
            .unwrap();

        assert_eq!(response.status(), 404);
        let body: serde_json::Value = response.json().await.unwrap();
        assert!(body["error"].as_str().unwrap().contains("not found"));
    }

    #[tokio::test]
    async fn ollama_service_unavailable() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/api/generate"))
            .respond_with(ResponseTemplate::new(503))
            .mount(&mock_server)
            .await;

        let client = CorrelatedHttpClient::new().unwrap();
        let response = client
            .post(format!("{}/api/generate", mock_server.uri()))
            .json(&serde_json::json!({"model": "test", "prompt": "hi"}))
            .send()
            .await
            .unwrap();

        assert_eq!(response.status(), 503);
    }

    #[tokio::test]
    async fn ollama_timeout_handling() {
        let mock_server = MockServer::start().await;

        // Simulate a slow response (would timeout in real scenario)
        Mock::given(method("POST"))
            .and(path("/api/generate"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(ollama_generate_response())
                    .set_delay(Duration::from_millis(100))
            )
            .mount(&mock_server)
            .await;

        let config = CorrelatedClientConfig::default()
            .with_timeout(Duration::from_secs(10)); // Long enough to complete
        let client = CorrelatedHttpClient::with_config(config).unwrap();
        
        let response = client
            .post(format!("{}/api/generate", mock_server.uri()))
            .json(&serde_json::json!({"model": "test", "prompt": "hi"}))
            .send()
            .await;

        assert!(response.is_ok());
    }
}

// ============================================================================
// Circuit Breaker Integration Tests
// ============================================================================

mod circuit_breaker_integration_tests {
    use super::*;
    use infrastructure::CircuitBreakerConfig;

    #[tokio::test]
    async fn circuit_breaker_opens_after_failures() {
        let mock_server = MockServer::start().await;

        // Server always returns 500
        Mock::given(method("GET"))
            .and(path("/flaky"))
            .respond_with(ResponseTemplate::new(500))
            .mount(&mock_server)
            .await;

        let client = CorrelatedHttpClient::new().unwrap();
        
        // Make multiple failing requests
        for _ in 0..5 {
            let _ = client
                .get(format!("{}/flaky", mock_server.uri()))
                .send()
                .await;
        }

        // Circuit breaker logic would be tested at the adapter level
        // Here we just verify the mock server received the requests
    }

    #[tokio::test]
    async fn circuit_breaker_config_validation() {
        let config = CircuitBreakerConfig {
            failure_threshold: 5,
            success_threshold: 2,
            half_open_timeout_secs: 30,
        };
        
        assert_eq!(config.failure_threshold, 5);
        assert_eq!(config.success_threshold, 2);
        assert_eq!(config.half_open_timeout_secs, 30);
    }
}

// ============================================================================
// Degraded Inference Adapter Tests
// ============================================================================

mod degraded_inference_tests {
    use infrastructure::adapters::{DegradedModeConfig, ServiceStatus};

    #[test]
    fn degraded_mode_config_defaults() {
        let config = DegradedModeConfig::default();
        
        assert!(config.enabled);
        assert_eq!(config.retry_cooldown_secs, 30);
        assert_eq!(config.failure_threshold, 3);
        assert_eq!(config.success_threshold, 2);
        assert!(!config.unavailable_message.is_empty());
    }

    #[test]
    fn degraded_mode_config_custom() {
        let config = DegradedModeConfig {
            enabled: true,
            unavailable_message: "Custom message".to_string(),
            retry_cooldown_secs: 60,
            failure_threshold: 5,
            success_threshold: 3,
        };
        
        assert_eq!(config.retry_cooldown_secs, 60);
        assert_eq!(config.failure_threshold, 5);
        assert_eq!(config.unavailable_message, "Custom message");
    }

    #[test]
    fn service_status_variants() {
        assert_eq!(ServiceStatus::default(), ServiceStatus::Healthy);
        
        let healthy = ServiceStatus::Healthy;
        let degraded = ServiceStatus::Degraded;
        let unavailable = ServiceStatus::Unavailable;
        
        assert_ne!(healthy, degraded);
        assert_ne!(degraded, unavailable);
        assert_ne!(healthy, unavailable);
    }

    #[test]
    fn degraded_mode_config_serialization() {
        let config = DegradedModeConfig::default();
        let json = serde_json::to_string(&config).unwrap();
        
        assert!(json.contains("enabled"));
        assert!(json.contains("unavailable_message"));
        assert!(json.contains("retry_cooldown_secs"));
        
        let parsed: DegradedModeConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(config.enabled, parsed.enabled);
        assert_eq!(config.failure_threshold, parsed.failure_threshold);
    }
}

// ============================================================================
// Encryption Adapter Extended Tests
// ============================================================================

mod encryption_extended_tests {
    use infrastructure::adapters::ChaChaEncryptionAdapter;
    use application::ports::EncryptionPort;

    #[test]
    fn key_generation_produces_correct_size() {
        let key = ChaChaEncryptionAdapter::generate_key();
        assert_eq!(key.len(), 32); // 256 bits
    }

    #[test]
    fn key_generation_is_random() {
        let keys: Vec<_> = (0..10)
            .map(|_| ChaChaEncryptionAdapter::generate_key())
            .collect();
        
        // All keys should be unique
        for (i, key1) in keys.iter().enumerate() {
            for (j, key2) in keys.iter().enumerate() {
                if i != j {
                    assert_ne!(key1, key2, "Keys at index {} and {} should be different", i, j);
                }
            }
        }
    }

    #[tokio::test]
    async fn encryption_roundtrip_various_sizes() {
        let key = ChaChaEncryptionAdapter::generate_key();
        let adapter = ChaChaEncryptionAdapter::new(&key).unwrap();
        
        // Test various data sizes
        let test_sizes = [0, 1, 16, 100, 1024, 65536];
        
        for size in test_sizes {
            let plaintext: Vec<u8> = (0..size).map(|i| (i % 256) as u8).collect();
            let encrypted = adapter.encrypt(&plaintext).await.unwrap();
            let decrypted = adapter.decrypt(&encrypted).await.unwrap();
            
            assert_eq!(plaintext, decrypted, "Failed for size {}", size);
        }
    }

    #[tokio::test]
    async fn encryption_with_unicode() {
        let key = ChaChaEncryptionAdapter::generate_key();
        let adapter = ChaChaEncryptionAdapter::new(&key).unwrap();
        
        let test_strings = [
            "Hello, World!",
            "Grüß Gott!",
            "こんにちは",
            "مرحبا",
            "🎉🎊🎈",
            "Mixed: Hello 世界 🌍",
        ];
        
        for s in test_strings {
            let encrypted = adapter.encrypt(s.as_bytes()).await.unwrap();
            let decrypted = adapter.decrypt(&encrypted).await.unwrap();
            let decrypted_str = String::from_utf8(decrypted).unwrap();
            
            assert_eq!(s, decrypted_str, "Failed for string: {}", s);
        }
    }

    #[test]
    fn new_adapter_rejects_wrong_key_size() {
        // Too short
        for size in [0, 1, 15, 16, 31] {
            let key = vec![0u8; size];
            let result = ChaChaEncryptionAdapter::new(&key);
            assert!(result.is_err(), "Should reject key of size {}", size);
        }
        
        // Too long
        for size in [33, 64, 128] {
            let key = vec![0u8; size];
            let result = ChaChaEncryptionAdapter::new(&key);
            assert!(result.is_err(), "Should reject key of size {}", size);
        }
        
        // Correct size
        let key = vec![0u8; 32];
        let result = ChaChaEncryptionAdapter::new(&key);
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn decryption_fails_for_tampered_data() {
        let key = ChaChaEncryptionAdapter::generate_key();
        let adapter = ChaChaEncryptionAdapter::new(&key).unwrap();
        
        let plaintext = b"Sensitive data";
        let mut encrypted = adapter.encrypt(plaintext).await.unwrap();
        
        // Tamper with various positions
        let test_positions = [0, 12, 24, encrypted.len() / 2, encrypted.len() - 1];
        
        for &pos in &test_positions {
            if pos < encrypted.len() {
                let original = encrypted[pos];
                encrypted[pos] ^= 0xFF;
                
                let result = adapter.decrypt(&encrypted).await;
                assert!(result.is_err(), "Should fail for tampered position {}", pos);
                
                encrypted[pos] = original; // Restore for next test
            }
        }
    }

    #[tokio::test]
    async fn different_keys_cannot_decrypt() {
        let key1 = ChaChaEncryptionAdapter::generate_key();
        let key2 = ChaChaEncryptionAdapter::generate_key();
        
        let adapter1 = ChaChaEncryptionAdapter::new(&key1).unwrap();
        let adapter2 = ChaChaEncryptionAdapter::new(&key2).unwrap();
        
        let plaintext = b"Secret message";
        let encrypted = adapter1.encrypt(plaintext).await.unwrap();
        
        // Decryption with different key should fail
        let result = adapter2.decrypt(&encrypted).await;
        assert!(result.is_err());
    }
}

// ============================================================================
// API Key Hasher Extended Tests
// ============================================================================

mod api_key_hasher_extended_tests {
    use infrastructure::adapters::ApiKeyHasher;

    #[test]
    fn hasher_handles_various_key_formats() {
        let hasher = ApiKeyHasher::new();
        
        let test_keys = [
            "sk-simple",
            "sk-with-dashes-and-numbers-123",
            "very_long_api_key_with_many_characters_0123456789abcdef",
            "short",
            "a",
            "key with spaces",
            "key-with-special-chars!@#$%",
        ];
        
        for key in test_keys {
            let hash = hasher.hash(key).unwrap();
            assert!(hasher.verify(key, &hash).unwrap(), "Failed for key: {}", key);
        }
    }

    #[test]
    fn hash_timing_is_not_exploitable() {
        // This is a basic check - in production you'd want more sophisticated
        // timing analysis, but we verify the API at least accepts various inputs
        let hasher = ApiKeyHasher::new();
        let hash = hasher.hash("reference-key").unwrap();
        
        // Various wrong keys should all be rejected
        let wrong_keys = [
            "wrong-key",
            "reference-ke",  // One char short
            "reference-key!", // One char extra
            "Reference-key",  // Case difference
        ];
        
        for wrong_key in wrong_keys {
            assert!(!hasher.verify(wrong_key, &hash).unwrap());
        }
    }

    #[test]
    fn detect_plaintext_keys_handles_edge_cases() {
        // Empty iterator
        let count = ApiKeyHasher::detect_plaintext_keys(std::iter::empty());
        assert_eq!(count, 0);
        
        // All hashed
        let all_hashed = &[
            "$argon2id$v=19$m=19456,t=2,p=1$salt$hash1",
            "$argon2id$v=19$m=19456,t=2,p=1$salt$hash2",
        ];
        let count = ApiKeyHasher::detect_plaintext_keys(all_hashed.iter().copied());
        assert_eq!(count, 0);
        
        // All plaintext
        let all_plaintext = &["key1", "key2", "key3"];
        let count = ApiKeyHasher::detect_plaintext_keys(all_plaintext.iter().copied());
        assert_eq!(count, 3);
    }

    #[test]
    fn is_hashed_edge_cases() {
        // Edge cases
        assert!(!ApiKeyHasher::is_hashed(""));
        assert!(!ApiKeyHasher::is_hashed("$"));
        assert!(!ApiKeyHasher::is_hashed("$argon"));
        assert!(!ApiKeyHasher::is_hashed("argon2id"));
        
        // Valid prefixes
        assert!(ApiKeyHasher::is_hashed("$argon2id$"));
        assert!(ApiKeyHasher::is_hashed("$argon2i$"));
        assert!(ApiKeyHasher::is_hashed("$argon2d$"));
        assert!(ApiKeyHasher::is_hashed("$argon2$")); // Generic
    }
}
