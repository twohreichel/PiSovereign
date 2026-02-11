//! Transit data models
//!
//! Typed representations of public transit journeys, legs, stops, and lines
//! as returned by the transport.rest HAFAS API.

use std::fmt;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// A complete journey from origin to destination, consisting of one or more legs
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Journey {
    /// Individual legs (segments) of the journey
    pub legs: Vec<Leg>,
    /// Token to refresh this journey for real-time updates
    #[serde(skip_serializing_if = "Option::is_none")]
    pub refresh_token: Option<String>,
}

impl Journey {
    /// Total travel duration in minutes
    #[must_use]
    pub fn duration_minutes(&self) -> u32 {
        let Some(first) = self.legs.first() else {
            return 0;
        };
        let Some(last) = self.legs.last() else {
            return 0;
        };
        let duration = last.arrival - first.departure;
        duration.num_minutes().unsigned_abs() as u32
    }

    /// Number of transfers (legs - 1, excluding walking legs)
    #[must_use]
    pub fn transfers(&self) -> u8 {
        let transport_legs = self
            .legs
            .iter()
            .filter(|leg| !leg.walking)
            .count();
        transport_legs.saturating_sub(1) as u8
    }

    /// Format as a compact one-line summary
    #[must_use]
    pub fn format_summary(&self) -> String {
        let Some(first) = self.legs.first() else {
            return String::from("No journey data");
        };
        let Some(last) = self.legs.last() else {
            return String::from("No journey data");
        };

        let dep = first.departure.format("%H:%M");
        let arr = last.arrival.format("%H:%M");
        let dur = self.duration_minutes();
        let transfers = self.transfers();

        let lines: Vec<String> = self
            .legs
            .iter()
            .filter(|l| !l.walking)
            .filter_map(|l| l.line.as_ref().map(|li| li.name.clone()))
            .collect();
        let route = lines.join(" → ");

        let delay_info = first.format_delay();

        format!(
            "{dep} → {arr} ({dur}min, {transfers}x umsteigen) {route}{delay_info}"
        )
    }
}

impl fmt::Display for Journey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.format_summary())
    }
}

/// A single leg (segment) of a journey
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Leg {
    /// Origin stop
    pub origin: Stop,
    /// Destination stop
    pub destination: Stop,
    /// Actual departure time (includes delay)
    pub departure: DateTime<Utc>,
    /// Scheduled departure time
    pub planned_departure: DateTime<Utc>,
    /// Actual arrival time (includes delay)
    pub arrival: DateTime<Utc>,
    /// Scheduled arrival time
    pub planned_arrival: DateTime<Utc>,
    /// Departure delay in seconds (None = unknown, 0 = on time)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub departure_delay: Option<i64>,
    /// Arrival delay in seconds
    #[serde(skip_serializing_if = "Option::is_none")]
    pub arrival_delay: Option<i64>,
    /// Departure platform
    #[serde(skip_serializing_if = "Option::is_none")]
    pub departure_platform: Option<String>,
    /// Arrival platform
    #[serde(skip_serializing_if = "Option::is_none")]
    pub arrival_platform: Option<String>,
    /// Line information (None for walking legs)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line: Option<LineInfo>,
    /// Whether this is a walking transfer leg
    #[serde(default)]
    pub walking: bool,
    /// Walking distance in meters (only for walking legs)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub distance: Option<u32>,
}

impl Leg {
    /// Get the transit mode for this leg
    #[must_use]
    pub fn mode(&self) -> TransitMode {
        if self.walking {
            return TransitMode::Walking;
        }
        self.line
            .as_ref()
            .map(|l| TransitMode::from_product(&l.product))
            .unwrap_or(TransitMode::Unknown)
    }

    /// Format the delay information as a human-readable string
    #[must_use]
    pub fn format_delay(&self) -> String {
        match self.departure_delay {
            Some(0) => String::new(),
            Some(secs) if secs > 0 => {
                let mins = secs / 60;
                format!(" ⚠️ +{mins}min")
            }
            Some(secs) if secs < 0 => {
                let mins = (-secs) / 60;
                format!(" 🟢 -{mins}min")
            }
            _ => String::new(),
        }
    }

