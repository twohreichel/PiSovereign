//! Contact management handlers
//!
//! REST API endpoints for CardDAV contact operations.

use application::ports::{ContactError, ContactPort, ContactUpdate, NewContact};
use axum::{
    Json,
    extract::{Path, State},
};
use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use tracing::{debug, instrument};
use utoipa::{IntoParams, ToSchema};

use crate::{error::ApiError, state::AppState};

// ---------------------------------------------------------------------------
// Response / request DTOs
// ---------------------------------------------------------------------------

/// Contact summary for list views
#[derive(Debug, Serialize, ToSchema)]
#[schema(example = json!({
    "id": "550e8400-e29b-41d4-a716-446655440000",
    "display_name": "Alice Smith",
    "email": "alice@example.com",
    "phone": "+49 170 1234567",
    "organization": "Acme Corp"
}))]
pub struct ContactResponse {
    /// Unique contact ID
    pub id: String,
    /// Display name
    pub display_name: String,
    /// Primary email address
    #[serde(skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,
    /// Primary phone number
    #[serde(skip_serializing_if = "Option::is_none")]
    pub phone: Option<String>,
    /// Organization name
    #[serde(skip_serializing_if = "Option::is_none")]
    pub organization: Option<String>,
}

/// Detailed contact information
#[derive(Debug, Serialize, ToSchema)]
#[schema(example = json!({
    "id": "550e8400-e29b-41d4-a716-446655440000",
    "display_name": "Alice Smith",
    "first_name": "Alice",
    "last_name": "Smith",
    "emails": ["alice@work.com", "alice@home.com"],
    "phones": ["+49 170 1234567"],
    "organization": "Acme Corp",
    "title": "Software Engineer",
    "addresses": ["123 Main St, Berlin 10115, Germany"],
    "birthday": "1990-05-15",
    "notes": "Met at conference",
    "categories": ["Work", "VIP"]
}))]
pub struct ContactDetailResponse {
    /// Unique contact ID
    pub id: String,
    /// Display name
    pub display_name: String,
    /// First name
    #[serde(skip_serializing_if = "Option::is_none")]
    pub first_name: Option<String>,
    /// Last name
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_name: Option<String>,
    /// Email addresses
    pub emails: Vec<String>,
    /// Phone numbers
    pub phones: Vec<String>,
    /// Organization name
    #[serde(skip_serializing_if = "Option::is_none")]
    pub organization: Option<String>,
    /// Job title
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// Formatted addresses
    pub addresses: Vec<String>,
    /// Birthday (YYYY-MM-DD)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub birthday: Option<NaiveDate>,
    /// Notes
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
    /// Photo URL
    #[serde(skip_serializing_if = "Option::is_none")]
    pub photo_url: Option<String>,
    /// Categories/tags
    pub categories: Vec<String>,
}

/// Create contact request body
#[derive(Debug, Deserialize, ToSchema)]
#[schema(example = json!({
    "name": "Alice Smith",
    "email": "alice@example.com",
    "phone": "+49 170 1234567",
    "organization": "Acme Corp",
    "birthday": "1990-05-15",
    "notes": "Met at conference"
}))]
pub struct CreateContactRequest {
    /// Display name (required)
    pub name: String,
    /// First name
    #[serde(default)]
    pub first_name: Option<String>,
    /// Last name
    #[serde(default)]
    pub last_name: Option<String>,
    /// Email address
    #[serde(default)]
    pub email: Option<String>,
    /// Phone number
    #[serde(default)]
    pub phone: Option<String>,
    /// Organization name
    #[serde(default)]
    pub organization: Option<String>,
    /// Birthday (YYYY-MM-DD)
    #[serde(default)]
    pub birthday: Option<NaiveDate>,
    /// Notes
    #[serde(default)]
    pub notes: Option<String>,
}

