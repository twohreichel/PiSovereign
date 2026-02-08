//! Timezone value object
//!
//! Represents a validated IANA timezone identifier using chrono-tz.
//!
//! # Examples
//!
//! ```
//! use domain::value_objects::Timezone;
//!
//! // Create a validated timezone
//! let tz = Timezone::try_new("Europe/Berlin").expect("valid timezone");
//! assert_eq!(tz.as_str(), "Europe/Berlin");
//!
//! // Use predefined constants
//! let utc = Timezone::utc();
//! assert!(utc.is_utc());
//!
//! // Invalid timezones return an error
//! assert!(Timezone::try_new("Invalid/Timezone").is_err());
//! ```

use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;
use thiserror::Error;

/// Error returned when a timezone string is invalid
#[derive(Debug, Clone, Error, PartialEq, Eq)]
#[error("invalid timezone: '{0}' is not a valid IANA timezone identifier")]
pub struct InvalidTimezone(String);

/// A validated timezone identifier (IANA timezone name)
///
/// All timezone values are validated against the IANA timezone database
/// using chrono-tz to ensure they are valid identifiers.
///
/// # Examples
///
/// ```
/// use domain::value_objects::Timezone;
///
/// let tz = Timezone::berlin();
/// assert_eq!(tz.to_string(), "Europe/Berlin");
/// assert!(!tz.is_utc());
///
/// // Validation example
/// assert!(Timezone::try_new("Invalid/Zone").is_err());
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize)]
pub struct Timezone(String);

impl Timezone {
    /// Create a new validated timezone
    ///
    /// Validates the timezone string against the IANA timezone database.
    ///
    /// # Errors
    ///
    /// Returns `InvalidTimezone` if the string is not a valid IANA timezone identifier.
    ///
    /// # Examples
    ///
    /// ```
    /// use domain::value_objects::Timezone;
    ///
    /// let tz = Timezone::try_new("America/Los_Angeles").expect("valid timezone");
    /// assert_eq!(tz.as_str(), "America/Los_Angeles");
    ///
    /// assert!(Timezone::try_new("Not/A/Timezone").is_err());
    /// ```
    pub fn try_new(tz: impl Into<String>) -> Result<Self, InvalidTimezone> {
        let tz_string = tz.into();
        // Validate against chrono-tz
        chrono_tz::Tz::from_str(&tz_string).map_err(|_| InvalidTimezone(tz_string.clone()))?;
        Ok(Self(tz_string))
    }

    /// Create a new timezone without validation
    ///
    /// # Safety
    ///
    /// This should only be used for known-valid timezone strings (e.g., constants).
    /// Prefer `try_new` for user-provided input.
    #[must_use]
    pub(crate) fn new_unchecked(tz: impl Into<String>) -> Self {
        Self(tz.into())
    }

    /// Get the timezone string
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Get the chrono-tz timezone
    ///
    /// This always succeeds because we validated on construction.
    #[must_use]
    pub fn as_chrono_tz(&self) -> chrono_tz::Tz {
        // Safe because we validated on construction
        chrono_tz::Tz::from_str(&self.0).expect("timezone was validated on construction")
    }

    /// Check if this is a UTC timezone
    ///
    /// # Examples
    ///
    /// ```
    /// use domain::value_objects::Timezone;
    ///
    /// assert!(Timezone::utc().is_utc());
    /// assert!(!Timezone::berlin().is_utc());
    /// ```
    #[must_use]
    pub fn is_utc(&self) -> bool {
        matches!(self.0.as_str(), "UTC" | "Etc/UTC" | "Etc/GMT")
    }
}

impl Default for Timezone {
    fn default() -> Self {
        Self::utc()
    }
}

impl fmt::Display for Timezone {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl FromStr for Timezone {
    type Err = InvalidTimezone;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::try_new(s)
    }
}

/// Custom deserialization that validates timezone strings
impl<'de> Deserialize<'de> for Timezone {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        Self::try_new(s).map_err(serde::de::Error::custom)
    }
}

/// Common timezone constants
impl Timezone {
    /// UTC timezone
    #[must_use]
    pub fn utc() -> Self {
        Self::new_unchecked("UTC")
    }

    /// Europe/Berlin timezone
    #[must_use]
    pub fn berlin() -> Self {
        Self::new_unchecked("Europe/Berlin")
    }

    /// Europe/London timezone
    #[must_use]
    pub fn london() -> Self {
        Self::new_unchecked("Europe/London")
    }

    /// America/New_York timezone
    #[must_use]
    pub fn new_york() -> Self {
        Self::new_unchecked("America/New_York")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_timezone_try_new_valid() {
        let tz = Timezone::try_new("Europe/Berlin").expect("valid timezone");
        assert_eq!(tz.as_str(), "Europe/Berlin");
    }

    #[test]
    fn test_timezone_try_new_invalid() {
        let result = Timezone::try_new("Invalid/Timezone");
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(
            err.to_string(),
            "invalid timezone: 'Invalid/Timezone' is not a valid IANA timezone identifier"
        );
    }

    #[test]
    fn test_timezone_default() {
        let tz = Timezone::default();
        assert_eq!(tz.as_str(), "UTC");
    }

    #[test]
    fn test_timezone_is_utc() {
        assert!(Timezone::utc().is_utc());
        assert!(Timezone::try_new("Etc/UTC").expect("valid").is_utc());
        assert!(Timezone::try_new("Etc/GMT").expect("valid").is_utc());
        assert!(!Timezone::berlin().is_utc());
    }

    #[test]
    fn test_timezone_display() {
        let tz = Timezone::berlin();
        assert_eq!(format!("{tz}"), "Europe/Berlin");
    }

    #[test]
    fn test_timezone_from_str() {
        let tz: Timezone = "America/New_York".parse().expect("valid timezone");
        assert_eq!(tz.as_str(), "America/New_York");

        let result: Result<Timezone, _> = "Bad/Zone".parse();
        assert!(result.is_err());
    }

    #[test]
    fn test_timezone_serialization() {
        let tz = Timezone::berlin();
        let json = serde_json::to_string(&tz).expect("serialize");
        assert!(json.contains("Europe/Berlin"));

        let deserialized: Timezone = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(tz, deserialized);
    }

    #[test]
    fn test_timezone_deserialization_invalid() {
        let result: Result<Timezone, _> = serde_json::from_str("\"Invalid/Zone\"");
        assert!(result.is_err());
    }

    #[test]
    fn test_timezone_equality() {
        let tz1 = Timezone::berlin();
        let tz2 = Timezone::berlin();
        let tz3 = Timezone::london();

        assert_eq!(tz1, tz2);
        assert_ne!(tz1, tz3);
    }

    #[test]
    fn test_as_chrono_tz() {
        let tz = Timezone::berlin();
        let chrono_tz = tz.as_chrono_tz();
        assert_eq!(chrono_tz, chrono_tz::Europe::Berlin);
    }

    #[test]
    fn test_common_timezones() {
        // Verify all constants are valid
        let _ = Timezone::utc();
        let _ = Timezone::berlin();
        let _ = Timezone::london();
        let _ = Timezone::new_york();
    }
}
