//! Contact port for application layer
//!
//! Defines the interface for contact management operations (CardDAV).
//! Implemented by adapters in the infrastructure layer.

use async_trait::async_trait;
use chrono::NaiveDate;
#[cfg(test)]
use mockall::automock;
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Contact port errors
#[derive(Debug, Error)]
pub enum ContactError {
    /// The contact service is unavailable
    #[error("Contact service unavailable")]
    ServiceUnavailable,

    /// Authentication with the contact service failed
    #[error("Authentication failed")]
    AuthenticationFailed,

    /// Addressbook not found
    #[error("Addressbook not found: {0}")]
    AddressbookNotFound(String),

    /// Contact not found
    #[error("Contact not found: {0}")]
    ContactNotFound(String),

    /// A write or read operation failed
    #[error("Operation failed: {0}")]
    OperationFailed(String),

    /// Invalid input data
    #[error("Invalid data: {0}")]
    InvalidData(String),
}

/// Contact summary (for list views)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ContactSummary {
    /// Unique contact identifier (from CardDAV href)
    pub id: String,
    /// Display name
    pub display_name: String,
    /// Primary email address
    pub email: Option<String>,
    /// Primary phone number
    pub phone: Option<String>,
    /// Organization name
    pub organization: Option<String>,
}

impl ContactSummary {
    /// Create a new contact summary
    pub fn new(id: impl Into<String>, display_name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            display_name: display_name.into(),
            email: None,
            phone: None,
            organization: None,
        }
    }

    /// Set the primary email
    #[must_use]
    pub fn with_email(mut self, email: impl Into<String>) -> Self {
        self.email = Some(email.into());
        self
    }

    /// Set the primary phone
    #[must_use]
    pub fn with_phone(mut self, phone: impl Into<String>) -> Self {
        self.phone = Some(phone.into());
        self
    }

    /// Set the organization
    #[must_use]
    pub fn with_organization(mut self, organization: impl Into<String>) -> Self {
        self.organization = Some(organization.into());
        self
    }
}

/// Full contact details
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ContactDetail {
    /// Unique contact identifier (from CardDAV href)
    pub id: String,
    /// Display name
    pub display_name: String,
    /// First name
    pub first_name: Option<String>,
    /// Last name
    pub last_name: Option<String>,
    /// Email addresses
    pub emails: Vec<String>,
    /// Phone numbers
    pub phones: Vec<String>,
    /// Organization name
    pub organization: Option<String>,
    /// Job title
    pub title: Option<String>,
    /// Formatted addresses
    pub addresses: Vec<String>,
    /// Birthday
    pub birthday: Option<NaiveDate>,
    /// Notes
    pub notes: Option<String>,
    /// Photo URL
    pub photo_url: Option<String>,
    /// Categories / tags
    pub categories: Vec<String>,
}

impl ContactDetail {
    /// Create a new contact detail with minimal fields
    pub fn new(id: impl Into<String>, display_name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            display_name: display_name.into(),
            first_name: None,
            last_name: None,
            emails: Vec::new(),
            phones: Vec::new(),
            organization: None,
            title: None,
            addresses: Vec::new(),
            birthday: None,
            notes: None,
            photo_url: None,
            categories: Vec::new(),
        }
    }

    /// Set the first name
    #[must_use]
    pub fn with_first_name(mut self, first_name: impl Into<String>) -> Self {
        self.first_name = Some(first_name.into());
        self
    }

    /// Set the last name
    #[must_use]
    pub fn with_last_name(mut self, last_name: impl Into<String>) -> Self {
        self.last_name = Some(last_name.into());
        self
    }

    /// Add an email address
    #[must_use]
    pub fn with_email(mut self, email: impl Into<String>) -> Self {
        self.emails.push(email.into());
        self
    }

    /// Add a phone number
    #[must_use]
    pub fn with_phone(mut self, phone: impl Into<String>) -> Self {
        self.phones.push(phone.into());
        self
    }

    /// Set the organization
    #[must_use]
    pub fn with_organization(mut self, organization: impl Into<String>) -> Self {
        self.organization = Some(organization.into());
        self
    }

    /// Set the job title
    #[must_use]
    pub fn with_title(mut self, title: impl Into<String>) -> Self {
        self.title = Some(title.into());
        self
    }

    /// Set the birthday
    #[must_use]
    pub const fn with_birthday(mut self, birthday: NaiveDate) -> Self {
        self.birthday = Some(birthday);
        self
    }

    /// Set notes
    #[must_use]
    pub fn with_notes(mut self, notes: impl Into<String>) -> Self {
        self.notes = Some(notes.into());
        self
    }
}

