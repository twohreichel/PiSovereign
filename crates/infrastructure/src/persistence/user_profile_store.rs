//! SQLite user profile store implementation
//!
//! Implements the `UserProfileStore` port using sqlx.

use application::{error::ApplicationError, ports::UserProfileStore};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use domain::{
    entities::UserProfile,
    value_objects::{GeoLocation, Timezone, UserId},
};
use sqlx::SqlitePool;
use tracing::{debug, instrument};

use super::error::map_sqlx_error;

/// SQLite-based user profile store
#[derive(Debug, Clone)]
pub struct SqliteUserProfileStore {
    pool: SqlitePool,
}

impl SqliteUserProfileStore {
    /// Create a new SQLite user profile store
    #[must_use]
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

/// Row type for user profile queries
#[derive(sqlx::FromRow)]
struct ProfileRow {
    user_id: String,
    latitude: Option<f64>,
    longitude: Option<f64>,
    timezone: String,
    created_at: String,
    updated_at: String,
}

impl ProfileRow {
    #[allow(clippy::wrong_self_convention)]
    fn to_profile(self) -> Result<UserProfile, ApplicationError> {
        let user_id = UserId::parse(&self.user_id)
            .map_err(|e| ApplicationError::Internal(format!("Invalid user_id: {e}")))?;

        let location = match (self.latitude, self.longitude) {
            (Some(lat), Some(lon)) => GeoLocation::new(lat, lon).ok(),
            _ => None,
        };

        // Validate timezone; fall back to UTC if invalid (for legacy data)
        let timezone = Timezone::try_new(&self.timezone).unwrap_or_else(|_| {
            tracing::warn!(
                timezone = %self.timezone,
                "Invalid timezone in database, falling back to UTC"
            );
            Timezone::utc()
        });

        let created_at = DateTime::parse_from_rfc3339(&self.created_at)
            .map_or_else(|_| Utc::now(), |dt| dt.with_timezone(&Utc));

        let updated_at = DateTime::parse_from_rfc3339(&self.updated_at)
            .map_or_else(|_| Utc::now(), |dt| dt.with_timezone(&Utc));

        Ok(UserProfile::restore(
            user_id, location, timezone, created_at, updated_at,
        ))
    }
}

#[async_trait]
impl UserProfileStore for SqliteUserProfileStore {
    #[instrument(skip(self, profile), fields(user_id = %profile.id()))]
    async fn save(&self, profile: &UserProfile) -> Result<(), ApplicationError> {
        let now = Utc::now().to_rfc3339();
        let (latitude, longitude) = profile.location().map_or((None, None), |loc| {
            (Some(loc.latitude()), Some(loc.longitude()))
        });

        sqlx::query(
            "INSERT INTO user_profiles (user_id, latitude, longitude, timezone, created_at, updated_at)
             VALUES ($1, $2, $3, $4, $5, $5)
             ON CONFLICT(user_id) DO UPDATE SET
                 latitude = excluded.latitude,
                 longitude = excluded.longitude,
                 timezone = excluded.timezone,
                 updated_at = excluded.updated_at",
        )
        .bind(profile.id().to_string())
        .bind(latitude)
        .bind(longitude)
        .bind(profile.timezone().as_str())
        .bind(&now)
        .execute(&self.pool)
        .await
        .map_err(map_sqlx_error)?;

        debug!("Saved user profile");
        Ok(())
    }

    #[instrument(skip(self), fields(user_id = %user_id))]
    async fn get(&self, user_id: &UserId) -> Result<Option<UserProfile>, ApplicationError> {
        let row: Option<ProfileRow> = sqlx::query_as(
            "SELECT user_id, latitude, longitude, timezone, created_at, updated_at
             FROM user_profiles WHERE user_id = $1",
        )
        .bind(user_id.to_string())
        .fetch_optional(&self.pool)
        .await
        .map_err(map_sqlx_error)?;

        let profile = row.map(ProfileRow::to_profile).transpose()?;
        debug!(found = profile.is_some(), "Retrieved user profile");
        Ok(profile)
    }

