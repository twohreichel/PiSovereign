//! User profile storage port
//!
//! Defines the interface for user profile persistence.

use async_trait::async_trait;
use domain::{
    entities::UserProfile,
    value_objects::{GeoLocation, Timezone, UserId},
};

use crate::error::ApplicationError;

/// Port for user profile storage operations
#[async_trait]
pub trait UserProfileStore: Send + Sync {
    /// Save or update a user profile
    async fn save(&self, profile: &UserProfile) -> Result<(), ApplicationError>;

    /// Get a user profile by ID
    async fn get(&self, user_id: &UserId) -> Result<Option<UserProfile>, ApplicationError>;

    /// Delete a user profile
    async fn delete(&self, user_id: &UserId) -> Result<bool, ApplicationError>;

    /// Update only the user's location
    ///
    /// Returns `true` if the profile was found and updated, `false` if not found.
    async fn update_location(
        &self,
        user_id: &UserId,
        location: Option<&GeoLocation>,
    ) -> Result<bool, ApplicationError>;

    /// Update only the user's timezone
    ///
    /// Returns `true` if the profile was found and updated, `false` if not found.
    async fn update_timezone(
        &self,
        user_id: &UserId,
        timezone: &Timezone,
    ) -> Result<bool, ApplicationError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    // Simple compile-time verification that the trait is object-safe
    fn _assert_object_safe(_: &dyn UserProfileStore) {}

    #[test]
    fn trait_is_send_sync() {
        fn assert_send_sync<T: Send + Sync + ?Sized>() {}
        assert_send_sync::<dyn UserProfileStore>();
    }
}