/// New contact request (for creating contacts)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewContact {
    /// Display name (required)
    pub name: String,
    /// First name
    pub first_name: Option<String>,
    /// Last name
    pub last_name: Option<String>,
    /// Email address
    pub email: Option<String>,
    /// Phone number
    pub phone: Option<String>,
    /// Organization name
    pub organization: Option<String>,
    /// Birthday (YYYY-MM-DD)
    pub birthday: Option<NaiveDate>,
    /// Notes
    pub notes: Option<String>,
}

impl NewContact {
    /// Create a new contact request with a display name
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            first_name: None,
            last_name: None,
            email: None,
            phone: None,
            organization: None,
            birthday: None,
            notes: None,
        }
    }

    /// Set the email address
    #[must_use]
    pub fn with_email(mut self, email: impl Into<String>) -> Self {
        self.email = Some(email.into());
        self
    }

    /// Set the phone number
    #[must_use]
    pub fn with_phone(mut self, phone: impl Into<String>) -> Self {
        self.phone = Some(phone.into());
        self
    }

    /// Set the organization
    #[must_use]
    pub fn with_organization(mut self, organization: impl Into<String>) -> Self {
        self.organization = Some(organization.into());
        self
    }

    /// Set the birthday
    #[must_use]
    pub const fn with_birthday(mut self, birthday: NaiveDate) -> Self {
        self.birthday = Some(birthday);
        self
    }

    /// Set notes
    #[must_use]
    pub fn with_notes(mut self, notes: impl Into<String>) -> Self {
        self.notes = Some(notes.into());
        self
    }
}

/// Contact update request (for partial updates)
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ContactUpdate {
    /// New display name
    pub name: Option<String>,
    /// New email address
    pub email: Option<String>,
    /// New phone number
    pub phone: Option<String>,
    /// New organization
    pub organization: Option<String>,
    /// New notes
    pub notes: Option<String>,
}

impl ContactUpdate {
    /// Create an empty update
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the new name
    #[must_use]
    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.name = Some(name.into());
        self
    }

    /// Set the new email
    #[must_use]
    pub fn with_email(mut self, email: impl Into<String>) -> Self {
        self.email = Some(email.into());
        self
    }

    /// Set the new phone
    #[must_use]
    pub fn with_phone(mut self, phone: impl Into<String>) -> Self {
        self.phone = Some(phone.into());
        self
    }

    /// Set the new organization
    #[must_use]
    pub fn with_organization(mut self, organization: impl Into<String>) -> Self {
        self.organization = Some(organization.into());
        self
    }

    /// Set the new notes
    #[must_use]
    pub fn with_notes(mut self, notes: impl Into<String>) -> Self {
        self.notes = Some(notes.into());
        self
    }

    /// Check if any field is set for update
    #[must_use]
    pub fn has_changes(&self) -> bool {
        self.name.is_some()
            || self.email.is_some()
            || self.phone.is_some()
            || self.organization.is_some()
            || self.notes.is_some()
    }
}

/// Addressbook information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AddressbookInfo {
    /// Addressbook identifier (href)
    pub id: String,
    /// Addressbook display name
    pub name: String,
    /// Whether this is the default addressbook
    pub is_default: bool,
}

/// Contact port trait
///
/// Defines operations for contact management via CardDAV.
/// Implemented by adapters that connect to CardDAV services (Baikal, etc).
#[cfg_attr(test, automock)]
#[async_trait]
pub trait ContactPort: Send + Sync {
    /// List available addressbooks
    async fn list_addressbooks(&self) -> Result<Vec<AddressbookInfo>, ContactError>;

