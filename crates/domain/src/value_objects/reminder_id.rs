//! Reminder identifier for tracking reminder entries

use std::fmt;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// A unique reminder identifier
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ReminderId(Uuid);

impl ReminderId {
    /// Create a new random reminder ID
    #[must_use]
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    /// Create a reminder ID from an existing UUID
    #[must_use]
    pub const fn from_uuid(uuid: Uuid) -> Self {
        Self(uuid)
    }

    /// Parse a reminder ID from a string
    ///
    /// # Errors
    ///
    /// Returns an error if the string is not a valid UUID.
    pub fn parse(s: &str) -> Result<Self, uuid::Error> {
        Ok(Self(Uuid::parse_str(s)?))
    }

    /// Get the underlying UUID
    #[must_use]
    pub const fn as_uuid(&self) -> Uuid {
        self.0
    }
}

impl Default for ReminderId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for ReminderId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<Uuid> for ReminderId {
    fn from(uuid: Uuid) -> Self {
        Self(uuid)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_reminder_id_is_unique() {
        let id1 = ReminderId::new();
        let id2 = ReminderId::new();
        assert_ne!(id1, id2);
    }

    #[test]
    fn reminder_id_roundtrips_through_string() {
        let original = ReminderId::new();
        let parsed = ReminderId::parse(&original.to_string()).unwrap();
        assert_eq!(original, parsed);
    }

    #[test]
    fn from_uuid() {
        let uuid = Uuid::new_v4();
        let id = ReminderId::from_uuid(uuid);
        assert_eq!(id.as_uuid(), uuid);
    }

    #[test]
    fn display_format() {
        let uuid = Uuid::new_v4();
        let id = ReminderId::from_uuid(uuid);
        assert_eq!(id.to_string(), uuid.to_string());
    }

    #[test]
    fn default_creates_new_id() {
        let id1 = ReminderId::default();
        let id2 = ReminderId::default();
        assert_ne!(id1, id2);
    }

    #[test]
    fn parse_invalid_returns_error() {
        assert!(ReminderId::parse("not-a-uuid").is_err());
    }
}
