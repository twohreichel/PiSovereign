//! Contact data model for CardDAV integration
//!
//! Represents a vCard 3.0 contact with support for extended fields
//! including addresses, birthdays, notes, photo URLs, and categories.

use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};

/// A contact from a CardDAV address book
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Contact {
    /// Unique contact ID (vCard UID)
    pub id: String,
    /// First name (from N property)
    pub first_name: Option<String>,
    /// Last name (from N property)
    pub last_name: Option<String>,
    /// Display name (FN property)
    pub display_name: Option<String>,
    /// Email addresses
    pub emails: Vec<ContactEmail>,
    /// Phone numbers
    pub phones: Vec<ContactPhone>,
    /// Organization name
    pub organization: Option<String>,
    /// Job title
    pub title: Option<String>,
    /// Postal addresses
    pub addresses: Vec<ContactAddress>,
    /// Birthday
    pub birthday: Option<NaiveDate>,
    /// Notes
    pub notes: Option<String>,
    /// Photo URL
    pub photo_url: Option<String>,
    /// Categories/tags
    pub categories: Vec<String>,
    /// Creation timestamp
    pub created: Option<DateTime<Utc>>,
    /// Last modification timestamp
    pub last_modified: Option<DateTime<Utc>>,
}

/// An email address with an optional type label
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ContactEmail {
    /// Type label (e.g., "home", "work")
    pub type_label: Option<String>,
    /// Email address value
    pub value: String,
}

/// A phone number with an optional type label
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ContactPhone {
    /// Type label (e.g., "home", "work", "cell")
    pub type_label: Option<String>,
    /// Phone number value
    pub value: String,
}

/// A postal address with an optional type label
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ContactAddress {
    /// Type label (e.g., "home", "work")
    pub type_label: Option<String>,
    /// Street address
    pub street: Option<String>,
    /// City
    pub city: Option<String>,
    /// State/province
    pub state: Option<String>,
    /// Postal/ZIP code
    pub postal_code: Option<String>,
    /// Country
    pub country: Option<String>,
}

impl Contact {
    /// Create a new contact with the given ID and display name
    #[must_use]
    pub fn new(id: impl Into<String>, display_name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            first_name: None,
            last_name: None,
            display_name: Some(display_name.into()),
            emails: Vec::new(),
            phones: Vec::new(),
            organization: None,
            title: None,
            addresses: Vec::new(),
            birthday: None,
            notes: None,
            photo_url: None,
            categories: Vec::new(),
            created: None,
            last_modified: None,
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
    pub fn with_email(mut self, value: impl Into<String>, type_label: Option<String>) -> Self {
        self.emails.push(ContactEmail {
            type_label,
            value: value.into(),
        });
        self
    }

    /// Add a phone number
    #[must_use]
    pub fn with_phone(mut self, value: impl Into<String>, type_label: Option<String>) -> Self {
        self.phones.push(ContactPhone {
            type_label,
            value: value.into(),
        });
        self
    }

    /// Set the organization
    #[must_use]
    pub fn with_organization(mut self, org: impl Into<String>) -> Self {
        self.organization = Some(org.into());
        self
    }

    /// Set the job title
    #[must_use]
    pub fn with_title(mut self, title: impl Into<String>) -> Self {
        self.title = Some(title.into());
        self
    }

    /// Add a postal address
    #[must_use]
    pub fn with_address(mut self, address: ContactAddress) -> Self {
        self.addresses.push(address);
        self
    }

    /// Set the birthday
    #[must_use]
    pub fn with_birthday(mut self, birthday: NaiveDate) -> Self {
        self.birthday = Some(birthday);
        self
    }

    /// Set notes
    #[must_use]
    pub fn with_notes(mut self, notes: impl Into<String>) -> Self {
        self.notes = Some(notes.into());
        self
    }

    /// Set photo URL
    #[must_use]
    pub fn with_photo_url(mut self, url: impl Into<String>) -> Self {
        self.photo_url = Some(url.into());
        self
    }

    /// Add a category
    #[must_use]
    pub fn with_category(mut self, category: impl Into<String>) -> Self {
        self.categories.push(category.into());
        self
    }

    /// Get the primary email address (first one)
    #[must_use]
    pub fn primary_email(&self) -> Option<&str> {
        self.emails.first().map(|e| e.value.as_str())
    }

    /// Get the primary phone number (first one)
    #[must_use]
    pub fn primary_phone(&self) -> Option<&str> {
        self.phones.first().map(|p| p.value.as_str())
    }

    /// Get the full name, preferring display name over composed name
    #[must_use]
    pub fn full_name(&self) -> String {
        if let Some(dn) = &self.display_name {
            if !dn.is_empty() {
                return dn.clone();
            }
        }
        match (&self.first_name, &self.last_name) {
            (Some(first), Some(last)) => format!("{first} {last}"),
            (Some(first), None) => first.clone(),
            (None, Some(last)) => last.clone(),
            (None, None) => String::new(),
        }
    }

    /// Check whether this contact matches a search query (case-insensitive)
    #[must_use]
    pub fn matches_query(&self, query: &str) -> bool {
        let q = query.to_lowercase();
        let name_match = self.full_name().to_lowercase().contains(&q);
        let email_match = self
            .emails
            .iter()
            .any(|e| e.value.to_lowercase().contains(&q));
        let phone_match = self.phones.iter().any(|p| p.value.contains(&q));
        let org_match = self
            .organization
            .as_ref()
            .is_some_and(|o| o.to_lowercase().contains(&q));
        name_match || email_match || phone_match || org_match
    }
}

impl ContactAddress {
    /// Create a new empty address with a type label
    #[must_use]
    pub fn new(type_label: Option<String>) -> Self {
        Self {
            type_label,
            street: None,
            city: None,
            state: None,
            postal_code: None,
            country: None,
        }
    }