/// Update contact request body (all fields optional)
#[derive(Debug, Deserialize, ToSchema)]
#[schema(example = json!({
    "name": "Alice Johnson",
    "email": "alice.johnson@example.com"
}))]
pub struct UpdateContactRequest {
    /// New display name
    #[serde(default)]
    pub name: Option<String>,
    /// New email address
    #[serde(default)]
    pub email: Option<String>,
    /// New phone number
    #[serde(default)]
    pub phone: Option<String>,
    /// New organization
    #[serde(default)]
    pub organization: Option<String>,
    /// New notes
    #[serde(default)]
    pub notes: Option<String>,
}

/// List contacts query parameters
#[derive(Debug, Deserialize, IntoParams, ToSchema)]
pub struct ListContactsQuery {
    /// Optional search query to filter contacts
    pub q: Option<String>,
}

/// Search contacts request body
#[derive(Debug, Deserialize, ToSchema)]
#[schema(example = json!({"query": "alice"}))]
pub struct SearchContactsRequest {
    /// Search query string
    pub query: String,
}

/// Created contact response
#[derive(Debug, Serialize, ToSchema)]
#[schema(example = json!({"id": "550e8400-e29b-41d4-a716-446655440000"}))]
pub struct CreatedContactResponse {
    /// ID of the created contact
    pub id: String,
}

/// Addressbook info response
#[derive(Debug, Serialize, ToSchema)]
#[schema(example = json!({
    "id": "/dav.php/addressbooks/user/default/",
    "name": "default",
    "is_default": true
}))]
pub struct AddressbookResponse {
    /// Addressbook path/ID
    pub id: String,
    /// Human-readable name
    pub name: String,
    /// Whether this is the default addressbook
    pub is_default: bool,
}

// ---------------------------------------------------------------------------
// Helper: extract contact_port from state
// ---------------------------------------------------------------------------

fn contact_port(state: &AppState) -> Result<&dyn ContactPort, ApiError> {
    state
        .contact_service
        .as_deref()
        .ok_or_else(|| ApiError::ServiceUnavailable("Contact service not configured".to_string()))
}

/// Map `ContactError` to `ApiError`
fn map_contact_error(error: ContactError) -> ApiError {
    match error {
        ContactError::ServiceUnavailable => {
            ApiError::ServiceUnavailable("CardDAV service unavailable".to_string())
        },
        ContactError::AuthenticationFailed => {
            ApiError::Unauthorized("CardDAV authentication failed".to_string())
        },
        ContactError::AddressbookNotFound(msg) | ContactError::ContactNotFound(msg) => {
            ApiError::NotFound(msg)
        },
        ContactError::InvalidData(msg) => ApiError::BadRequest(msg),
        ContactError::OperationFailed(msg) => ApiError::Internal(msg),
    }
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

/// List available addressbooks
///
/// GET /v1/contacts/addressbooks
#[utoipa::path(
    get,
    path = "/v1/contacts/addressbooks",
    tag = "contacts",
    responses(
        (status = 200, description = "List of addressbooks", body = Vec<AddressbookResponse>),
        (status = 503, description = "Service unavailable", body = crate::error::ErrorResponse)
    ),
    security(("api_key" = []))
)]
#[instrument(skip(state))]
pub async fn list_addressbooks(
    State(state): State<AppState>,
) -> Result<Json<Vec<AddressbookResponse>>, ApiError> {
    let port = contact_port(&state)?;

    let books = port.list_addressbooks().await.map_err(map_contact_error)?;

    let response: Vec<AddressbookResponse> = books
        .into_iter()
        .map(|b| AddressbookResponse {
            id: b.id,
            name: b.name,
            is_default: b.is_default,
        })
        .collect();

    debug!(count = response.len(), "Listed addressbooks");
    Ok(Json(response))
}

