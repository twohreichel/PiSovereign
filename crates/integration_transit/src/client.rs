//! HAFAS transit client via transport.rest API
//!
//! Provides journey planning, stop search, and nearby-stop lookup
//! using the public [v6.db.transport.rest](https://v6.db.transport.rest) API.

use std::time::Duration;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use reqwest::Client;
use serde::Deserialize;
use tracing::{debug, instrument, warn};

use crate::config::TransitConfig;
use crate::error::TransitError;
use crate::models::{Journey, Leg, LineInfo, Stop, TransitResponse};

/// Trait for transit service clients
#[async_trait]
pub trait TransitClient: Send + Sync {
    /// Search for journeys between two coordinate pairs
    async fn search_journeys(
        &self,
        from_lat: f64,
        from_lon: f64,
        to_lat: f64,
        to_lon: f64,
        departure: Option<DateTime<Utc>>,
        max_results: u8,
    ) -> Result<TransitResponse, TransitError>;

    /// Find stops near a set of coordinates
    async fn find_nearby_stops(
        &self,
        latitude: f64,
        longitude: f64,
        max_results: u8,
    ) -> Result<Vec<Stop>, TransitError>;

    /// Search for stops by name
    async fn search_stops(&self, query: &str, max_results: u8) -> Result<Vec<Stop>, TransitError>;

    /// Check if the transit service is reachable
    async fn is_healthy(&self) -> bool;
}

/// HAFAS-based transit client using the transport.rest API
#[derive(Debug)]
pub struct HafasTransitClient {
    client: Client,
    config: TransitConfig,
}

impl HafasTransitClient {
    /// Create a new HAFAS transit client
    ///
    /// # Errors
    ///
    /// Returns an error if the HTTP client cannot be initialized.
    pub fn new(config: &TransitConfig) -> Result<Self, TransitError> {
        let client = Client::builder()
            .timeout(Duration::from_secs(config.timeout_secs))
            .user_agent("PiSovereign/1.0")
            .build()
            .map_err(|e| TransitError::ConnectionFailed(e.to_string()))?;

        Ok(Self {
            client,
            config: config.clone(),
        })
    }

    /// Build product query parameters based on config
    fn product_params(&self) -> Vec<(&str, &str)> {
        vec![
            ("bus", bool_str(self.config.products_bus)),
            ("suburban", bool_str(self.config.products_suburban)),
            ("subway", bool_str(self.config.products_subway)),
            ("tram", bool_str(self.config.products_tram)),
            ("regional", bool_str(self.config.products_regional)),
            ("national", bool_str(self.config.products_national)),
            (
                "nationalExpress",
                bool_str(self.config.products_national_express),
            ),
        ]
    }

    /// Parse the raw HAFAS JSON journey response into typed models
    fn parse_journeys_response(body: &str) -> Result<TransitResponse, TransitError> {
        let raw: RawJourneysResponse =
            serde_json::from_str(body).map_err(|e| TransitError::ParseError(e.to_string()))?;

        let journeys = raw
            .journeys
            .into_iter()
            .map(Self::convert_journey)
            .collect();

        Ok(TransitResponse {
            journeys,
            earlier_ref: raw.earlier_ref,
            later_ref: raw.later_ref,
        })
    }

    /// Convert a raw journey to a typed journey
    fn convert_journey(raw: RawJourney) -> Journey {
        let legs = raw.legs.into_iter().map(Self::convert_leg).collect();
        Journey {
            legs,
            refresh_token: raw.refresh_token,
        }
    }

    /// Convert a raw leg to a typed leg
    fn convert_leg(raw: RawLeg) -> Leg {
        let walking = raw.walking.unwrap_or(false);
        Leg {
            origin: Self::convert_stop(raw.origin),
            destination: Self::convert_stop(raw.destination),
            departure: raw.departure.unwrap_or_else(Utc::now),
            planned_departure: raw.planned_departure.unwrap_or_else(Utc::now),
            arrival: raw.arrival.unwrap_or_else(Utc::now),
            planned_arrival: raw.planned_arrival.unwrap_or_else(Utc::now),
            departure_delay: raw.departure_delay,
            arrival_delay: raw.arrival_delay,
            departure_platform: raw.departure_platform,
            arrival_platform: raw.arrival_platform,
            line: raw.line.map(Self::convert_line),
            walking,
            distance: raw.distance,
        }
    }

