//! SQLite user profile store implementation
//!
//! Implements the `UserProfileStore` port using SQLite.

use std::sync::Arc;

use application::{error::ApplicationError, ports::UserProfileStore};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use domain::{
    entities::UserProfile,
    value_objects::{GeoLocation, Timezone, UserId},
};
use rusqlite::{OptionalExtension, Row, params};
use tokio::task;
use tracing::{debug, instrument};

use super::connection::ConnectionPool;

/// SQLite-based user profile store
#[derive(Debug, Clone)]
pub struct SqliteUserProfileStore {
    pool: Arc<ConnectionPool>,
}

impl SqliteUserProfileStore {
    /// Create a new SQLite user profile store
    #[must_use]
    pub const fn new(pool: Arc<ConnectionPool>) -> Self {
        Self { pool }
    }
}

/// Convert a database row to a `UserProfile`
fn row_to_profile(row: &Row<'_>) -> Result<UserProfile, rusqlite::Error> {
    let user_id_str: String = row.get(0)?;
    let latitude: Option<f64> = row.get(1)?;
    let longitude: Option<f64> = row.get(2)?;
    let timezone_str: String = row.get(3)?;
    let created_at_str: String = row.get(4)?;
    let updated_at_str: String = row.get(5)?;

    let user_id = UserId::parse(&user_id_str).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(e))
    })?;

    let location = match (latitude, longitude) {
        (Some(lat), Some(lon)) => GeoLocation::new(lat, lon).ok(),
        _ => None,
    };

    // Validate timezone; fall back to UTC if invalid (for legacy data)
    let timezone = Timezone::try_new(&timezone_str).unwrap_or_else(|_| {
        tracing::warn!(
            timezone = %timezone_str,
            "Invalid timezone in database, falling back to UTC"
        );
        Timezone::utc()
    });

    let created_at = DateTime::parse_from_rfc3339(&created_at_str)
        .map_or_else(|_| Utc::now(), |dt| dt.with_timezone(&Utc));

    let updated_at = DateTime::parse_from_rfc3339(&updated_at_str)
        .map_or_else(|_| Utc::now(), |dt| dt.with_timezone(&Utc));

    Ok(UserProfile::restore(
        user_id, location, timezone, created_at, updated_at,
    ))
}

#[async_trait]
impl UserProfileStore for SqliteUserProfileStore {
    #[instrument(skip(self, profile), fields(user_id = %profile.id()))]
    async fn save(&self, profile: &UserProfile) -> Result<(), ApplicationError> {
        let pool = Arc::clone(&self.pool);
        let profile = profile.clone();
        let now = Utc::now().to_rfc3339();

        task::spawn_blocking(move || {
            let conn = pool
                .get()
                .map_err(|e| ApplicationError::Internal(e.to_string()))?;

            let (latitude, longitude) = profile
                .location()
                .map_or((None, None), |loc| (Some(loc.latitude()), Some(loc.longitude())));

            conn.execute(
                "INSERT INTO user_profiles (user_id, latitude, longitude, timezone, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?5)
                 ON CONFLICT(user_id) DO UPDATE SET
                     latitude = excluded.latitude,
                     longitude = excluded.longitude,
                     timezone = excluded.timezone,
                     updated_at = excluded.updated_at",
                params![
                    profile.id().to_string(),
                    latitude,
                    longitude,
                    profile.timezone().as_str(),
                    now,
                ],
            )
            .map_err(|e| ApplicationError::Internal(e.to_string()))?;

            debug!("Saved user profile");
            Ok(())
        })
        .await
        .map_err(|e| ApplicationError::Internal(e.to_string()))?
    }

    #[instrument(skip(self), fields(user_id = %user_id))]
    async fn get(&self, user_id: &UserId) -> Result<Option<UserProfile>, ApplicationError> {
        let pool = Arc::clone(&self.pool);
        let user_id_str = user_id.to_string();

        task::spawn_blocking(move || {
            let conn = pool
                .get()
                .map_err(|e| ApplicationError::Internal(e.to_string()))?;

            let profile = conn
                .query_row(
                    "SELECT user_id, latitude, longitude, timezone, created_at, updated_at
                     FROM user_profiles WHERE user_id = ?1",
                    [&user_id_str],
                    row_to_profile,
                )
                .optional()
                .map_err(|e| ApplicationError::Internal(e.to_string()))?;

            debug!(found = profile.is_some(), "Retrieved user profile");
            Ok(profile)
        })
        .await
        .map_err(|e| ApplicationError::Internal(e.to_string()))?
    }

