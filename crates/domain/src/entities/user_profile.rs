//! User profile entity
//!
//! Represents a user's profile with location and preferences.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::value_objects::{GeoLocation, Timezone, UserId};

/// User profile with location and preferences
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserProfile {
    /// Unique user identifier
    id: UserId,
    /// User's current location (for weather, etc.)
    location: Option<GeoLocation>,
    /// User's timezone
    timezone: Timezone,
    /// When the profile was created
    created_at: DateTime<Utc>,
    /// When the profile was last updated
    updated_at: DateTime<Utc>,
}

impl UserProfile {
    /// Create a new user profile
    #[must_use]
    pub fn new(id: UserId) -> Self {
        let now = Utc::now();
        Self {
            id,
            location: None,
            timezone: Timezone::default(),
            created_at: now,
            updated_at: now,
        }
    }

    /// Create a profile with a default location
    #[must_use]
    pub fn with_defaults(id: UserId, location: GeoLocation, timezone: Timezone) -> Self {
        let now = Utc::now();
        Self {
            id,
            location: Some(location),
            timezone,
            created_at: now,
            updated_at: now,
        }
    }

    /// Restore a profile from storage
    #[must_use]
    pub const fn restore(
        id: UserId,
        location: Option<GeoLocation>,
        timezone: Timezone,
        created_at: DateTime<Utc>,
        updated_at: DateTime<Utc>,
    ) -> Self {
        Self {
            id,
            location,
            timezone,
            created_at,
            updated_at,
        }
    }

    /// Get the user ID
    #[must_use]
    pub const fn id(&self) -> UserId {
        self.id
    }

    /// Get the user's location
    #[must_use]
    pub const fn location(&self) -> Option<GeoLocation> {
        self.location
    }

    /// Get the user's timezone
    #[must_use]
    pub const fn timezone(&self) -> &Timezone {
        &self.timezone
    }

    /// Get the creation timestamp
    #[must_use]
    pub const fn created_at(&self) -> DateTime<Utc> {
        self.created_at
    }

    /// Get the last update timestamp
    #[must_use]
    pub const fn updated_at(&self) -> DateTime<Utc> {
        self.updated_at
    }

    /// Update the user's location
    pub fn update_location(&mut self, location: GeoLocation) {
        self.location = Some(location);
        self.updated_at = Utc::now();
    }

    /// Clear the user's location
    pub fn clear_location(&mut self) {
        self.location = None;
        self.updated_at = Utc::now();
    }

    /// Update the user's timezone
    pub fn update_timezone(&mut self, timezone: Timezone) {
        self.timezone = timezone;
        self.updated_at = Utc::now();
    }

    /// Check if the profile has a location set
    #[must_use]
    pub const fn has_location(&self) -> bool {
        self.location.is_some()
    }
}

impl Default for UserProfile {
    fn default() -> Self {
        Self::new(UserId::new())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_profile() {
        let id = UserId::new();
        let profile = UserProfile::new(id);

        assert_eq!(profile.id(), id);
        assert!(profile.location().is_none());
        assert!(profile.timezone().is_utc());
        assert!(!profile.has_location());
    }

    #[test]
    fn test_profile_with_defaults() {
        let id = UserId::new();
        let location = GeoLocation::berlin();
        let timezone = Timezone::berlin();

        let profile = UserProfile::with_defaults(id, location, timezone.clone());

        assert_eq!(profile.id(), id);
        assert_eq!(profile.location(), Some(location));
        assert_eq!(profile.timezone(), &timezone);
        assert!(profile.has_location());
    }

    #[test]
    fn test_update_location() {
        let mut profile = UserProfile::default();
        let original_updated_at = profile.updated_at();

        // Small delay to ensure timestamp changes
        std::thread::sleep(std::time::Duration::from_millis(1));

        let location = GeoLocation::london();
        profile.update_location(location);

        assert_eq!(profile.location(), Some(location));
        assert!(profile.updated_at() >= original_updated_at);
    }

    #[test]
    fn test_clear_location() {
        let location = GeoLocation::berlin();
        let mut profile = UserProfile::with_defaults(UserId::new(), location, Timezone::berlin());

        assert!(profile.has_location());
        profile.clear_location();
        assert!(!profile.has_location());
    }

    #[test]
    fn test_update_timezone() {
        let mut profile = UserProfile::default();

        profile.update_timezone(Timezone::new_york());

        assert_eq!(profile.timezone().as_str(), "America/New_York");
    }

    #[test]
    fn test_restore_profile() {
        let id = UserId::new();
        let location = GeoLocation::new_york();
        let timezone = Timezone::new_york();
        let created = Utc::now() - chrono::Duration::days(30);
        let updated = Utc::now() - chrono::Duration::days(1);

        let profile = UserProfile::restore(id, Some(location), timezone.clone(), created, updated);

        assert_eq!(profile.id(), id);
        assert_eq!(profile.location(), Some(location));
        assert_eq!(profile.timezone(), &timezone);
        assert_eq!(profile.created_at(), created);
        assert_eq!(profile.updated_at(), updated);
    }

    #[test]
    fn test_serialization() {
        let profile =
            UserProfile::with_defaults(UserId::new(), GeoLocation::berlin(), Timezone::berlin());

        let json = serde_json::to_string(&profile).expect("serialize");
        let deserialized: UserProfile = serde_json::from_str(&json).expect("deserialize");

        assert_eq!(profile.id(), deserialized.id());
        assert_eq!(profile.location(), deserialized.location());
    }
}