    /// Format this leg as a detailed line
    #[must_use]
    pub fn format_detail(&self) -> String {
        let emoji = self.mode().emoji();
        let dep = self.departure.format("%H:%M");
        let arr = self.arrival.format("%H:%M");

        if self.walking {
            let dist = self.distance.unwrap_or(0);
            return format!("{emoji} {dep}–{arr} Walk ({dist}m)");
        }

        let line_name = self
            .line
            .as_ref()
            .map_or("?", |l| l.name.as_str());
        let direction = self
            .line
            .as_ref()
            .and_then(|l| l.direction.as_deref())
            .unwrap_or("");
        let platform = self
            .departure_platform
            .as_deref()
            .map(|p| format!(" Gl.{p}"))
            .unwrap_or_default();
        let delay = self.format_delay();

        format!(
            "{emoji} {dep}–{arr} *{line_name}* → {direction}{platform}{delay}"
        )
    }
}

impl fmt::Display for Leg {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.format_detail())
    }
}

/// A transit stop (station, bus stop, etc.)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Stop {
    /// Unique stop identifier
    pub id: String,
    /// Human-readable stop name
    pub name: String,
    /// Latitude coordinate
    #[serde(skip_serializing_if = "Option::is_none")]
    pub latitude: Option<f64>,
    /// Longitude coordinate
    #[serde(skip_serializing_if = "Option::is_none")]
    pub longitude: Option<f64>,
}

impl Stop {
    /// Create a new stop
    #[must_use]
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            latitude: None,
            longitude: None,
        }
    }

    /// Create a stop with coordinates
    #[must_use]
    pub fn with_coords(mut self, latitude: f64, longitude: f64) -> Self {
        self.latitude = Some(latitude);
        self.longitude = Some(longitude);
        self
    }
}

impl fmt::Display for Stop {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.name)
    }
}

