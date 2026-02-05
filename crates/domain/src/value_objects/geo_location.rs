//! Geographic location value object

use serde::{Deserialize, Serialize};
use std::fmt;

/// A geographic location with latitude and longitude
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct GeoLocation {
    /// Latitude in degrees (-90 to 90)
    latitude: f64,
    /// Longitude in degrees (-180 to 180)
    longitude: f64,
}

/// Error type for invalid coordinates
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InvalidCoordinates;

impl fmt::Display for InvalidCoordinates {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Invalid coordinates: latitude must be -90 to 90, longitude must be -180 to 180"
        )
    }
}

impl std::error::Error for InvalidCoordinates {}

impl GeoLocation {
    /// Create a new location with validation
    ///
    /// # Errors
    ///
    /// Returns `InvalidCoordinates` if latitude is not in [-90, 90]
    /// or longitude is not in [-180, 180]
    pub fn new(latitude: f64, longitude: f64) -> Result<Self, InvalidCoordinates> {
        if !(-90.0..=90.0).contains(&latitude) || !(-180.0..=180.0).contains(&longitude) {
            return Err(InvalidCoordinates);
        }
        Ok(Self {
            latitude,
            longitude,
        })
    }

    /// Create a location without validation (for trusted sources)
    ///
    /// # Safety
    ///
    /// Caller must ensure latitude is in [-90, 90] and longitude in [-180, 180]
    #[must_use]
    pub const fn new_unchecked(latitude: f64, longitude: f64) -> Self {
        Self {
            latitude,
            longitude,
        }
    }

    /// Get the latitude
    #[must_use]
    pub const fn latitude(&self) -> f64 {
        self.latitude
    }

    /// Get the longitude
    #[must_use]
    pub const fn longitude(&self) -> f64 {
        self.longitude
    }

    /// Calculate approximate distance to another location in kilometers
    ///
    /// Uses the Haversine formula for great-circle distance
    #[must_use]
    pub fn distance_km(&self, other: &Self) -> f64 {
        const EARTH_RADIUS_KM: f64 = 6371.0;

        let lat1_rad = self.latitude.to_radians();
        let lat2_rad = other.latitude.to_radians();
        let delta_lat = (other.latitude - self.latitude).to_radians();
        let delta_lon = (other.longitude - self.longitude).to_radians();

        let a = (lat1_rad.cos() * lat2_rad.cos()).mul_add(
            (delta_lon / 2.0).sin().powi(2),
            (delta_lat / 2.0).sin().powi(2),
        );
        let c = 2.0 * a.sqrt().atan2((1.0 - a).sqrt());

        EARTH_RADIUS_KM * c
    }
}

impl fmt::Display for GeoLocation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:.6}, {:.6}", self.latitude, self.longitude)
    }
}

/// Common locations for defaults
impl GeoLocation {
    /// Berlin, Germany
    #[must_use]
    pub const fn berlin() -> Self {
        Self::new_unchecked(52.52, 13.405)
    }

    /// London, UK
    #[must_use]
    pub const fn london() -> Self {
        Self::new_unchecked(51.5074, -0.1278)
    }

    /// New York, USA
    #[must_use]
    pub const fn new_york() -> Self {
        Self::new_unchecked(40.7128, -74.006)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_coordinates() {
        let loc = GeoLocation::new(52.52, 13.405).expect("valid coordinates");
        assert!((loc.latitude() - 52.52).abs() < f64::EPSILON);
        assert!((loc.longitude() - 13.405).abs() < f64::EPSILON);
    }

    #[test]
    fn test_boundary_coordinates() {
        assert!(GeoLocation::new(90.0, 180.0).is_ok());
        assert!(GeoLocation::new(-90.0, -180.0).is_ok());
        assert!(GeoLocation::new(0.0, 0.0).is_ok());
    }

    #[test]
    fn test_invalid_latitude() {
        assert!(GeoLocation::new(91.0, 0.0).is_err());
        assert!(GeoLocation::new(-91.0, 0.0).is_err());
    }

    #[test]
    fn test_invalid_longitude() {
        assert!(GeoLocation::new(0.0, 181.0).is_err());
        assert!(GeoLocation::new(0.0, -181.0).is_err());
    }

    #[test]
    fn test_display() {
        let loc = GeoLocation::new(52.52, 13.405).expect("valid");
        let display = format!("{loc}");
        assert!(display.contains("52.52"));
        assert!(display.contains("13.405"));
    }

    #[test]
    fn test_distance_same_location() {
        let loc = GeoLocation::berlin();
        assert!(loc.distance_km(&loc).abs() < 0.001);
    }

    #[test]
    fn test_distance_berlin_london() {
        let berlin = GeoLocation::berlin();
        let london = GeoLocation::london();
        let distance = berlin.distance_km(&london);
        // Berlin to London is approximately 930km
        assert!((distance - 930.0).abs() < 50.0);
    }

    #[test]
    fn test_serialization() {
        let loc = GeoLocation::new(52.52, 13.405).expect("valid");
        let json = serde_json::to_string(&loc).expect("serialize");
        assert!(json.contains("52.52"));
        assert!(json.contains("13.405"));

        let deserialized: GeoLocation = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(loc, deserialized);
    }

    #[test]
    fn test_common_locations() {
        assert!((GeoLocation::berlin().latitude() - 52.52).abs() < 0.01);
        assert!((GeoLocation::london().latitude() - 51.5074).abs() < 0.01);
        assert!((GeoLocation::new_york().latitude() - 40.7128).abs() < 0.01);
    }
}