    /// Convert a raw stop to a typed stop
    fn convert_stop(raw: RawStop) -> Stop {
        let (latitude, longitude) = raw.location.map_or((None, None), |loc| {
            (Some(loc.latitude), Some(loc.longitude))
        });

        Stop {
            id: raw.id.unwrap_or_default(),
            name: raw.name.unwrap_or_default(),
            latitude,
            longitude,
        }
    }

    /// Convert a raw line to a typed line info
    fn convert_line(raw: RawLine) -> LineInfo {
        LineInfo {
            name: raw.name.unwrap_or_default(),
            product: raw.product.unwrap_or_default(),
            mode: raw.mode.unwrap_or_default(),
            direction: None, // direction comes from the leg, not the line
        }
    }

    /// Parse the raw HAFAS JSON locations response into typed stops
    fn parse_locations_response(body: &str) -> Result<Vec<Stop>, TransitError> {
        let raw: Vec<RawStop> =
            serde_json::from_str(body).map_err(|e| TransitError::ParseError(e.to_string()))?;

        Ok(raw.into_iter().map(Self::convert_stop).collect())
    }
}

#[async_trait]
impl TransitClient for HafasTransitClient {
    #[instrument(skip(self), fields(from = %format!("{from_lat},{from_lon}"), to = %format!("{to_lat},{to_lon}")))]
    async fn search_journeys(
        &self,
        from_lat: f64,
        from_lon: f64,
        to_lat: f64,
        to_lon: f64,
        departure: Option<DateTime<Utc>>,
        max_results: u8,
    ) -> Result<TransitResponse, TransitError> {
        let url = format!("{}/journeys", self.config.base_url);

        let mut params: Vec<(&str, String)> = vec![
            ("from.latitude", from_lat.to_string()),
            ("from.longitude", from_lon.to_string()),
            ("from.address", format!("{from_lat},{from_lon}")),
            ("to.latitude", to_lat.to_string()),
            ("to.longitude", to_lon.to_string()),
            ("to.address", format!("{to_lat},{to_lon}")),
            ("results", max_results.to_string()),
            ("stopovers", "false".to_string()),
            ("remarks", "true".to_string()),
            ("language", "de".to_string()),
        ];

        if let Some(dep) = departure {
            params.push(("departure", dep.to_rfc3339()));
        }

        for (key, val) in self.product_params() {
            params.push((key, val.to_string()));
        }

        debug!(?url, "Searching journeys");

        let response = self
            .client
            .get(&url)
            .query(&params)
            .send()
            .await
            .map_err(|e| {
                if e.is_timeout() {
                    TransitError::Timeout {
                        timeout_secs: self.config.timeout_secs,
                    }
                } else {
                    TransitError::ConnectionFailed(e.to_string())
                }
            })?;

        let status = response.status();
        if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
            return Err(TransitError::RateLimitExceeded {
                retry_after_secs: response
                    .headers()
                    .get("retry-after")
                    .and_then(|v| v.to_str().ok())
                    .and_then(|v| v.parse().ok()),
            });
        }

        if !status.is_success() {
            return Err(TransitError::RequestFailed(format!("HTTP {status}")));
        }

        let body = response
            .text()
            .await
            .map_err(|e| TransitError::ParseError(e.to_string()))?;

        let result = Self::parse_journeys_response(&body)?;

        if result.journeys.is_empty() {
            warn!("No journeys found");
        }

