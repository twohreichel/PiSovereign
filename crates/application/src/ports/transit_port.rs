//! Public transit service port
//!
//! Defines the interface for public transit routing and connection search.
//! Adapters in the infrastructure layer implement this port using transit APIs.

use std::fmt;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use domain::value_objects::GeoLocation;
#[cfg(test)]
use mockall::automock;
use serde::{Deserialize, Serialize};

use crate::error::ApplicationError;

/// A transit connection from origin to destination
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransitConnection {
    /// Departure time from origin
    pub departure_time: DateTime<Utc>,
    /// Arrival time at destination
    pub arrival_time: DateTime<Utc>,
    /// Total travel duration in minutes
    pub duration_minutes: u32,
    /// Number of transfers
    pub transfers: u8,
    /// Individual legs of the connection
    pub legs: Vec<TransitLeg>,
    /// Summary of any delays
    pub delay_info: Option<String>,
}

impl TransitConnection {
    /// Format as a compact summary line
    #[must_use]
    pub fn format_summary(&self) -> String {
        let dep = self.departure_time.format("%H:%M");
        let arr = self.arrival_time.format("%H:%M");
        let dur = self.duration_minutes;
        let transfers = self.transfers;

        let lines: Vec<String> = self
            .legs
            .iter()
            .filter(|l| l.mode != TransitMode::Walking)
            .filter_map(|l| l.line_name.clone())
            .collect();
        let route = lines.join(" → ");

        let delay = self
            .delay_info
            .as_deref()
            .map(|d| format!(" {d}"))
            .unwrap_or_default();

        if transfers == 0 {
            format!("🕐 {dep} → {arr} ({dur}min) {route}{delay}")
        } else {
            format!(
                "🕐 {dep} → {arr} ({dur}min, {transfers}x ↔️) {route}{delay}"
            )
        }
    }
}

impl fmt::Display for TransitConnection {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.format_summary())
    }
}

/// A single leg of a transit connection
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransitLeg {
    /// Transport mode (train, bus, walking, etc.)
    pub mode: TransitMode,
    /// Line name (e.g., "S5", "Bus 248")
    pub line_name: Option<String>,
    /// Travel direction
    pub direction: Option<String>,
    /// Departure stop name
    pub from_stop: String,
    /// Arrival stop name
    pub to_stop: String,
    /// Departure time
    pub departure: DateTime<Utc>,
    /// Arrival time
    pub arrival: DateTime<Utc>,
    /// Platform number
    pub platform: Option<String>,
    /// Delay in seconds (None = unknown, 0 = on time)
    pub delay_seconds: Option<i64>,
}

impl TransitLeg {
    /// Format as a detail line with emoji
    #[must_use]
    pub fn format_detail(&self) -> String {
        let emoji = self.mode.emoji();
        let dep = self.departure.format("%H:%M");
        let arr = self.arrival.format("%H:%M");

        if self.mode == TransitMode::Walking {
            return format!("{emoji} {dep}–{arr} Walk");
        }

        let line = self
            .line_name
            .as_deref()
            .unwrap_or("?");
        let dir = self
            .direction
            .as_deref()
            .map(|d| format!(" → {d}"))
            .unwrap_or_default();
        let plat = self
            .platform
            .as_deref()
            .map(|p| format!(" Gl.{p}"))
            .unwrap_or_default();
        let delay = match self.delay_seconds {
            Some(s) if s > 0 => format!(" ⚠️ +{}min", s / 60),
            _ => String::new(),
        };

        format!("{emoji} {dep}–{arr} *{line}*{dir}{plat}{delay}")
    }
}

/// Transit mode classification
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TransitMode {
    /// ICE / IC / EC
    NationalExpress,
    /// IC / EC
    National,
    /// RE / RB
    Regional,
    /// S-Bahn
    Suburban,
    /// U-Bahn
    Subway,
    /// Tram / Straßenbahn
    Tram,
    /// Bus
    Bus,
    /// Ferry
    Ferry,
    /// Walking transfer
    Walking,
    /// Unknown transport mode
    Unknown,
}

impl TransitMode {
    /// Emoji representation for message formatting
    #[must_use]
    pub const fn emoji(&self) -> &'static str {
        match self {
            Self::NationalExpress | Self::National | Self::Regional => "🚆",
            Self::Suburban => "🚈",
            Self::Subway => "🚇",
            Self::Tram => "🚊",
            Self::Bus => "🚌",
            Self::Ferry => "⛴️",
            Self::Walking => "🚶",
            Self::Unknown => "🚋",
        }
    }

    /// Human-readable label
    #[must_use]
    pub const fn label(&self) -> &'static str {
        match self {
            Self::NationalExpress => "ICE",
            Self::National => "IC/EC",
            Self::Regional => "Regional",
            Self::Suburban => "S-Bahn",
            Self::Subway => "U-Bahn",
            Self::Tram => "Tram",
            Self::Bus => "Bus",
            Self::Ferry => "Ferry",
            Self::Walking => "Walk",
            Self::Unknown => "Transit",
        }
    }
}

