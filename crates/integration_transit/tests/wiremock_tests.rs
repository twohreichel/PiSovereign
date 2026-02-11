//! Integration tests for the transit client (wiremock-based)

use wiremock::matchers::{method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

use integration_transit::{HafasTransitClient, TransitClient, TransitConfig};

fn config_for_mock(base_url: &str) -> TransitConfig {
    TransitConfig {
        base_url: base_url.to_string(),
        timeout_secs: 5,
        max_results: 3,
        cache_ttl_minutes: 0,
        ..TransitConfig::default()
    }
}

const fn sample_journeys_json() -> &'static str {
    r#"{
        "earlierRef": "e1",
        "laterRef": "l1",
        "journeys": [{
            "refreshToken": "tok1",
            "legs": [
                {
                    "origin": {
                        "id": "900100003",
                        "name": "S+U Alexanderplatz",
                        "location": { "latitude": 52.521508, "longitude": 13.411267 }
                    },
                    "destination": {
                        "id": "900023201",
                        "name": "S+U Zoologischer Garten",
                        "location": { "latitude": 52.506891, "longitude": 13.332711 }
                    },
                    "departure": "2026-02-11T10:00:00+00:00",
                    "plannedDeparture": "2026-02-11T10:00:00+00:00",
                    "arrival": "2026-02-11T10:18:00+00:00",
                    "plannedArrival": "2026-02-11T10:18:00+00:00",
                    "departureDelay": 0,
                    "arrivalDelay": 0,
                    "departurePlatform": "2",
                    "line": {
                        "name": "S5",
                        "product": "suburban",
                        "mode": "train"
                    }
                }
            ]
        }]
    }"#
}

const fn sample_locations_json() -> &'static str {
    r#"[
        {
            "id": "900100003",
            "name": "S+U Alexanderplatz",
            "location": { "latitude": 52.521508, "longitude": 13.411267 }
        },
        {
            "id": "900100004",
            "name": "S Alexanderplatz Bhf",
            "location": { "latitude": 52.5219, "longitude": 13.4115 }
        }
    ]"#
}

#[tokio::test]
async fn test_search_journeys_success() {
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/journeys"))
        .respond_with(ResponseTemplate::new(200).set_body_string(sample_journeys_json()))
        .mount(&server)
        .await;

    let config = config_for_mock(&server.uri());
    let client = HafasTransitClient::new(&config).unwrap();

    let result = client
        .search_journeys(52.52, 13.41, 52.50, 13.33, None, 3)
        .await
        .unwrap();

    assert_eq!(result.journeys.len(), 1);
    let journey = &result.journeys[0];
    assert_eq!(journey.legs.len(), 1);
    assert_eq!(journey.legs[0].origin.name, "S+U Alexanderplatz");
    assert_eq!(journey.legs[0].line.as_ref().unwrap().name, "S5");
    assert_eq!(journey.transfers(), 0);
}

#[tokio::test]
async fn test_search_journeys_rate_limited() {
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/journeys"))
        .respond_with(ResponseTemplate::new(429).insert_header("retry-after", "30"))
        .mount(&server)
        .await;

    let config = config_for_mock(&server.uri());
    let client = HafasTransitClient::new(&config).unwrap();

    let result = client
        .search_journeys(52.52, 13.41, 52.50, 13.33, None, 3)
        .await;

    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(err.is_retryable());
}

#[tokio::test]
async fn test_search_journeys_server_error() {
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/journeys"))
        .respond_with(ResponseTemplate::new(500))
        .mount(&server)
        .await;

    let config = config_for_mock(&server.uri());
    let client = HafasTransitClient::new(&config).unwrap();

    let result = client
        .search_journeys(52.52, 13.41, 52.50, 13.33, None, 3)
        .await;

    assert!(result.is_err());
}

#[tokio::test]
async fn test_find_nearby_stops() {
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/locations/nearby"))
        .respond_with(ResponseTemplate::new(200).set_body_string(sample_locations_json()))
        .mount(&server)
        .await;

    let config = config_for_mock(&server.uri());
    let client = HafasTransitClient::new(&config).unwrap();

    let stops = client.find_nearby_stops(52.52, 13.41, 5).await.unwrap();
    assert_eq!(stops.len(), 2);
    assert_eq!(stops[0].name, "S+U Alexanderplatz");
}

#[tokio::test]
async fn test_search_stops_by_name() {
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/locations"))
        .and(query_param("query", "Alexanderplatz"))
        .respond_with(ResponseTemplate::new(200).set_body_string(sample_locations_json()))
        .mount(&server)
        .await;

    let config = config_for_mock(&server.uri());
    let client = HafasTransitClient::new(&config).unwrap();

    let stops = client.search_stops("Alexanderplatz", 5).await.unwrap();
    assert_eq!(stops.len(), 2);
    assert_eq!(stops[0].id, "900100003");
}

#[tokio::test]
async fn test_search_stops_empty_query() {
    let config = TransitConfig::for_testing();
    let client = HafasTransitClient::new(&config).unwrap();

    let result = client.search_stops("", 5).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_empty_journeys_response() {
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/journeys"))
        .respond_with(ResponseTemplate::new(200).set_body_string(r#"{ "journeys": [] }"#))
        .mount(&server)
        .await;

    let config = config_for_mock(&server.uri());
    let client = HafasTransitClient::new(&config).unwrap();

    let result = client
        .search_journeys(52.52, 13.41, 52.50, 13.33, None, 3)
        .await
        .unwrap();

    assert!(result.journeys.is_empty());
}