    /// Set the street
    #[must_use]
    pub fn with_street(mut self, street: impl Into<String>) -> Self {
        self.street = Some(street.into());
        self
    }

    /// Set the city
    #[must_use]
    pub fn with_city(mut self, city: impl Into<String>) -> Self {
        self.city = Some(city.into());
        self
    }

    /// Set the state
    #[must_use]
    pub fn with_state(mut self, state: impl Into<String>) -> Self {
        self.state = Some(state.into());
        self
    }

    /// Set the postal code
    #[must_use]
    pub fn with_postal_code(mut self, postal_code: impl Into<String>) -> Self {
        self.postal_code = Some(postal_code.into());
        self
    }

    /// Set the country
    #[must_use]
    pub fn with_country(mut self, country: impl Into<String>) -> Self {
        self.country = Some(country.into());
        self
    }

    /// Format the address as a single-line string
    #[must_use]
    pub fn format_oneline(&self) -> String {
        let parts: Vec<&str> = [
            self.street.as_deref(),
            self.city.as_deref(),
            self.state.as_deref(),
            self.postal_code.as_deref(),
            self.country.as_deref(),
        ]
        .into_iter()
        .flatten()
        .collect();
        parts.join(", ")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_contact_has_display_name() {
        let contact = Contact::new("uid-1", "Max Mustermann");
        assert_eq!(contact.id, "uid-1");
        assert_eq!(contact.display_name.as_deref(), Some("Max Mustermann"));
        assert!(contact.first_name.is_none());
        assert!(contact.emails.is_empty());
    }

    #[test]
    fn builder_pattern_works() {
        let contact = Contact::new("uid-1", "Max Mustermann")
            .with_first_name("Max")
            .with_last_name("Mustermann")
            .with_email("max@example.com", Some("work".to_string()))
            .with_phone("+49 123 456", Some("cell".to_string()))
            .with_organization("ACME Corp")
            .with_title("Engineer")
            .with_notes("Important contact")
            .with_category("friends");

        assert_eq!(contact.first_name.as_deref(), Some("Max"));
        assert_eq!(contact.last_name.as_deref(), Some("Mustermann"));
        assert_eq!(contact.emails.len(), 1);
        assert_eq!(contact.emails[0].value, "max@example.com");
        assert_eq!(contact.emails[0].type_label.as_deref(), Some("work"));
        assert_eq!(contact.phones.len(), 1);
        assert_eq!(contact.organization.as_deref(), Some("ACME Corp"));
        assert_eq!(contact.title.as_deref(), Some("Engineer"));
        assert_eq!(contact.notes.as_deref(), Some("Important contact"));
        assert_eq!(contact.categories, vec!["friends"]);
    }

    #[test]
    fn primary_email_returns_first() {
        let contact = Contact::new("uid-1", "Test")
            .with_email("first@example.com", None)
            .with_email("second@example.com", None);
        assert_eq!(contact.primary_email(), Some("first@example.com"));
    }

    #[test]
    fn primary_email_returns_none_when_empty() {
        let contact = Contact::new("uid-1", "Test");
        assert_eq!(contact.primary_email(), None);
    }

    #[test]
    fn primary_phone_returns_first() {
        let contact = Contact::new("uid-1", "Test")
            .with_phone("+49 123", None)
            .with_phone("+49 456", None);
        assert_eq!(contact.primary_phone(), Some("+49 123"));
    }

    #[test]
    fn full_name_prefers_display_name() {
        let contact = Contact::new("uid-1", "Display Name")
            .with_first_name("First")
            .with_last_name("Last");
        assert_eq!(contact.full_name(), "Display Name");
    }

    #[test]
    fn full_name_composes_from_parts() {
        let mut contact = Contact::new("uid-1", "");
        contact.display_name = None;
        contact.first_name = Some("Max".to_string());
        contact.last_name = Some("Mustermann".to_string());
        assert_eq!(contact.full_name(), "Max Mustermann");
    }

    #[test]
    fn full_name_first_only() {
        let mut contact = Contact::new("uid-1", "");
        contact.display_name = None;
        contact.first_name = Some("Max".to_string());
        assert_eq!(contact.full_name(), "Max");
    }

    #[test]
    fn full_name_last_only() {
        let mut contact = Contact::new("uid-1", "");
        contact.display_name = None;
        contact.last_name = Some("Mustermann".to_string());
        assert_eq!(contact.full_name(), "Mustermann");
    }

    #[test]
    fn full_name_empty() {
        let mut contact = Contact::new("uid-1", "");
        contact.display_name = None;
        assert_eq!(contact.full_name(), "");
    }

    #[test]
    fn matches_query_by_name() {
        let contact = Contact::new("uid-1", "Max Mustermann");
        assert!(contact.matches_query("Max"));
        assert!(contact.matches_query("max"));
        assert!(contact.matches_query("muster"));
        assert!(!contact.matches_query("Schmidt"));
    }

    #[test]
    fn matches_query_by_email() {
        let contact = Contact::new("uid-1", "Max").with_email("max@example.com", None);
        assert!(contact.matches_query("example.com"));
        assert!(contact.matches_query("max@"));
    }

    #[test]
    fn matches_query_by_phone() {
        let contact = Contact::new("uid-1", "Max").with_phone("+49123456", None);
        assert!(contact.matches_query("49123"));
    }

    #[test]
    fn matches_query_by_organization() {
        let contact = Contact::new("uid-1", "Max").with_organization("ACME Corp");
        assert!(contact.matches_query("acme"));
        assert!(contact.matches_query("ACME"));
    }

    #[test]
    fn matches_query_no_match() {
        let contact = Contact::new("uid-1", "Max Mustermann");
        assert!(!contact.matches_query("xyz-no-match"));
    }

    #[test]
    fn contact_address_builder() {
        let addr = ContactAddress::new(Some("home".to_string()))
            .with_street("123 Main St")
            .with_city("Springfield")
            .with_state("IL")
            .with_postal_code("62701")
            .with_country("USA");

        assert_eq!(addr.type_label.as_deref(), Some("home"));
        assert_eq!(addr.street.as_deref(), Some("123 Main St"));
        assert_eq!(addr.city.as_deref(), Some("Springfield"));
        assert_eq!(addr.state.as_deref(), Some("IL"));
        assert_eq!(addr.postal_code.as_deref(), Some("62701"));
        assert_eq!(addr.country.as_deref(), Some("USA"));
    }

    #[test]
    fn contact_address_format_oneline() {
        let addr = ContactAddress::new(None)
            .with_street("123 Main St")
            .with_city("Springfield")
            .with_country("USA");
        assert_eq!(addr.format_oneline(), "123 Main St, Springfield, USA");
    }

    #[test]
    fn contact_address_format_oneline_empty() {
        let addr = ContactAddress::new(None);
        assert_eq!(addr.format_oneline(), "");
    }

    #[test]
    fn contact_serialization_roundtrip() {
        let contact = Contact::new("uid-1", "Max Mustermann")
            .with_first_name("Max")
            .with_last_name("Mustermann")
            .with_email("max@example.com", Some("work".to_string()))
            .with_phone("+49 123 456", Some("cell".to_string()))
            .with_birthday(NaiveDate::from_ymd_opt(1990, 5, 15).expect("valid date"));

        let json = serde_json::to_string(&contact).expect("serialize");
        let parsed: Contact = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(contact, parsed);
    }

    #[test]
    fn contact_with_birthday() {
        let bday = NaiveDate::from_ymd_opt(1990, 5, 15).expect("valid date");
        let contact = Contact::new("uid-1", "Max").with_birthday(bday);
        assert_eq!(contact.birthday, Some(bday));
    }

    #[test]
    fn contact_with_photo_url() {
        let contact = Contact::new("uid-1", "Max").with_photo_url("https://example.com/photo.jpg");
        assert_eq!(
            contact.photo_url.as_deref(),
            Some("https://example.com/photo.jpg")
        );
    }

    #[test]
    fn contact_debug_format() {
        let contact = Contact::new("uid-1", "Max");
        let debug = format!("{contact:?}");
        assert!(debug.contains("uid-1"));
        assert!(debug.contains("Max"));
    }

    #[test]
    fn contact_clone() {
        let contact = Contact::new("uid-1", "Max").with_email("max@example.com", None);
        let cloned = contact.clone();
        assert_eq!(contact, cloned);
    }

    #[test]
    fn contact_email_equality() {
        let e1 = ContactEmail {
            type_label: Some("work".to_string()),
            value: "test@example.com".to_string(),
        };
        let e2 = e1.clone();
        assert_eq!(e1, e2);
    }

    #[test]
    fn contact_phone_equality() {
        let p1 = ContactPhone {
            type_label: Some("cell".to_string()),
            value: "+49123".to_string(),
        };
        let p2 = p1.clone();
        assert_eq!(p1, p2);
    }
}