        debug!(count = result.journeys.len(), "Journeys found");
        Ok(result)
    }

    #[instrument(skip(self))]
    async fn find_nearby_stops(
        &self,
        latitude: f64,
        longitude: f64,
        max_results: u8,
    ) -> Result<Vec<Stop>, TransitError> {
        let url = format!("{}/locations/nearby", self.config.base_url);

        let params = [
            ("latitude", latitude.to_string()),
            ("longitude", longitude.to_string()),
            ("results", max_results.to_string()),
            ("stops", "true".to_string()),
            ("poi", "false".to_string()),
        ];

        debug!(?url, "Searching nearby stops");

        let response = self
            .client
            .get(&url)
            .query(&params)
            .send()
            .await
            .map_err(|e| {
                if e.is_timeout() {
                    TransitError::Timeout {
                        timeout_secs: self.config.timeout_secs,
                    }
                } else {
                    TransitError::ConnectionFailed(e.to_string())
                }
            })?;

        if !response.status().is_success() {
            return Err(TransitError::RequestFailed(format!(
                "HTTP {}",
                response.status()
            )));
        }

        let body = response
            .text()
            .await
            .map_err(|e| TransitError::ParseError(e.to_string()))?;

        Self::parse_locations_response(&body)
    }

    #[instrument(skip(self))]
    async fn search_stops(&self, query: &str, max_results: u8) -> Result<Vec<Stop>, TransitError> {
        if query.trim().is_empty() {
            return Err(TransitError::InvalidLocation(
                "Search query must not be empty".to_string(),
            ));
        }

        let url = format!("{}/locations", self.config.base_url);

        let params = [
            ("query", query.to_string()),
            ("results", max_results.to_string()),
            ("stops", "true".to_string()),
            ("addresses", "false".to_string()),
            ("poi", "false".to_string()),
            ("fuzzy", "true".to_string()),
        ];

        debug!(?url, ?query, "Searching stops by name");

        let response = self
            .client
            .get(&url)
            .query(&params)
            .send()
            .await
            .map_err(|e| {
                if e.is_timeout() {
                    TransitError::Timeout {
                        timeout_secs: self.config.timeout_secs,
                    }
                } else {
                    TransitError::ConnectionFailed(e.to_string())
                }
            })?;

        if !response.status().is_success() {
            return Err(TransitError::RequestFailed(format!(
                "HTTP {}",
                response.status()
            )));
        }

        let body = response
            .text()
            .await
            .map_err(|e| TransitError::ParseError(e.to_string()))?;

        Self::parse_locations_response(&body)
    }

    async fn is_healthy(&self) -> bool {
        let url = format!("{}/locations?query=test&results=1", self.config.base_url);
        self.client.get(&url).send().await.is_ok()
    }
}

/// Convert bool to "true"/"false" str for query params
const fn bool_str(val: bool) -> &'static str {
    if val { "true" } else { "false" }
}