    #[instrument(skip(self), fields(user_id = %user_id))]
    async fn delete(&self, user_id: &UserId) -> Result<bool, ApplicationError> {
        let pool = Arc::clone(&self.pool);
        let user_id_str = user_id.to_string();

        task::spawn_blocking(move || {
            let conn = pool
                .get()
                .map_err(|e| ApplicationError::Internal(e.to_string()))?;

            let deleted = conn
                .execute(
                    "DELETE FROM user_profiles WHERE user_id = ?1",
                    [&user_id_str],
                )
                .map_err(|e| ApplicationError::Internal(e.to_string()))?;

            debug!(deleted = deleted > 0, "Deleted user profile");
            Ok(deleted > 0)
        })
        .await
        .map_err(|e| ApplicationError::Internal(e.to_string()))?
    }

    #[instrument(skip(self), fields(user_id = %user_id))]
    async fn update_location(
        &self,
        user_id: &UserId,
        location: Option<&GeoLocation>,
    ) -> Result<bool, ApplicationError> {
        let pool = Arc::clone(&self.pool);
        let user_id_str = user_id.to_string();
        let (latitude, longitude) = location.map_or((None, None), |loc| {
            (Some(loc.latitude()), Some(loc.longitude()))
        });
        let now = Utc::now().to_rfc3339();

        task::spawn_blocking(move || {
            let conn = pool
                .get()
                .map_err(|e| ApplicationError::Internal(e.to_string()))?;

            let updated = conn
                .execute(
                    "UPDATE user_profiles SET latitude = ?1, longitude = ?2, updated_at = ?3
                     WHERE user_id = ?4",
                    params![latitude, longitude, now, user_id_str],
                )
                .map_err(|e| ApplicationError::Internal(e.to_string()))?;

            debug!(updated = updated > 0, "Updated user location");
            Ok(updated > 0)
        })
        .await
        .map_err(|e| ApplicationError::Internal(e.to_string()))?
    }

