//! Timezone value object

use serde::{Deserialize, Serialize};
use std::fmt;

/// A timezone identifier (IANA timezone name)
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Timezone(String);

impl Timezone {
    /// Create a new timezone
    ///
    /// Note: This does not validate against the IANA database.
    /// For production use, consider integrating with chrono-tz for validation.
    #[must_use]
    pub fn new(tz: impl Into<String>) -> Self {
        Self(tz.into())
    }

    /// Get the timezone string
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Check if this is a UTC timezone
    #[must_use]
    pub fn is_utc(&self) -> bool {
        matches!(self.0.as_str(), "UTC" | "Etc/UTC" | "Etc/GMT")
    }
}

impl Default for Timezone {
    fn default() -> Self {
        Self("UTC".to_string())
    }
}

impl fmt::Display for Timezone {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<&str> for Timezone {
    fn from(s: &str) -> Self {
        Self::new(s)
    }
}

impl From<String> for Timezone {
    fn from(s: String) -> Self {
        Self(s)
    }
}

/// Common timezone constants
impl Timezone {
    /// UTC timezone
    #[must_use]
    pub fn utc() -> Self {
        Self("UTC".to_string())
    }

    /// Europe/Berlin timezone
    #[must_use]
    pub fn berlin() -> Self {
        Self("Europe/Berlin".to_string())
    }

    /// Europe/London timezone
    #[must_use]
    pub fn london() -> Self {
        Self("Europe/London".to_string())
    }

    /// America/New_York timezone
    #[must_use]
    pub fn new_york() -> Self {
        Self("America/New_York".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_timezone_creation() {
        let tz = Timezone::new("Europe/Berlin");
        assert_eq!(tz.as_str(), "Europe/Berlin");
    }

    #[test]
    fn test_timezone_default() {
        let tz = Timezone::default();
        assert_eq!(tz.as_str(), "UTC");
    }

    #[test]
    fn test_timezone_is_utc() {
        assert!(Timezone::utc().is_utc());
        assert!(Timezone::new("Etc/UTC").is_utc());
        assert!(Timezone::new("Etc/GMT").is_utc());
        assert!(!Timezone::berlin().is_utc());
    }

    #[test]
    fn test_timezone_display() {
        let tz = Timezone::berlin();
        assert_eq!(format!("{tz}"), "Europe/Berlin");
    }

    #[test]
    fn test_timezone_from_str() {
        let tz: Timezone = "America/New_York".into();
        assert_eq!(tz.as_str(), "America/New_York");
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
    fn test_timezone_equality() {
        let tz1 = Timezone::berlin();
        let tz2 = Timezone::berlin();
        let tz3 = Timezone::london();

        assert_eq!(tz1, tz2);
        assert_ne!(tz1, tz3);
    }
}