/// List contacts, optionally filtered by query
///
/// GET /v1/contacts
#[utoipa::path(
    get,
    path = "/v1/contacts",
    tag = "contacts",
    params(ListContactsQuery),
    responses(
        (status = 200, description = "List of contacts", body = Vec<ContactResponse>),
        (status = 503, description = "Service unavailable", body = crate::error::ErrorResponse)
    ),
    security(("api_key" = []))
)]
#[instrument(skip(state))]
pub async fn list_contacts(
    State(state): State<AppState>,
    axum::extract::Query(query): axum::extract::Query<ListContactsQuery>,
) -> Result<Json<Vec<ContactResponse>>, ApiError> {
    let port = contact_port(&state)?;

    let contacts = port
        .list_contacts(query.q)
        .await
        .map_err(map_contact_error)?;

    let response: Vec<ContactResponse> = contacts
        .into_iter()
        .map(|c| ContactResponse {
            id: c.id,
            display_name: c.display_name,
            email: c.email,
            phone: c.phone,
            organization: c.organization,
        })
        .collect();

    debug!(count = response.len(), "Listed contacts");
    Ok(Json(response))
}

/// Get a contact by ID
///
/// GET /v1/contacts/:id
#[utoipa::path(
    get,
    path = "/v1/contacts/{id}",
    tag = "contacts",
    params(
        ("id" = String, Path, description = "Contact ID (vCard UID)")
    ),
    responses(
        (status = 200, description = "Contact details", body = ContactDetailResponse),
        (status = 404, description = "Contact not found", body = crate::error::ErrorResponse),
        (status = 503, description = "Service unavailable", body = crate::error::ErrorResponse)
    ),
    security(("api_key" = []))
)]
#[instrument(skip(state))]
pub async fn get_contact(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<ContactDetailResponse>, ApiError> {
    let port = contact_port(&state)?;

    let detail = port.get_contact(&id).await.map_err(map_contact_error)?;

    Ok(Json(ContactDetailResponse {
        id: detail.id,
        display_name: detail.display_name,
        first_name: detail.first_name,
        last_name: detail.last_name,
        emails: detail.emails,
        phones: detail.phones,
        organization: detail.organization,
        title: detail.title,
        addresses: detail.addresses,
        birthday: detail.birthday,
        notes: detail.notes,
        photo_url: detail.photo_url,
        categories: detail.categories,
    }))
}

/// Create a new contact
///
/// POST /v1/contacts
#[utoipa::path(
    post,
    path = "/v1/contacts",
    tag = "contacts",
    request_body = CreateContactRequest,
    responses(
        (status = 201, description = "Contact created", body = CreatedContactResponse),
        (status = 400, description = "Invalid request", body = crate::error::ErrorResponse),
        (status = 503, description = "Service unavailable", body = crate::error::ErrorResponse)
    ),
    security(("api_key" = []))
)]
#[instrument(skip(state))]
pub async fn create_contact(
    State(state): State<AppState>,
    Json(body): Json<CreateContactRequest>,
) -> Result<(axum::http::StatusCode, Json<CreatedContactResponse>), ApiError> {
    let port = contact_port(&state)?;

    if body.name.trim().is_empty() {
        return Err(ApiError::BadRequest("Name is required".to_string()));
    }

    let mut new = NewContact::new(&body.name);
    if let Some(ref first) = body.first_name {
        new.first_name = Some(first.clone());
    }
    if let Some(ref last) = body.last_name {
        new.last_name = Some(last.clone());
    }
    if let Some(ref email) = body.email {
        new = new.with_email(email);
    }
    if let Some(ref phone) = body.phone {
        new = new.with_phone(phone);
    }
    if let Some(ref org) = body.organization {
        new = new.with_organization(org);
    }
    if let Some(birthday) = body.birthday {
        new = new.with_birthday(birthday);
    }
    if let Some(ref notes) = body.notes {
        new = new.with_notes(notes);
    }

    let id = port.create_contact(&new).await.map_err(map_contact_error)?;

    debug!(id = %id, name = %body.name, "Created contact");
    Ok((
        axum::http::StatusCode::CREATED,
        Json(CreatedContactResponse { id }),
    ))
}

