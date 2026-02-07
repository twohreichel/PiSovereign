//! Humidity value object
//!
//! Represents a validated relative humidity percentage (0-100%).
//!
//! # Examples
//!
//! ```
//! use domain::value_objects::Humidity;
//!
//! // Create a valid humidity value
//! let h = Humidity::new(65).expect("valid humidity");
//! assert_eq!(h.value(), 65);
//!
//! // Invalid values return an error
//! assert!(Humidity::new(101).is_err());
//!
//! // Clamp out-of-range values
//! let clamped = Humidity::clamped(150);
//! assert_eq!(clamped.value(), 100);
//! ```

use serde::{Deserialize, Serialize};
use std::fmt;
use thiserror::Error;

/// Error returned when a humidity value is out of range
#[derive(Debug, Clone, Copy, Error, PartialEq, Eq)]
#[error("invalid humidity: {0}% is out of range (must be 0-100)")]
pub struct InvalidHumidity(u8);

/// Relative humidity percentage (0-100%)
///
/// This value object ensures humidity values are always within valid bounds.
///
/// # Examples
///
/// ```
/// use domain::value_objects::Humidity;
///
/// let h = Humidity::new(50).expect("valid humidity");
/// assert_eq!(h.value(), 50);
/// assert_eq!(format!("{h}"), "50%");
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub struct Humidity(u8);

impl Humidity {
    /// Maximum valid humidity percentage
    pub const MAX: u8 = 100;

    /// Create a new validated humidity value
    ///
    /// # Errors
    ///
    /// Returns `InvalidHumidity` if the value is greater than 100.
    ///
    /// # Examples
    ///
    /// ```
    /// use domain::value_objects::Humidity;
    ///
    /// assert!(Humidity::new(0).is_ok());
    /// assert!(Humidity::new(100).is_ok());
    /// assert!(Humidity::new(101).is_err());
    /// ```
    pub const fn new(value: u8) -> Result<Self, InvalidHumidity> {
        if value > Self::MAX {
            Err(InvalidHumidity(value))
        } else {
            Ok(Self(value))
        }
    }

    /// Create a humidity value, clamping to valid range
    ///
    /// Values greater than 100 are clamped to 100.
    ///
    /// # Examples
    ///
    /// ```
    /// use domain::value_objects::Humidity;
    ///
    /// assert_eq!(Humidity::clamped(150).value(), 100);
    /// assert_eq!(Humidity::clamped(50).value(), 50);
    /// ```
    #[must_use]
    pub const fn clamped(value: u8) -> Self {
        if value > Self::MAX {
            Self(Self::MAX)
        } else {
            Self(value)
        }
    }

    /// Get the humidity value as a u8
    #[must_use]
    pub const fn value(self) -> u8 {
        self.0
    }

    /// Check if humidity is considered "dry" (< 30%)
    #[must_use]
    pub const fn is_dry(self) -> bool {
        self.0 < 30
    }

    /// Check if humidity is considered "comfortable" (30-60%)
    #[must_use]
    pub const fn is_comfortable(self) -> bool {
        self.0 >= 30 && self.0 <= 60
    }

    /// Check if humidity is considered "humid" (> 60%)
    #[must_use]
    pub const fn is_humid(self) -> bool {
        self.0 > 60
    }
}

impl Default for Humidity {
    fn default() -> Self {
        Self(50)
    }
}

impl fmt::Display for Humidity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}%", self.0)
    }
}

impl TryFrom<u8> for Humidity {
    type Error = InvalidHumidity;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl From<Humidity> for u8 {
    fn from(h: Humidity) -> Self {
        h.0
    }
}

/// Custom deserialization that validates humidity values
impl<'de> Deserialize<'de> for Humidity {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = u8::deserialize(deserializer)?;
        Self::new(value).map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_humidity_new_valid() {
        assert!(Humidity::new(0).is_ok());
        assert!(Humidity::new(50).is_ok());
        assert!(Humidity::new(100).is_ok());
    }

    #[test]
    fn test_humidity_new_invalid() {
        let result = Humidity::new(101);
        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err().to_string(),
            "invalid humidity: 101% is out of range (must be 0-100)"
        );
    }

    #[test]
    fn test_humidity_clamped() {
        assert_eq!(Humidity::clamped(0).value(), 0);
        assert_eq!(Humidity::clamped(50).value(), 50);
        assert_eq!(Humidity::clamped(100).value(), 100);
        assert_eq!(Humidity::clamped(101).value(), 100);
        assert_eq!(Humidity::clamped(255).value(), 100);
    }

    #[test]
    fn test_humidity_display() {
        assert_eq!(format!("{}", Humidity::new(65).unwrap()), "65%");
    }

    #[test]
    fn test_humidity_categories() {
        assert!(Humidity::new(20).unwrap().is_dry());
        assert!(!Humidity::new(20).unwrap().is_comfortable());
        assert!(!Humidity::new(20).unwrap().is_humid());

        assert!(!Humidity::new(45).unwrap().is_dry());
        assert!(Humidity::new(45).unwrap().is_comfortable());
        assert!(!Humidity::new(45).unwrap().is_humid());

        assert!(!Humidity::new(75).unwrap().is_dry());
        assert!(!Humidity::new(75).unwrap().is_comfortable());
        assert!(Humidity::new(75).unwrap().is_humid());
    }

    #[test]
    fn test_humidity_try_from() {
        assert!(Humidity::try_from(50u8).is_ok());
        assert!(Humidity::try_from(101u8).is_err());
    }

    #[test]
    fn test_humidity_into_u8() {
        let h = Humidity::new(65).unwrap();
        let v: u8 = h.into();
        assert_eq!(v, 65);
    }

    #[test]
    fn test_humidity_serialization() {
        let h = Humidity::new(65).unwrap();
        let json = serde_json::to_string(&h).expect("serialize");
        assert_eq!(json, "65");
    }

    #[test]
    fn test_humidity_deserialization_valid() {
        let h: Humidity = serde_json::from_str("65").expect("deserialize");
        assert_eq!(h.value(), 65);
    }

    #[test]
    fn test_humidity_deserialization_invalid() {
        let result: Result<Humidity, _> = serde_json::from_str("101");
        assert!(result.is_err());
    }

    #[test]
    fn test_humidity_ordering() {
        let low = Humidity::new(30).unwrap();
        let high = Humidity::new(70).unwrap();
        assert!(low < high);
    }

    #[test]
    fn test_humidity_default() {
        assert_eq!(Humidity::default().value(), 50);
    }
}