    /// List all contacts (optionally filtered by query)
    async fn list_contacts(
        &self,
        query: Option<String>,
    ) -> Result<Vec<ContactSummary>, ContactError>;

    /// Get full details for a specific contact
    async fn get_contact(&self, contact_id: &str) -> Result<ContactDetail, ContactError>;

    /// Create a new contact
    ///
    /// # Returns
    /// The created contact's ID
    async fn create_contact(&self, contact: &NewContact) -> Result<String, ContactError>;

    /// Update an existing contact
    async fn update_contact(
        &self,
        contact_id: &str,
        update: &ContactUpdate,
    ) -> Result<(), ContactError>;

    /// Delete a contact
    async fn delete_contact(&self, contact_id: &str) -> Result<(), ContactError>;

    /// Search contacts by query string
    ///
    /// Searches across name, email, phone, and organization fields.
    async fn search_contacts(&self, query: &str) -> Result<Vec<ContactSummary>, ContactError>;

    /// Check if the contact service is available
    async fn is_available(&self) -> bool;

    /// Get contacts with upcoming birthdays within the next N days
    async fn get_upcoming_birthdays(&self, days: u32) -> Result<Vec<ContactSummary>, ContactError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn contact_summary_creation() {
        let summary = ContactSummary::new("c-1", "Alice Smith");
        assert_eq!(summary.id, "c-1");
        assert_eq!(summary.display_name, "Alice Smith");
        assert!(summary.email.is_none());
        assert!(summary.phone.is_none());
        assert!(summary.organization.is_none());
    }

    #[test]
    fn contact_summary_builder_pattern() {
        let summary = ContactSummary::new("c-1", "Alice Smith")
            .with_email("alice@example.com")
            .with_phone("+49 123 456")
            .with_organization("Acme Corp");

        assert_eq!(summary.email, Some("alice@example.com".to_string()));
        assert_eq!(summary.phone, Some("+49 123 456".to_string()));
        assert_eq!(summary.organization, Some("Acme Corp".to_string()));
    }

    #[test]
    fn contact_detail_creation() {
        let detail = ContactDetail::new("c-1", "Alice Smith");
        assert_eq!(detail.id, "c-1");
        assert_eq!(detail.display_name, "Alice Smith");
        assert!(detail.emails.is_empty());
        assert!(detail.phones.is_empty());
    }

    #[test]
    fn contact_detail_builder_pattern() {
        let detail = ContactDetail::new("c-1", "Alice Smith")
            .with_first_name("Alice")
            .with_last_name("Smith")
            .with_email("alice@work.com")
            .with_email("alice@home.com")
            .with_phone("+49 123")
            .with_organization("Acme")
            .with_title("Engineer")
            .with_birthday(NaiveDate::from_ymd_opt(1990, 5, 15).unwrap())
            .with_notes("VIP contact");

        assert_eq!(detail.first_name, Some("Alice".to_string()));
        assert_eq!(detail.last_name, Some("Smith".to_string()));
        assert_eq!(detail.emails.len(), 2);
        assert_eq!(detail.phones.len(), 1);
        assert_eq!(detail.organization, Some("Acme".to_string()));
        assert_eq!(detail.title, Some("Engineer".to_string()));
        assert_eq!(
            detail.birthday,
            Some(NaiveDate::from_ymd_opt(1990, 5, 15).unwrap())
        );
        assert_eq!(detail.notes, Some("VIP contact".to_string()));
    }

    #[test]
    fn new_contact_creation() {
        let contact = NewContact::new("Bob Jones");
        assert_eq!(contact.name, "Bob Jones");
        assert!(contact.email.is_none());
    }

