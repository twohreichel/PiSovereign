//! Integration tests for infrastructure crate
//!
//! Tests cover:
//! - Correlated HTTP client with wiremock
//! - Retry logic with property-based tests
//! - Security validation
//! - Configuration handling

use std::time::Duration;
use uuid::Uuid;
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use infrastructure::{
    CorrelatedClientConfig, CorrelatedHttpClient, RetryConfig, SecurityValidator, SecurityWarning,
    WarningSeverity, X_REQUEST_ID,
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