    #[instrument(skip(self), fields(user_id = %user_id))]
    async fn update_timezone(
        &self,
        user_id: &UserId,
        timezone: &Timezone,
    ) -> Result<bool, ApplicationError> {
        let pool = Arc::clone(&self.pool);
        let user_id_str = user_id.to_string();
        let tz_str = timezone.as_str().to_owned();
        let now = Utc::now().to_rfc3339();

        task::spawn_blocking(move || {
            let conn = pool
                .get()
                .map_err(|e| ApplicationError::Internal(e.to_string()))?;

            let updated = conn
                .execute(
                    "UPDATE user_profiles SET timezone = ?1, updated_at = ?2
                     WHERE user_id = ?3",
                    params![tz_str, now, user_id_str],
                )
                .map_err(|e| ApplicationError::Internal(e.to_string()))?;

            debug!(updated = updated > 0, "Updated user timezone");
            Ok(updated > 0)
        })
        .await
        .map_err(|e| ApplicationError::Internal(e.to_string()))?
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::DatabaseConfig;
    use crate::persistence::{create_pool, migrations::run_migrations};
    use std::sync::Arc;

    fn memory_config() -> DatabaseConfig {
        DatabaseConfig {
            path: ":memory:".to_string(),
            max_connections: 1,
            run_migrations: false, // We'll run migrations manually for more control
        }
    }

    fn setup_test_db() -> Arc<ConnectionPool> {
        let pool = create_pool(&memory_config()).unwrap();
        {
            let conn = pool.get().unwrap();
            run_migrations(&conn).unwrap();
        }
        Arc::new(pool)
    }

    #[tokio::test]
    async fn save_and_get_profile() {
        let pool = setup_test_db();
        let store = SqliteUserProfileStore::new(pool);

        let user_id = UserId::new();
        let location = GeoLocation::new(52.52, 13.405).unwrap();
        let timezone = Timezone::try_new("Europe/Berlin").expect("valid tz");
        let profile = UserProfile::with_defaults(user_id, location, timezone);

        // Save profile
        store.save(&profile).await.unwrap();

        // Get profile
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
        let pool = setup_test_db();
        let store = SqliteUserProfileStore::new(pool);

        let user_id = UserId::new();
        let profile = UserProfile::new(user_id);

        // Save profile
        store.save(&profile).await.unwrap();

        // Get profile
        let retrieved = store.get(&profile.id()).await.unwrap().unwrap();
        assert!(retrieved.location().is_none());
        assert!(retrieved.timezone().is_utc());
    }

    #[tokio::test]
    async fn update_existing_profile() {
        let pool = setup_test_db();
        let store = SqliteUserProfileStore::new(pool);

        let user_id = UserId::new();
        let profile = UserProfile::new(user_id);

        // Save initial profile
        store.save(&profile).await.unwrap();

        // Update with location
        let location = GeoLocation::new(48.8566, 2.3522).unwrap();
        let new_tz = Timezone::try_new("Europe/Paris").expect("valid tz");
        let updated_profile = UserProfile::with_defaults(profile.id(), location, new_tz);
        store.save(&updated_profile).await.unwrap();

        // Verify update
        let retrieved = store.get(&profile.id()).await.unwrap().unwrap();
        assert!(retrieved.location().is_some());
        assert_eq!(retrieved.timezone().as_str(), "Europe/Paris");
    }

    #[tokio::test]
    async fn get_nonexistent_profile() {
        let pool = setup_test_db();
        let store = SqliteUserProfileStore::new(pool);

        let user_id = UserId::new();
        let result = store.get(&user_id).await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn delete_profile() {
        let pool = setup_test_db();
        let store = SqliteUserProfileStore::new(pool);

        let user_id = UserId::new();
        let profile = UserProfile::new(user_id);

        // Save profile
        store.save(&profile).await.unwrap();

        // Delete profile
        let deleted = store.delete(&profile.id()).await.unwrap();
        assert!(deleted);

        // Verify deletion
        let result = store.get(&profile.id()).await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn delete_nonexistent_profile() {
        let pool = setup_test_db();
        let store = SqliteUserProfileStore::new(pool);

        let user_id = UserId::new();
        let deleted = store.delete(&user_id).await.unwrap();
        assert!(!deleted);
    }

    #[tokio::test]
    async fn update_location_only() {
        let pool = setup_test_db();
        let store = SqliteUserProfileStore::new(pool);

        let user_id = UserId::new();
        let mut profile = UserProfile::new(user_id);
        profile.update_timezone(Timezone::try_new("America/New_York").expect("valid tz"));

        // Save profile
        store.save(&profile).await.unwrap();

        // Update location
        let location = GeoLocation::new(40.7128, -74.0060).unwrap();
        let updated = store
            .update_location(&profile.id(), Some(&location))
            .await
            .unwrap();
        assert!(updated);

        // Verify - location updated, timezone unchanged
        let retrieved = store.get(&profile.id()).await.unwrap().unwrap();
        assert!(retrieved.location().is_some());
        let loc = retrieved.location().unwrap();
        assert!((loc.latitude() - 40.7128).abs() < 0.001);
        assert_eq!(retrieved.timezone().as_str(), "America/New_York");
    }

    #[tokio::test]
    async fn update_location_nonexistent_profile() {
        let pool = setup_test_db();
        let store = SqliteUserProfileStore::new(pool);

        let user_id = UserId::new();
        let location = GeoLocation::new(40.7128, -74.0060).unwrap();
        let updated = store
            .update_location(&user_id, Some(&location))
            .await
            .unwrap();
        assert!(!updated);
    }

    #[tokio::test]
    async fn clear_location() {
        let pool = setup_test_db();
        let store = SqliteUserProfileStore::new(pool);

        let location = GeoLocation::new(40.7128, -74.0060).unwrap();
        let profile = UserProfile::with_defaults(UserId::new(), location, Timezone::default());

        // Save profile with location
        store.save(&profile).await.unwrap();

        // Clear location
        let updated = store.update_location(&profile.id(), None).await.unwrap();
        assert!(updated);

        // Verify location is cleared
        let retrieved = store.get(&profile.id()).await.unwrap().unwrap();
        assert!(retrieved.location().is_none());
    }

    #[tokio::test]
    async fn update_timezone_only() {
        let pool = setup_test_db();
        let store = SqliteUserProfileStore::new(pool);

        let location = GeoLocation::new(51.5074, -0.1278).unwrap();
        let profile = UserProfile::with_defaults(UserId::new(), location, Timezone::default());

        // Save profile
        store.save(&profile).await.unwrap();

        // Update timezone
        let new_tz = Timezone::try_new("Europe/London").expect("valid tz");
        let updated = store.update_timezone(&profile.id(), &new_tz).await.unwrap();
        assert!(updated);

        // Verify - timezone updated, location unchanged
        let retrieved = store.get(&profile.id()).await.unwrap().unwrap();
        assert_eq!(retrieved.timezone().as_str(), "Europe/London");
        assert!(retrieved.location().is_some());
    }

    #[tokio::test]
    async fn update_timezone_nonexistent_profile() {
        let pool = setup_test_db();
        let store = SqliteUserProfileStore::new(pool);

        let user_id = UserId::new();
        let timezone = Timezone::try_new("Asia/Tokyo").expect("valid tz");
        let updated = store.update_timezone(&user_id, &timezone).await.unwrap();
        assert!(!updated);
    }
}