impl fmt::Display for TransitMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.label())
    }
}

/// Options for a transit search query
#[derive(Debug, Clone)]
pub struct TransitQuery {
    /// Origin coordinates
    pub from: GeoLocation,
    /// Destination coordinates
    pub to: GeoLocation,
    /// Departure time (None = now)
    pub departure: Option<DateTime<Utc>>,
    /// Maximum number of results
    pub max_results: u8,
}

impl TransitQuery {
    /// Create a new transit query
    #[must_use]
    pub fn new(from: GeoLocation, to: GeoLocation) -> Self {
        Self {
            from,
            to,
            departure: None,
            max_results: 3,
        }
    }

    /// Set departure time
    #[must_use]
    pub const fn with_departure(mut self, departure: DateTime<Utc>) -> Self {
        self.departure = Some(departure);
        self
    }

    /// Set maximum number of results
    #[must_use]
    pub const fn with_max_results(mut self, max: u8) -> Self {
        self.max_results = max;
        self
    }
}

/// Port for public transit operations
#[cfg_attr(test, automock)]
#[async_trait]
pub trait TransitPort: Send + Sync {
    /// Search for transit connections between two coordinate points
    async fn search_connections(
        &self,
        query: &TransitQuery,
    ) -> Result<Vec<TransitConnection>, ApplicationError>;

    /// Search for connections from coordinates to a named address
    ///
    /// Geocodes the destination address internally.
    async fn find_connections_to_address(
        &self,
        from: &GeoLocation,
        to_address: &str,
        departure: Option<DateTime<Utc>>,
        max_results: u8,
    ) -> Result<Vec<TransitConnection>, ApplicationError>;

    /// Check if the transit service is available
    async fn is_available(&self) -> bool;
}

/// Format a list of connections as a compact multi-line string
///
/// Suitable for embedding in reminder messages.
#[must_use]
pub fn format_connections(connections: &[TransitConnection]) -> String {
    if connections.is_empty() {
        return String::from("No connections found");
    }

    connections
        .iter()
        .map(TransitConnection::format_summary)
        .collect::<Vec<_>>()
        .join("\n")
}

