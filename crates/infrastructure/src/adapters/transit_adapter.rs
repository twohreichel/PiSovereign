//! Transit adapter - Implements TransitPort using integration_transit

use application::error::ApplicationError;
use application::ports::{
    TransitConnection, TransitLeg, TransitMode, TransitPort, TransitQuery,
};
use async_trait::async_trait;
use domain::value_objects::GeoLocation;
use chrono::{DateTime, Utc};
use integration_transit::{
    GeocodingClient, HafasTransitClient, NominatimGeocodingClient, TransitClient,
    TransitMode as IntegrationMode,
};
use tracing::{debug, instrument, warn};

use super::{CircuitBreaker, CircuitBreakerConfig};

/// Adapter for public transit services using HAFAS (transport.rest) and Nominatim
pub struct TransitAdapter {
    transit_client: HafasTransitClient,
    geocoding_client: NominatimGeocodingClient,
    circuit_breaker: Option<CircuitBreaker>,
}

impl std::fmt::Debug for TransitAdapter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TransitAdapter")
            .field("transit_client", &"HafasTransitClient")
            .field("geocoding_client", &"NominatimGeocodingClient")
            .field(
                "circuit_breaker",
                &self.circuit_breaker.as_ref().map(CircuitBreaker::name),
            )
            .finish()
    }
}

impl TransitAdapter {
    /// Create a new transit adapter
    ///
    /// # Errors
    ///
    /// Returns an error if the HTTP clients fail to initialize.
    pub fn new(
        transit_client: HafasTransitClient,
        geocoding_client: NominatimGeocodingClient,
    ) -> Self {
        Self {
            transit_client,
            geocoding_client,
            circuit_breaker: None,
        }
    }

    /// Enable circuit breaker with default configuration
    #[must_use]
    pub fn with_circuit_breaker(mut self) -> Self {
        self.circuit_breaker = Some(CircuitBreaker::new("transit"));
        self
    }

    /// Enable circuit breaker with custom configuration
    #[must_use]
    pub fn with_circuit_breaker_config(mut self, config: CircuitBreakerConfig) -> Self {
        self.circuit_breaker = Some(CircuitBreaker::with_config("transit", config));
        self
    }

    /// Check circuit and return error if open
    fn check_circuit(&self) -> Result<(), ApplicationError> {
        if let Some(ref cb) = self.circuit_breaker {
            if cb.is_open() {
                return Err(ApplicationError::ExternalService(
                    "Transit service circuit breaker is open".to_string(),
                ));
            }
        }
        Ok(())
    }

    /// Convert an integration transit mode to app-layer transit mode
    fn convert_mode(mode: IntegrationMode) -> TransitMode {
        match mode {
            IntegrationMode::NationalExpress => TransitMode::NationalExpress,
            IntegrationMode::National => TransitMode::National,
            IntegrationMode::Regional => TransitMode::Regional,
            IntegrationMode::Suburban => TransitMode::Suburban,
            IntegrationMode::Subway => TransitMode::Subway,
            IntegrationMode::Tram => TransitMode::Tram,
            IntegrationMode::Bus => TransitMode::Bus,
            IntegrationMode::Ferry => TransitMode::Ferry,
            IntegrationMode::Walking => TransitMode::Walking,
            IntegrationMode::Unknown => TransitMode::Unknown,
        }
    }
}

#[async_trait]
impl TransitPort for TransitAdapter {
    #[instrument(skip(self))]
    async fn search_connections(
        &self,
        query: &TransitQuery,
    ) -> Result<Vec<TransitConnection>, ApplicationError> {
        self.check_circuit()?;

        let result = self
            .transit_client
            .search_journeys(
                query.from.latitude(),
                query.from.longitude(),
                query.to.latitude(),
                query.to.longitude(),
                query.departure,
                query.max_results,
            )
            .await
            .map_err(|e| {
                ApplicationError::ExternalService(format!("Transit search failed: {e}"))
            })?;

        let connections = result
            .journeys
            .into_iter()
            .map(|journey| {
                let legs: Vec<TransitLeg> = journey
                    .legs
                    .iter()
                    .map(|leg| {
                        let mode = Self::convert_mode(leg.mode());
                        TransitLeg {
                            mode,
                            line_name: leg.line.as_ref().map(|l| l.name.clone()),
                            direction: leg
                                .line
                                .as_ref()
                                .and_then(|l| l.direction.clone()),
                            from_stop: leg.origin.name.clone(),
                            to_stop: leg.destination.name.clone(),
                            departure: leg.departure,
                            arrival: leg.arrival,
                            platform: leg.departure_platform.clone(),
                            delay_seconds: leg.departure_delay,
                        }
                    })
                    .collect();

                let delay_info = journey
                    .legs
                    .first()
                    .and_then(|l| {
                        l.departure_delay.and_then(|d| {
                            if d > 0 {
                                Some(format!("⚠️ +{}min", d / 60))
                            } else {
                                None
                            }
                        })
                    });

                TransitConnection {
                    departure_time: journey
                        .legs
                        .first()
                        .map(|l| l.departure)
                        .unwrap_or_else(Utc::now),
                    arrival_time: journey
                        .legs
                        .last()
                        .map(|l| l.arrival)
                        .unwrap_or_else(Utc::now),
                    duration_minutes: journey.duration_minutes(),
                    transfers: journey.transfers(),
                    legs,
                    delay_info,
                }
            })
            .collect();

        Ok(connections)
    }

    #[instrument(skip(self))]
    async fn find_connections_to_address(
        &self,
        from: &GeoLocation,
        to_address: &str,
        departure: Option<DateTime<Utc>>,
        max_results: u8,
    ) -> Result<Vec<TransitConnection>, ApplicationError> {
        debug!(%to_address, "Geocoding destination address");

        let to_location = self
            .geocoding_client
            .geocode(to_address)
            .await
            .map_err(|e| {
                warn!(%to_address, %e, "Failed to geocode address");
                ApplicationError::ExternalService(format!(
                    "Failed to geocode '{to_address}': {e}"
                ))
            })?;

        let query = TransitQuery {
            from: from.clone(),
            to: to_location,
            departure,
            max_results,
        };

        self.search_connections(&query).await
    }

    #[instrument(skip(self))]
    async fn geocode_address(
        &self,
        address: &str,
    ) -> Result<Option<GeoLocation>, ApplicationError> {
        debug!(%address, "Geocoding address");

        match self.geocoding_client.geocode(address).await {
            Ok(location) => Ok(Some(location)),
            Err(e) => {
                warn!(%address, %e, "Failed to geocode address");
                Ok(None)
            },
        }
    }

    async fn is_available(&self) -> bool {
        self.transit_client.is_healthy().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_convert_mode() {
        assert_eq!(
            TransitAdapter::convert_mode(IntegrationMode::Suburban),
            TransitMode::Suburban
        );
        assert_eq!(
            TransitAdapter::convert_mode(IntegrationMode::Subway),
            TransitMode::Subway
        );
        assert_eq!(
            TransitAdapter::convert_mode(IntegrationMode::Walking),
            TransitMode::Walking
        );
        assert_eq!(
            TransitAdapter::convert_mode(IntegrationMode::Bus),
            TransitMode::Bus
        );
        assert_eq!(
            TransitAdapter::convert_mode(IntegrationMode::NationalExpress),
            TransitMode::NationalExpress
        );
    }
}
