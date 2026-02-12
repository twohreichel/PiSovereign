//! Public transit connection search handler

use chrono::Utc;
use tracing::{info, warn};

use super::{AgentService, ExecutionResult};
use crate::{error::ApplicationError, ports::format_connections};

impl AgentService {
    /// Handle searching for transit connections
    pub(super) async fn handle_search_transit(
        &self,
        from: &str,
        to: &str,
        departure: Option<&str>,
    ) -> Result<ExecutionResult, ApplicationError> {
        let Some(ref transit_service) = self.transit_service else {
            return Ok(ExecutionResult {
                success: false,
                response: format!(
                    "🚆 Transit service not yet configured. Cannot search: {from} → {to}"
                ),
            });
        };

        // Determine the origin - use home location if "from" is empty
        let from_location = if from.is_empty() {
            match &self.home_location {
                Some(loc) => *loc,
                None => {
                    return Ok(ExecutionResult {
                        success: false,
                        response:
                            "🚆 Keine Startadresse angegeben und keine Heimadresse konfiguriert.\n\
                                   Bitte geben Sie einen Startpunkt an."
                                .to_string(),
                    });
                },
            }
        } else {
            // Geocode the address
            match transit_service.geocode_address(from).await {
                Ok(Some(loc)) => loc,
                Ok(None) => {
                    return Ok(ExecutionResult {
                        success: false,
                        response: format!(
                            "📍 Startadresse konnte nicht gefunden werden: **{from}**\n\n\
                             Bitte versuchen Sie eine genauere Adresse."
                        ),
                    });
                },
                Err(e) => {
                    warn!(error = %e, address = %from, "Failed to geocode from address");
                    return Ok(ExecutionResult {
                        success: false,
                        response: format!("❌ Fehler bei der Geolokalisierung: {e}"),
                    });
                },
            }
        };

        // Parse departure time if provided
        #[allow(clippy::option_if_let_else)] // Complex parsing chain doesn't simplify well
        let departure_time = if let Some(dep) = departure {
            chrono::DateTime::parse_from_rfc3339(dep)
                .map(|dt| dt.with_timezone(&Utc))
                .or_else(|_| {
                    chrono::NaiveDateTime::parse_from_str(dep, "%Y-%m-%dT%H:%M:%S")
                        .or_else(|_| chrono::NaiveDateTime::parse_from_str(dep, "%Y-%m-%dT%H:%M"))
                        .map(|ndt| ndt.and_utc())
                })
                .ok()
        } else {
            None
        };

        info!(from = %from, to = %to, departure = ?departure_time, "Searching transit connections");

        // Search for connections (default to 5 results)
        match transit_service
            .find_connections_to_address(&from_location, to, departure_time, 5)
            .await
        {
            Ok(connections) => {
                if connections.is_empty() {
                    return Ok(ExecutionResult {
                        success: true,
                        response: format!(
                            "🚆 Keine Verbindungen gefunden.\n\n\
                             **Von:** {}\n\
                             **Nach:** {to}\n\n\
                             Versuchen Sie einen anderen Zeitpunkt oder prüfen Sie die Adressen.",
                            if from.is_empty() { "Heimadresse" } else { from }
                        ),
                    });
                }

                let response = format!(
                    "🚆 **ÖPNV-Verbindungen nach {to}**\n\n\
                     **Von:** {}\n\n\
                     {}",
                    if from.is_empty() { "Heimadresse" } else { from },
                    format_connections(&connections)
                );

                Ok(ExecutionResult {
                    success: true,
                    response,
                })
            },
            Err(e) => {
                warn!(error = %e, "Transit search failed");
                Ok(ExecutionResult {
                    success: false,
                    response: format!(
                        "❌ Fehler bei der Verbindungssuche: {e}\n\n\
                         Bitte versuchen Sie es später erneut."
                    ),
                })
            },
        }
    }
}