/// Information about a transit line (train, bus, etc.)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LineInfo {
    /// Display name (e.g., "ICE 1601", "S5", "Bus 248")
    pub name: String,
    /// Product type from HAFAS (e.g., "nationalExpress", "suburban", "bus")
    pub product: String,
    /// Transport mode (e.g., "train", "bus")
    pub mode: String,
    /// Travel direction
    #[serde(skip_serializing_if = "Option::is_none")]
    pub direction: Option<String>,
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
    /// Map a HAFAS product string to a transit mode
    #[must_use]
    pub fn from_product(product: &str) -> Self {
        match product {
            "nationalExpress" => Self::NationalExpress,
            "national" => Self::National,
            "regionalExpress" | "regional" => Self::Regional,
            "suburban" => Self::Suburban,
            "subway" => Self::Subway,
            "tram" => Self::Tram,
            "bus" => Self::Bus,
            "ferry" => Self::Ferry,
            _ => Self::Unknown,
        }
    }

    /// Emoji representation for rich message formatting
    #[must_use]
    pub const fn emoji(&self) -> &'static str {
        match self {
            Self::NationalExpress | Self::National => "🚆",
            Self::Regional => "🚆",
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

/// Response from a journey search
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransitResponse {
    /// Found journeys
    pub journeys: Vec<Journey>,
    /// Pagination token for later journeys
    #[serde(skip_serializing_if = "Option::is_none")]
    pub earlier_ref: Option<String>,
    /// Pagination token for earlier journeys
    #[serde(skip_serializing_if = "Option::is_none")]
    pub later_ref: Option<String>,
}

impl TransitResponse {
    /// Create a response with no results
    #[must_use]
    pub fn empty() -> Self {
        Self {
            journeys: Vec::new(),
            earlier_ref: None,
            later_ref: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;

    use super::*;

    fn sample_stop(id: &str, name: &str) -> Stop {
        Stop::new(id, name).with_coords(52.52, 13.37)
    }

    fn sample_line(name: &str, product: &str) -> LineInfo {
        LineInfo {
            name: name.to_string(),
            product: product.to_string(),
            mode: "train".to_string(),
            direction: Some("Endstation".to_string()),
        }
    }

    fn sample_leg(walking: bool) -> Leg {
        let dep = Utc.with_ymd_and_hms(2026, 2, 11, 8, 0, 0).unwrap();
        let arr = Utc.with_ymd_and_hms(2026, 2, 11, 8, 30, 0).unwrap();
        Leg {
            origin: sample_stop("1", "Start"),
            destination: sample_stop("2", "End"),
            departure: dep,
            planned_departure: dep,
            arrival: arr,
            planned_arrival: arr,
            departure_delay: Some(0),
            arrival_delay: Some(0),
            departure_platform: Some("3".to_string()),
            arrival_platform: Some("5".to_string()),
            line: if walking {
                None
            } else {
                Some(sample_line("S5", "suburban"))
            },
            walking,
            distance: if walking { Some(200) } else { None },
        }
    }

    #[test]
    fn test_journey_duration() {
        let journey = Journey {
            legs: vec![sample_leg(false)],
            refresh_token: None,
        };
        assert_eq!(journey.duration_minutes(), 30);
    }

    #[test]
    fn test_journey_duration_empty() {
        let journey = Journey {
            legs: vec![],
            refresh_token: None,
        };
        assert_eq!(journey.duration_minutes(), 0);
    }

    #[test]
    fn test_journey_transfers() {
        let mut leg2 = sample_leg(false);
        leg2.line = Some(sample_line("U2", "subway"));
        let journey = Journey {
            legs: vec![sample_leg(false), sample_leg(true), leg2],
            refresh_token: None,
        };
        // 2 transport legs, 1 walking → 1 transfer
        assert_eq!(journey.transfers(), 1);
    }

    #[test]
    fn test_journey_no_transfers() {
        let journey = Journey {
            legs: vec![sample_leg(false)],
            refresh_token: None,
        };
        assert_eq!(journey.transfers(), 0);
    }

    #[test]
    fn test_journey_format_summary() {
        let journey = Journey {
            legs: vec![sample_leg(false)],
            refresh_token: None,
        };
        let summary = journey.format_summary();
        assert!(summary.contains("08:00"));
        assert!(summary.contains("08:30"));
        assert!(summary.contains("30min"));
        assert!(summary.contains("S5"));
    }

    #[test]
    fn test_leg_mode_walking() {
        let leg = sample_leg(true);
        assert_eq!(leg.mode(), TransitMode::Walking);
    }

    #[test]
    fn test_leg_mode_suburban() {
        let leg = sample_leg(false);
        assert_eq!(leg.mode(), TransitMode::Suburban);
    }

    #[test]
    fn test_leg_format_delay_on_time() {
        let leg = sample_leg(false);
        assert!(leg.format_delay().is_empty());
    }

    #[test]
    fn test_leg_format_delay_late() {
        let mut leg = sample_leg(false);
        leg.departure_delay = Some(300); // 5 minutes
        assert!(leg.format_delay().contains("+5min"));
    }

    #[test]
    fn test_leg_format_delay_early() {
        let mut leg = sample_leg(false);
        leg.departure_delay = Some(-120); // 2 minutes early
        assert!(leg.format_delay().contains("-2min"));
    }

    #[test]
    fn test_leg_format_detail_transport() {
        let leg = sample_leg(false);
        let detail = leg.format_detail();
        assert!(detail.contains("🚈")); // S-Bahn emoji
        assert!(detail.contains("S5"));
        assert!(detail.contains("Gl.3"));
    }

    #[test]
    fn test_leg_format_detail_walking() {
        let leg = sample_leg(true);
        let detail = leg.format_detail();
        assert!(detail.contains("🚶"));
        assert!(detail.contains("200m"));
    }

    #[test]
    fn test_transit_mode_from_product() {
        assert_eq!(
            TransitMode::from_product("nationalExpress"),
            TransitMode::NationalExpress
        );
        assert_eq!(TransitMode::from_product("suburban"), TransitMode::Suburban);
        assert_eq!(TransitMode::from_product("subway"), TransitMode::Subway);
        assert_eq!(TransitMode::from_product("bus"), TransitMode::Bus);
        assert_eq!(TransitMode::from_product("tram"), TransitMode::Tram);
        assert_eq!(TransitMode::from_product("ferry"), TransitMode::Ferry);
        assert_eq!(TransitMode::from_product("unknown"), TransitMode::Unknown);
    }

    #[test]
    fn test_transit_mode_emoji() {
        assert_eq!(TransitMode::Suburban.emoji(), "🚈");
        assert_eq!(TransitMode::Subway.emoji(), "🚇");
        assert_eq!(TransitMode::Bus.emoji(), "🚌");
        assert_eq!(TransitMode::Walking.emoji(), "🚶");
    }

    #[test]
    fn test_transit_mode_label() {
        assert_eq!(TransitMode::Suburban.label(), "S-Bahn");
        assert_eq!(TransitMode::NationalExpress.label(), "ICE");
    }

    #[test]
    fn test_stop_display() {
        let stop = sample_stop("123", "Berlin Hbf");
        assert_eq!(stop.to_string(), "Berlin Hbf");
    }

    #[test]
    fn test_transit_response_empty() {
        let resp = TransitResponse::empty();
        assert!(resp.journeys.is_empty());
        assert!(resp.earlier_ref.is_none());
    }
}