    #[instrument(skip(self), fields(user_id = %user_id))]
    async fn delete(&self, user_id: &UserId) -> Result<bool, ApplicationError> {
        let result = sqlx::query("DELETE FROM user_profiles WHERE user_id = $1")
            .bind(user_id.to_string())
            .execute(&self.pool)
            .await
            .map_err(map_sqlx_error)?;

        let deleted = result.rows_affected() > 0;
        debug!(deleted, "Deleted user profile");
        Ok(deleted)
    }

    #[instrument(skip(self), fields(user_id = %user_id))]
    async fn update_location(
        &self,
        user_id: &UserId,
        location: Option<&GeoLocation>,
    ) -> Result<bool, ApplicationError> {
        let (latitude, longitude) = location.map_or((None, None), |loc| {
            (Some(loc.latitude()), Some(loc.longitude()))
        });
        let now = Utc::now().to_rfc3339();

        let result = sqlx::query(
            "UPDATE user_profiles SET latitude = $1, longitude = $2, updated_at = $3
             WHERE user_id = $4",
        )
        .bind(latitude)
        .bind(longitude)
        .bind(&now)
        .bind(user_id.to_string())
        .execute(&self.pool)
        .await
        .map_err(map_sqlx_error)?;

        let updated = result.rows_affected() > 0;
        debug!(updated, "Updated user location");
        Ok(updated)
    }