/// Format connections with detailed leg information
#[must_use]
pub fn format_connections_detailed(connections: &[TransitConnection]) -> String {
    if connections.is_empty() {
        return String::from("No connections found");
    }

    connections
        .iter()
        .enumerate()
        .map(|(i, conn)| {
            let header = format!("*Option {}:* {}", i + 1, conn.format_summary());
            let legs: Vec<String> = conn
                .legs
                .iter()
                .map(TransitLeg::format_detail)
                .collect();
            format!("{header}\n{}", legs.join("\n"))
        })
        .collect::<Vec<_>>()
        .join("\n\n")
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;

    use super::*;

    fn _assert_object_safe(_: &dyn TransitPort) {}

    #[test]
    fn trait_is_send_sync() {
        fn assert_send_sync<T: Send + Sync + ?Sized>() {}
        assert_send_sync::<dyn TransitPort>();
    }

    fn sample_connection() -> TransitConnection {
        let dep = Utc.with_ymd_and_hms(2026, 2, 11, 8, 0, 0).unwrap();
        let arr = Utc.with_ymd_and_hms(2026, 2, 11, 8, 25, 0).unwrap();
        TransitConnection {
            departure_time: dep,
            arrival_time: arr,
            duration_minutes: 25,
            transfers: 1,
            legs: vec![
                TransitLeg {
                    mode: TransitMode::Suburban,
                    line_name: Some("S5".to_string()),
                    direction: Some("Westkreuz".to_string()),
                    from_stop: "Alexanderplatz".to_string(),
                    to_stop: "Friedrichstraße".to_string(),
                    departure: dep,
                    arrival: Utc.with_ymd_and_hms(2026, 2, 11, 8, 10, 0).unwrap(),
                    platform: Some("3".to_string()),
                    delay_seconds: Some(120),
                },
                TransitLeg {
                    mode: TransitMode::Subway,
                    line_name: Some("U6".to_string()),
                    direction: Some("Alt-Tegel".to_string()),
                    from_stop: "Friedrichstraße".to_string(),
                    to_stop: "Naturkundemuseum".to_string(),
                    departure: Utc.with_ymd_and_hms(2026, 2, 11, 8, 15, 0).unwrap(),
                    arrival: arr,
                    platform: None,
                    delay_seconds: Some(0),
                },
            ],
            delay_info: Some("⚠️ +2min".to_string()),
        }
    }

    #[test]
    fn test_connection_format_summary() {
        let conn = sample_connection();
        let summary = conn.format_summary();
        assert!(summary.contains("08:00"));
        assert!(summary.contains("08:25"));
        assert!(summary.contains("25min"));
        assert!(summary.contains("1x"));
        assert!(summary.contains("S5"));
        assert!(summary.contains("U6"));
    }

    #[test]
    fn test_connection_no_transfers() {
        let dep = Utc.with_ymd_and_hms(2026, 2, 11, 8, 0, 0).unwrap();
        let arr = Utc.with_ymd_and_hms(2026, 2, 11, 8, 15, 0).unwrap();
        let conn = TransitConnection {
            departure_time: dep,
            arrival_time: arr,
            duration_minutes: 15,
            transfers: 0,
            legs: vec![TransitLeg {
                mode: TransitMode::Bus,
                line_name: Some("248".to_string()),
                direction: None,
                from_stop: "A".to_string(),
                to_stop: "B".to_string(),
                departure: dep,
                arrival: arr,
                platform: None,
                delay_seconds: None,
            }],
            delay_info: None,
        };
        let summary = conn.format_summary();
        assert!(!summary.contains("↔️")); // no transfer indicator
    }

    #[test]
    fn test_leg_format_detail_transport() {
        let dep = Utc.with_ymd_and_hms(2026, 2, 11, 8, 0, 0).unwrap();
        let arr = Utc.with_ymd_and_hms(2026, 2, 11, 8, 10, 0).unwrap();
        let leg = TransitLeg {
            mode: TransitMode::Suburban,
            line_name: Some("S5".to_string()),
            direction: Some("Westkreuz".to_string()),
            from_stop: "Alex".to_string(),
            to_stop: "Zoo".to_string(),
            departure: dep,
            arrival: arr,
            platform: Some("3".to_string()),
            delay_seconds: Some(300),
        };
        let detail = leg.format_detail();
        assert!(detail.contains("🚈"));
        assert!(detail.contains("S5"));
        assert!(detail.contains("Gl.3"));
        assert!(detail.contains("+5min"));
    }

    #[test]
    fn test_leg_format_detail_walking() {
        let dep = Utc.with_ymd_and_hms(2026, 2, 11, 8, 0, 0).unwrap();
        let arr = Utc.with_ymd_and_hms(2026, 2, 11, 8, 5, 0).unwrap();
        let leg = TransitLeg {
            mode: TransitMode::Walking,
            line_name: None,
            direction: None,
            from_stop: "A".to_string(),
            to_stop: "B".to_string(),
            departure: dep,
            arrival: arr,
            platform: None,
            delay_seconds: None,
        };
        assert!(leg.format_detail().contains("🚶"));
        assert!(leg.format_detail().contains("Walk"));
    }

    #[test]
    fn test_transit_mode_display() {
        assert_eq!(TransitMode::Suburban.to_string(), "S-Bahn");
        assert_eq!(TransitMode::NationalExpress.to_string(), "ICE");
        assert_eq!(TransitMode::Walking.to_string(), "Walk");
    }

    #[test]
    fn test_transit_query_builder() {
        let from = GeoLocation::new(52.52, 13.41).unwrap();
        let to = GeoLocation::new(52.50, 13.33).unwrap();
        let dep = Utc::now();

        let query = TransitQuery::new(from, to)
            .with_departure(dep)
            .with_max_results(5);

        assert_eq!(query.departure, Some(dep));
        assert_eq!(query.max_results, 5);
    }

    #[test]
    fn test_format_connections_empty() {
        assert_eq!(format_connections(&[]), "No connections found");
    }

    #[test]
    fn test_format_connections() {
        let connections = vec![sample_connection()];
        let formatted = format_connections(&connections);
        assert!(formatted.contains("08:00"));
        assert!(formatted.contains("S5"));
    }

    #[test]
    fn test_format_connections_detailed() {
        let connections = vec![sample_connection()];
        let formatted = format_connections_detailed(&connections);
        assert!(formatted.contains("Option 1"));
        assert!(formatted.contains("🚈"));
        assert!(formatted.contains("🚇"));
    }
}