// --- Raw API response types for deserialization ---

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawJourneysResponse {
    journeys: Vec<RawJourney>,
    earlier_ref: Option<String>,
    later_ref: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawJourney {
    legs: Vec<RawLeg>,
    refresh_token: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawLeg {
    origin: RawStop,
    destination: RawStop,
    departure: Option<DateTime<Utc>>,
    planned_departure: Option<DateTime<Utc>>,
    arrival: Option<DateTime<Utc>>,
    planned_arrival: Option<DateTime<Utc>>,
    departure_delay: Option<i64>,
    arrival_delay: Option<i64>,
    departure_platform: Option<String>,
    arrival_platform: Option<String>,
    line: Option<RawLine>,
    walking: Option<bool>,
    distance: Option<u32>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawStop {
    id: Option<String>,
    name: Option<String>,
    location: Option<RawLocation>,
}

#[derive(Debug, Deserialize)]
struct RawLocation {
    latitude: f64,
    longitude: f64,
}

#[derive(Debug, Deserialize)]
struct RawLine {
    name: Option<String>,
    product: Option<String>,
    mode: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bool_str() {
        assert_eq!(bool_str(true), "true");
        assert_eq!(bool_str(false), "false");
    }

    #[test]
    fn test_parse_journeys_response() {
        let json = r#"{
            "earlierRef": "earlier123",
            "laterRef": "later456",
            "journeys": [{
                "refreshToken": "token123",
                "legs": [{
                    "origin": {
                        "id": "8011113",
                        "name": "Berlin Südkreuz",
                        "location": { "latitude": 52.47623, "longitude": 13.365863 }
                    },
                    "destination": {
                        "id": "8010159",
                        "name": "Leipzig Hbf",
                        "location": { "latitude": 51.3, "longitude": 12.38 }
                    },
                    "departure": "2026-02-11T14:37:00Z",
                    "plannedDeparture": "2026-02-11T14:37:00Z",
                    "arrival": "2026-02-11T15:42:00Z",
                    "plannedArrival": "2026-02-11T15:42:00Z",
                    "departureDelay": 60,
                    "arrivalDelay": null,
                    "departurePlatform": "3",
                    "arrivalPlatform": "11",
                    "line": {
                        "name": "ICE 1601",
                        "product": "nationalExpress",
                        "mode": "train"
                    }
                }]
            }]
        }"#;

        let result = HafasTransitClient::parse_journeys_response(json).unwrap();
        assert_eq!(result.journeys.len(), 1);
        assert_eq!(result.earlier_ref.as_deref(), Some("earlier123"));
        assert_eq!(result.later_ref.as_deref(), Some("later456"));

        let journey = &result.journeys[0];
        assert_eq!(journey.refresh_token.as_deref(), Some("token123"));
        assert_eq!(journey.legs.len(), 1);

        let leg = &journey.legs[0];
        assert_eq!(leg.origin.name, "Berlin Südkreuz");
        assert_eq!(leg.destination.name, "Leipzig Hbf");
        assert_eq!(leg.departure_delay, Some(60));
        assert_eq!(leg.departure_platform.as_deref(), Some("3"));
        assert_eq!(leg.line.as_ref().unwrap().name, "ICE 1601");
        assert_eq!(leg.line.as_ref().unwrap().product, "nationalExpress");
        assert!(!leg.walking);
    }

    #[test]
    fn test_parse_journeys_walking_leg() {
        let json = r#"{
            "journeys": [{
                "legs": [{
                    "origin": { "name": "Platform 3" },
                    "destination": { "name": "Platform 11" },
                    "departure": "2026-02-11T15:42:00Z",
                    "plannedDeparture": "2026-02-11T15:42:00Z",
                    "arrival": "2026-02-11T15:48:00Z",
                    "plannedArrival": "2026-02-11T15:48:00Z",
                    "walking": true,
                    "distance": 116
                }]
            }]
        }"#;

        let result = HafasTransitClient::parse_journeys_response(json).unwrap();
        let leg = &result.journeys[0].legs[0];
        assert!(leg.walking);
        assert_eq!(leg.distance, Some(116));
        assert!(leg.line.is_none());
    }

    #[test]
    fn test_parse_locations_response() {
        let json = r#"[
            {
                "id": "8011113",
                "name": "Berlin Südkreuz",
                "location": { "latitude": 52.47623, "longitude": 13.365863 }
            },
            {
                "id": "8011155",
                "name": "Berlin Hbf",
                "location": { "latitude": 52.525, "longitude": 13.369 }
            }
        ]"#;

        let stops = HafasTransitClient::parse_locations_response(json).unwrap();
        assert_eq!(stops.len(), 2);
        assert_eq!(stops[0].name, "Berlin Südkreuz");
        assert_eq!(stops[0].id, "8011113");
        assert!((stops[0].latitude.unwrap() - 52.47623).abs() < 0.001);
    }

    #[test]
    fn test_parse_empty_journeys() {
        let json = r#"{ "journeys": [] }"#;
        let result = HafasTransitClient::parse_journeys_response(json).unwrap();
        assert!(result.journeys.is_empty());
    }

    #[test]
    fn test_parse_invalid_json() {
        let result = HafasTransitClient::parse_journeys_response("not json");
        assert!(result.is_err());
    }

    #[test]
    fn test_product_params() {
        let config = TransitConfig::default();
        let client = HafasTransitClient::new(&config).unwrap();
        let params = client.product_params();
        assert!(!params.is_empty());
        // Default: bus, suburban, subway, tram, regional are true
        assert!(params.contains(&("bus", "true")));
        assert!(params.contains(&("suburban", "true")));
        // national is false by default
        assert!(params.contains(&("national", "false")));
    }
}