    #[test]
    fn new_contact_builder_pattern() {
        let contact = NewContact::new("Bob Jones")
            .with_email("bob@test.com")
            .with_phone("+1 555 0100")
            .with_organization("TestCorp")
            .with_birthday(NaiveDate::from_ymd_opt(1985, 3, 20).unwrap())
            .with_notes("Friend");

        assert_eq!(contact.email, Some("bob@test.com".to_string()));
        assert_eq!(contact.phone, Some("+1 555 0100".to_string()));
        assert_eq!(contact.organization, Some("TestCorp".to_string()));
        assert_eq!(
            contact.birthday,
            Some(NaiveDate::from_ymd_opt(1985, 3, 20).unwrap())
        );
        assert_eq!(contact.notes, Some("Friend".to_string()));
    }

    #[test]
    fn contact_update_empty() {
        let update = ContactUpdate::new();
        assert!(!update.has_changes());
    }

    #[test]
    fn contact_update_with_changes() {
        let update = ContactUpdate::new()
            .with_name("New Name")
            .with_email("new@test.com");
        assert!(update.has_changes());
        assert_eq!(update.name, Some("New Name".to_string()));
        assert_eq!(update.email, Some("new@test.com".to_string()));
    }

    #[test]
    fn contact_update_all_fields() {
        let update = ContactUpdate::new()
            .with_name("Name")
            .with_email("e@t.com")
            .with_phone("+1")
            .with_organization("Org")
            .with_notes("Notes");
        assert!(update.has_changes());
        assert_eq!(update.organization, Some("Org".to_string()));
        assert_eq!(update.notes, Some("Notes".to_string()));
    }

    #[test]
    fn contact_error_display() {
        let err = ContactError::ServiceUnavailable;
        assert_eq!(err.to_string(), "Contact service unavailable");

        let err = ContactError::ContactNotFound("c-123".to_string());
        assert_eq!(err.to_string(), "Contact not found: c-123");

        let err = ContactError::AddressbookNotFound("default".to_string());
        assert_eq!(err.to_string(), "Addressbook not found: default");

        let err = ContactError::AuthenticationFailed;
        assert_eq!(err.to_string(), "Authentication failed");

        let err = ContactError::OperationFailed("timeout".to_string());
        assert_eq!(err.to_string(), "Operation failed: timeout");

        let err = ContactError::InvalidData("missing name".to_string());
        assert_eq!(err.to_string(), "Invalid data: missing name");
    }

    #[test]
    fn contact_summary_serialization() {
        let summary = ContactSummary::new("c-1", "Alice Smith").with_email("alice@test.com");
        let json = serde_json::to_string(&summary).unwrap();
        assert!(json.contains("\"id\":\"c-1\""));
        assert!(json.contains("\"display_name\":\"Alice Smith\""));
    }

    #[test]
    fn contact_summary_deserialization() {
        let json = r#"{
            "id": "c-1",
            "display_name": "Alice",
            "email": "alice@test.com",
            "phone": null,
            "organization": null
        }"#;
        let summary: ContactSummary = serde_json::from_str(json).unwrap();
        assert_eq!(summary.id, "c-1");
        assert_eq!(summary.email, Some("alice@test.com".to_string()));
    }

    #[test]
    fn contact_detail_serialization() {
        let detail = ContactDetail::new("c-1", "Alice Smith")
            .with_email("alice@test.com")
            .with_birthday(NaiveDate::from_ymd_opt(1990, 1, 1).unwrap());
        let json = serde_json::to_string(&detail).unwrap();
        let parsed: ContactDetail = serde_json::from_str(&json).unwrap();
        assert_eq!(detail, parsed);
    }

    #[test]
    fn new_contact_serialization() {
        let contact = NewContact::new("Test User").with_email("test@test.com");
        let json = serde_json::to_string(&contact).unwrap();
        let parsed: NewContact = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.name, "Test User");
        assert_eq!(parsed.email, Some("test@test.com".to_string()));
    }

    #[test]
    fn addressbook_info_serialization() {
        let info = AddressbookInfo {
            id: "ab-1".to_string(),
            name: "Personal".to_string(),
            is_default: true,
        };
        let json = serde_json::to_string(&info).unwrap();
        assert!(json.contains("\"name\":\"Personal\""));
        assert!(json.contains("\"is_default\":true"));
    }
}