/// Update an existing contact
///
/// PUT /v1/contacts/:id
#[utoipa::path(
    put,
    path = "/v1/contacts/{id}",
    tag = "contacts",
    params(
        ("id" = String, Path, description = "Contact ID (vCard UID)")
    ),
    request_body = UpdateContactRequest,
    responses(
        (status = 204, description = "Contact updated"),
        (status = 400, description = "Invalid request", body = crate::error::ErrorResponse),
        (status = 404, description = "Contact not found", body = crate::error::ErrorResponse),
        (status = 503, description = "Service unavailable", body = crate::error::ErrorResponse)
    ),
    security(("api_key" = []))
)]
#[instrument(skip(state))]
pub async fn update_contact(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<UpdateContactRequest>,
) -> Result<axum::http::StatusCode, ApiError> {
    let port = contact_port(&state)?;

    let mut update = ContactUpdate::new();
    if let Some(ref name) = body.name {
        update = update.with_name(name);
    }
    if let Some(ref email) = body.email {
        update = update.with_email(email);
    }
    if let Some(ref phone) = body.phone {
        update = update.with_phone(phone);
    }
    if let Some(ref org) = body.organization {
        update = update.with_organization(org);
    }
    if let Some(ref notes) = body.notes {
        update = update.with_notes(notes);
    }

    if !update.has_changes() {
        return Err(ApiError::BadRequest(
            "No fields to update provided".to_string(),
        ));
    }

    port.update_contact(&id, &update)
        .await
        .map_err(map_contact_error)?;

    debug!(id = %id, "Updated contact");
    Ok(axum::http::StatusCode::NO_CONTENT)
}

/// Delete a contact
///
/// DELETE /v1/contacts/:id
#[utoipa::path(
    delete,
    path = "/v1/contacts/{id}",
    tag = "contacts",
    params(
        ("id" = String, Path, description = "Contact ID (vCard UID)")
    ),
    responses(
        (status = 204, description = "Contact deleted"),
        (status = 404, description = "Contact not found", body = crate::error::ErrorResponse),
        (status = 503, description = "Service unavailable", body = crate::error::ErrorResponse)
    ),
    security(("api_key" = []))
)]
#[instrument(skip(state))]
pub async fn delete_contact(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<axum::http::StatusCode, ApiError> {
    let port = contact_port(&state)?;

    port.delete_contact(&id)
        .await
        .map_err(map_contact_error)?;

    debug!(id = %id, "Deleted contact");
    Ok(axum::http::StatusCode::NO_CONTENT)
}

/// Search contacts by query
///
/// POST /v1/contacts/search
#[utoipa::path(
    post,
    path = "/v1/contacts/search",
    tag = "contacts",
    request_body = SearchContactsRequest,
    responses(
        (status = 200, description = "Search results", body = Vec<ContactResponse>),
        (status = 400, description = "Invalid request", body = crate::error::ErrorResponse),
        (status = 503, description = "Service unavailable", body = crate::error::ErrorResponse)
    ),
    security(("api_key" = []))
)]
#[instrument(skip(state))]
pub async fn search_contacts(
    State(state): State<AppState>,
    Json(body): Json<SearchContactsRequest>,
) -> Result<Json<Vec<ContactResponse>>, ApiError> {
    let port = contact_port(&state)?;

    if body.query.trim().is_empty() {
        return Err(ApiError::BadRequest(
            "Search query is required".to_string(),
        ));
    }

    let contacts = port
        .search_contacts(&body.query)
        .await
        .map_err(map_contact_error)?;

    let response: Vec<ContactResponse> = contacts
        .into_iter()
        .map(|c| ContactResponse {
            id: c.id,
            display_name: c.display_name,
            email: c.email,
            phone: c.phone,
            organization: c.organization,
        })
        .collect();

    debug!(count = response.len(), query = %body.query, "Searched contacts");
    Ok(Json(response))
}