    #[instrument(skip(self), fields(user_id = %user_id))]
    async fn update_timezone(
        &self,
        user_id: &UserId,
        timezone: &Timezone,
    ) -> Result<bool, ApplicationError> {
        let now = Utc::now().to_rfc3339();

        let result = sqlx::query(
            "UPDATE user_profiles SET timezone = $1, updated_at = $2
             WHERE user_id = $3",
        )
        .bind(timezone.as_str())
        .bind(&now)
        .bind(user_id.to_string())
        .execute(&self.pool)
        .await
        .map_err(map_sqlx_error)?;

        let updated = result.rows_affected() > 0;
        debug!(updated, "Updated user timezone");
        Ok(updated)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::persistence::async_connection::AsyncDatabase;

    async fn setup() -> (AsyncDatabase, SqliteUserProfileStore) {
        let db = AsyncDatabase::in_memory().await.unwrap();
        db.migrate().await.unwrap();
        let store = SqliteUserProfileStore::new(db.pool().clone());
        (db, store)
    }

    #[tokio::test]
    async fn save_and_get_profile() {
        let (_db, store) = setup().await;

        let user_id = UserId::new();
        let location = GeoLocation::new(52.52, 13.405).unwrap();
        let timezone = Timezone::try_new("Europe/Berlin").expect("valid tz");
        let profile = UserProfile::with_defaults(user_id, location, timezone);

        store.save(&profile).await.unwrap();

        let retrieved = store.get(&profile.id()).await.unwrap().unwrap();
        assert_eq!(retrieved.id(), profile.id());
        assert!(retrieved.location().is_some());
        let loc = retrieved.location().unwrap();
        assert!((loc.latitude() - 52.52).abs() < 0.001);
        assert!((loc.longitude() - 13.405).abs() < 0.001);
        assert_eq!(retrieved.timezone().as_str(), "Europe/Berlin");
    }

    #[tokio::test]
    async fn save_profile_without_location() {
        let (_db, store) = setup().await;

        let user_id = UserId::new();
        let profile = UserProfile::new(user_id);

        store.save(&profile).await.unwrap();

        let retrieved = store.get(&profile.id()).await.unwrap().unwrap();
        assert!(retrieved.location().is_none());
        assert!(retrieved.timezone().is_utc());
    }

    #[tokio::test]
    async fn update_existing_profile() {
        let (_db, store) = setup().await;

        let user_id = UserId::new();
        let profile = UserProfile::new(user_id);
        store.save(&profile).await.unwrap();

        let location = GeoLocation::new(48.8566, 2.3522).unwrap();
        let new_tz = Timezone::try_new("Europe/Paris").expect("valid tz");
        let updated_profile = UserProfile::with_defaults(profile.id(), location, new_tz);
        store.save(&updated_profile).await.unwrap();

        let retrieved = store.get(&profile.id()).await.unwrap().unwrap();
        assert!(retrieved.location().is_some());
        assert_eq!(retrieved.timezone().as_str(), "Europe/Paris");
    }

    #[tokio::test]
    async fn get_nonexistent_profile() {
        let (_db, store) = setup().await;
        let result = store.get(&UserId::new()).await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn delete_profile() {
        let (_db, store) = setup().await;

        let profile = UserProfile::new(UserId::new());
        store.save(&profile).await.unwrap();

        let deleted = store.delete(&profile.id()).await.unwrap();
        assert!(deleted);

        let result = store.get(&profile.id()).await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn delete_nonexistent_profile() {
        let (_db, store) = setup().await;
        let deleted = store.delete(&UserId::new()).await.unwrap();
        assert!(!deleted);
    }

    #[tokio::test]
    async fn update_location_only() {
        let (_db, store) = setup().await;

        let user_id = UserId::new();
        let mut profile = UserProfile::new(user_id);
        profile.update_timezone(Timezone::try_new("America/New_York").expect("valid tz"));
        store.save(&profile).await.unwrap();

        let location = GeoLocation::new(40.7128, -74.0060).unwrap();
        let updated = store
            .update_location(&profile.id(), Some(&location))
            .await
            .unwrap();
        assert!(updated);

        let retrieved = store.get(&profile.id()).await.unwrap().unwrap();
        assert!(retrieved.location().is_some());
        let loc = retrieved.location().unwrap();
        assert!((loc.latitude() - 40.7128).abs() < 0.001);
        assert_eq!(retrieved.timezone().as_str(), "America/New_York");
    }

    #[tokio::test]
    async fn update_location_nonexistent_profile() {
        let (_db, store) = setup().await;
        let location = GeoLocation::new(40.7128, -74.0060).unwrap();
        let updated = store
            .update_location(&UserId::new(), Some(&location))
            .await
            .unwrap();
        assert!(!updated);
    }

    #[tokio::test]
    async fn clear_location() {
        let (_db, store) = setup().await;

        let location = GeoLocation::new(40.7128, -74.0060).unwrap();
        let profile = UserProfile::with_defaults(UserId::new(), location, Timezone::default());
        store.save(&profile).await.unwrap();

        let updated = store.update_location(&profile.id(), None).await.unwrap();
        assert!(updated);

        let retrieved = store.get(&profile.id()).await.unwrap().unwrap();
        assert!(retrieved.location().is_none());
    }

    #[tokio::test]
    async fn update_timezone_only() {
        let (_db, store) = setup().await;

        let location = GeoLocation::new(51.5074, -0.1278).unwrap();
        let profile = UserProfile::with_defaults(UserId::new(), location, Timezone::default());
        store.save(&profile).await.unwrap();

        let new_tz = Timezone::try_new("Europe/London").expect("valid tz");
        let updated = store.update_timezone(&profile.id(), &new_tz).await.unwrap();
        assert!(updated);

        let retrieved = store.get(&profile.id()).await.unwrap().unwrap();
        assert_eq!(retrieved.timezone().as_str(), "Europe/London");
        assert!(retrieved.location().is_some());
    }

    #[tokio::test]
    async fn update_timezone_nonexistent_profile() {
        let (_db, store) = setup().await;
        let timezone = Timezone::try_new("Asia/Tokyo").expect("valid tz");
        let updated = store
            .update_timezone(&UserId::new(), &timezone)
            .await
            .unwrap();
        assert!(!updated);
    }
}
