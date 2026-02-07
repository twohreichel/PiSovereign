//! Location update handlers
//!
//! Endpoints for updating user location for contextual services.

use std::sync::Arc;

use application::ports::UserProfileStore;
use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
};
use domain::value_objects::{GeoLocation, UserId};
use serde::{Deserialize, Serialize};
use tracing::{info, instrument, warn};

use crate::error::ApiError;

/// Request body for updating location
#[derive(Debug, Deserialize)]
pub struct UpdateLocationRequest {
    /// Latitude (-90 to 90)
    pub latitude: f64,
    /// Longitude (-180 to 180)
    pub longitude: f64,
}

/// Response for location operations
#[derive(Debug, Serialize)]
pub struct LocationResponse {
    /// Whether the operation succeeded
    pub success: bool,
    /// Optional message
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    /// Current location if available
    #[serde(skip_serializing_if = "Option::is_none")]
    pub location: Option<LocationData>,
}

/// Location data in responses
#[derive(Debug, Serialize)]
pub struct LocationData {
    /// Latitude
    pub latitude: f64,
    /// Longitude
    pub longitude: f64,
}

impl From<&GeoLocation> for LocationData {
    fn from(loc: &GeoLocation) -> Self {
        Self {
            latitude: loc.latitude(),
            longitude: loc.longitude(),
        }
    }
}

impl From<GeoLocation> for LocationData {
    fn from(loc: GeoLocation) -> Self {
        Self {
            latitude: loc.latitude(),
            longitude: loc.longitude(),
        }
    }
}

/// State required for location handlers
#[derive(Clone)]
pub struct LocationState {
    /// User profile store for persisting location
    pub profile_store: Arc<dyn UserProfileStore>,
}

impl std::fmt::Debug for LocationState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LocationState")
            .field("profile_store", &"<UserProfileStore>")
            .finish()
    }
}

/// Update user location
///
/// PUT /v1/users/{user_id}/location
#[instrument(skip(state), fields(user_id = %user_id))]
pub async fn update_location(
    State(state): State<LocationState>,
    Path(user_id): Path<String>,
    Json(request): Json<UpdateLocationRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let user_id = parse_user_id(&user_id)?;

    // Validate coordinates
    let location = GeoLocation::new(request.latitude, request.longitude).map_err(|_| {
        ApiError::BadRequest(
            "Invalid coordinates: latitude must be -90 to 90, longitude must be -180 to 180"
                .to_string(),
        )
    })?;

    info!(
        latitude = location.latitude(),
        longitude = location.longitude(),
        "Updating user location"
    );

    // Update in store
    let updated = state
        .profile_store
        .update_location(&user_id, Some(&location))
        .await
        .map_err(|e| {
            warn!(error = %e, "Failed to update location");
            ApiError::Internal(format!("Failed to update location: {e}"))
        })?;

    if !updated {
        return Err(ApiError::NotFound(format!("User {user_id} not found")));
    }

    Ok((
        StatusCode::OK,
        Json(LocationResponse {
            success: true,
            message: Some("Location updated".to_string()),
            location: Some(LocationData::from(&location)),
        }),
    ))
}

/// Clear user location
///
/// DELETE /v1/users/{user_id}/location
#[instrument(skip(state), fields(user_id = %user_id))]
pub async fn clear_location(
    State(state): State<LocationState>,
    Path(user_id): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    let user_id = parse_user_id(&user_id)?;

    info!("Clearing user location");

    let updated = state
        .profile_store
        .update_location(&user_id, None)
        .await
        .map_err(|e| {
            warn!(error = %e, "Failed to clear location");
            ApiError::Internal(format!("Failed to clear location: {e}"))
        })?;

    if !updated {
        return Err(ApiError::NotFound(format!("User {user_id} not found")));
    }

    Ok((
        StatusCode::OK,
        Json(LocationResponse {
            success: true,
            message: Some("Location cleared".to_string()),
            location: None,
        }),
    ))
}

/// Get current user location
///
/// GET /v1/users/{user_id}/location
#[instrument(skip(state), fields(user_id = %user_id))]
pub async fn get_location(
    State(state): State<LocationState>,
    Path(user_id): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    let user_id_val = parse_user_id(&user_id)?;

    let profile = state.profile_store.get(&user_id_val).await.map_err(|e| {
        warn!(error = %e, "Failed to get user profile");
        ApiError::Internal(format!("Failed to get location: {e}"))
    })?;

    profile.map_or_else(
        || Err(ApiError::NotFound(format!("User {user_id} not found"))),
        |p| {
            Ok((
                StatusCode::OK,
                Json(LocationResponse {
                    success: true,
                    message: None,
                    location: p.location().map(LocationData::from),
                }),
            ))
        },
    )
}

/// Parse user ID from string
fn parse_user_id(s: &str) -> Result<UserId, ApiError> {
    UserId::parse(s).map_err(|_| ApiError::BadRequest(format!("Invalid user ID: {s}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_location_data_from_geo_location() {
        let geo = GeoLocation::new(52.52, 13.405).unwrap();
        let data = LocationData::from(&geo);

        assert!((data.latitude - 52.52).abs() < 0.001);
        assert!((data.longitude - 13.405).abs() < 0.001);
    }

    #[test]
    fn test_parse_user_id_valid() {
        let uuid = "550e8400-e29b-41d4-a716-446655440000";
        let result = parse_user_id(uuid);
        assert!(result.is_ok());
    }

    #[test]
    fn test_parse_user_id_invalid() {
        let result = parse_user_id("not-a-uuid");
        assert!(result.is_err());
    }
}
