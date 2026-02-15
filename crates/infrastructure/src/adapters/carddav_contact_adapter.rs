//! CardDAV Contact adapter — Implements `ContactPort` using `integration_carddav`

use application::ports::{
    AddressbookInfo, ContactDetail, ContactError, ContactPort, ContactSummary, ContactUpdate,
    NewContact,
};
use async_trait::async_trait;
use chrono::Datelike;
use integration_carddav::{
    CardDavClient, CardDavConfig, CardDavError, Contact as CardDavContact, ContactEmail,
    ContactPhone, HttpCardDavClient,
};
use tracing::{debug, instrument, warn};

use super::{CircuitBreaker, CircuitBreakerConfig};

/// Adapter for CardDAV contact servers (e.g., Baikal).
///
/// Wraps an [`HttpCardDavClient`] and implements [`ContactPort`] from the
/// application layer, translating between integration-level and port-level
/// types. An optional [`CircuitBreaker`] protects against cascading failures.
pub struct CardDavContactAdapter {
    client: HttpCardDavClient,
    default_addressbook: Option<String>,
    circuit_breaker: Option<CircuitBreaker>,
}

impl std::fmt::Debug for CardDavContactAdapter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CardDavContactAdapter")
            .field("client", &self.client)
            .field("default_addressbook", &self.default_addressbook)
            .field(
                "circuit_breaker",
                &self
                    .circuit_breaker
                    .as_ref()
                    .map(super::circuit_breaker::CircuitBreaker::name),
            )
            .finish()
    }
}

impl CardDavContactAdapter {
    /// Create a new adapter from the given configuration.
    ///
    /// # Errors
    ///
    /// Returns [`ContactError`] when the underlying HTTP client cannot be
    /// constructed (e.g. invalid server URL).
    pub fn new(config: CardDavConfig) -> Result<Self, ContactError> {
        let default_addressbook = config.addressbook_path.clone();
        let client = HttpCardDavClient::new(config).map_err(Self::map_error)?;
        Ok(Self {
            client,
            default_addressbook,
            circuit_breaker: None,
        })
    }

    /// Enable circuit breaker with default configuration.
    #[must_use]
    pub fn with_circuit_breaker(mut self) -> Self {
        self.circuit_breaker = Some(CircuitBreaker::new("carddav-contacts"));
        self
    }

    /// Enable circuit breaker with custom configuration.
    #[must_use]
    pub fn with_circuit_breaker_config(mut self, config: CircuitBreakerConfig) -> Self {
        self.circuit_breaker = Some(CircuitBreaker::with_config("carddav-contacts", config));
        self
    }

    // -- circuit breaker helpers ------------------------------------------

    /// Check if circuit breaker is blocking requests.
    fn is_circuit_open(&self) -> bool {
        self.circuit_breaker
            .as_ref()
            .is_some_and(CircuitBreaker::is_open)
    }

    /// Get circuit breaker state description for logging.
    fn circuit_state_desc(&self) -> &'static str {
        match &self.circuit_breaker {
            Some(cb) if cb.is_open() => "open",
            Some(cb) if cb.is_closed() => "closed",
            Some(_) => "half-open",
            None => "disabled",
        }
    }

    /// Return an error early when the circuit is open.
    fn check_circuit(&self) -> Result<(), ContactError> {
        if self.is_circuit_open() {
            warn!("CardDAV contact circuit breaker is open, failing fast");
            return Err(ContactError::ServiceUnavailable);
        }
        Ok(())
    }

    // -- error / data mapping helpers -------------------------------------

    /// Map [`CardDavError`] to [`ContactError`].
    fn map_error(error: CardDavError) -> ContactError {
        match error {
            CardDavError::AuthenticationFailed => ContactError::AuthenticationFailed,
            CardDavError::ConnectionFailed(_) | CardDavError::Timeout => {
                ContactError::ServiceUnavailable
            },
            CardDavError::AddressBookNotFound(msg) => ContactError::AddressbookNotFound(msg),
            CardDavError::ContactNotFound(msg) => ContactError::ContactNotFound(msg),
            CardDavError::ParseError(msg) => ContactError::OperationFailed(msg),
            CardDavError::InvalidData(msg) => ContactError::InvalidData(msg),
            CardDavError::RequestFailed(msg) => {
                ContactError::OperationFailed(format!("Request failed: {msg}"))
            },
        }
    }

    /// Resolve the default addressbook path, falling back to auto-discovery.
    async fn get_default_addressbook(&self) -> Result<String, ContactError> {
        if let Some(ref ab) = self.default_addressbook {
            return Ok(ab.clone());
        }

        let books = self
            .client
            .list_addressbooks()
            .await
            .map_err(Self::map_error)?;

        books.first().cloned().ok_or_else(|| {
            ContactError::AddressbookNotFound("No addressbooks available".to_string())
        })
    }

    /// Convert a CardDAV [`Contact`](CardDavContact) to a [`ContactSummary`]
    /// (list view).
    fn to_summary(contact: &CardDavContact) -> ContactSummary {
        let display = if contact.full_name().is_empty() {
            "Unnamed".to_string()
        } else {
            contact.full_name()
        };

        let mut summary = ContactSummary::new(contact.id.clone(), display);

        if let Some(email) = contact.primary_email() {
            summary = summary.with_email(email);
        }
        if let Some(phone) = contact.primary_phone() {
            summary = summary.with_phone(phone);
        }
        if let Some(ref org) = contact.organization {
            summary = summary.with_organization(org);
        }

        summary
    }

    /// Convert a CardDAV [`Contact`](CardDavContact) to a [`ContactDetail`]
    /// (full view).
    fn to_detail(contact: &CardDavContact) -> ContactDetail {
        let display = if contact.full_name().is_empty() {
            "Unnamed".to_string()
        } else {
            contact.full_name()
        };

        let mut detail = ContactDetail::new(contact.id.clone(), display);

        if let Some(ref first) = contact.first_name {
            detail = detail.with_first_name(first);
        }
        if let Some(ref last) = contact.last_name {
            detail = detail.with_last_name(last);
        }
        for email in &contact.emails {
            detail = detail.with_email(&email.value);
        }
        for phone in &contact.phones {
            detail = detail.with_phone(&phone.value);
        }
        if let Some(ref org) = contact.organization {
            detail = detail.with_organization(org);
        }
        if let Some(ref title) = contact.title {
            detail = detail.with_title(title);
        }
        for addr in &contact.addresses {
            detail.addresses.push(addr.format_oneline());
        }
        if let Some(birthday) = contact.birthday {
            detail = detail.with_birthday(birthday);
        }
        if let Some(ref notes) = contact.notes {
            detail = detail.with_notes(notes);
        }
        if let Some(ref photo) = contact.photo_url {
            detail.photo_url = Some(photo.clone());
        }
        detail.categories.clone_from(&contact.categories);

        detail
    }

    /// Build a CardDAV [`Contact`](CardDavContact) from a [`NewContact`].
    fn from_new_contact(new: &NewContact) -> CardDavContact {
        let uid = uuid::Uuid::new_v4().to_string();
        let mut contact = CardDavContact::new(uid, new.name.clone());

        // Apply explicit first/last names when provided
        if let Some(ref first) = new.first_name {
            contact.first_name = Some(first.clone());
        }
        if let Some(ref last) = new.last_name {
            contact.last_name = Some(last.clone());
        }

        // If no explicit first/last names, try to split the display name
        if new.first_name.is_none() && new.last_name.is_none() {
            let parts: Vec<&str> = new.name.splitn(2, ' ').collect();
            if parts.len() == 2 {
                contact.first_name = Some(parts[0].to_string());
                contact.last_name = Some(parts[1].to_string());
            }
        }

        if let Some(ref email) = new.email {
            contact = contact.with_email(email, None);
        }
        if let Some(ref phone) = new.phone {
            contact = contact.with_phone(phone, None);
        }
        if let Some(ref org) = new.organization {
            contact.organization = Some(org.clone());
        }
        if let Some(birthday) = new.birthday {
            contact.birthday = Some(birthday);
        }
        if let Some(ref notes) = new.notes {
            contact.notes = Some(notes.clone());
        }

        contact
    }
}

#[async_trait]
impl ContactPort for CardDavContactAdapter {
    #[instrument(skip(self), fields(circuit = %self.circuit_state_desc()))]
    async fn list_addressbooks(&self) -> Result<Vec<AddressbookInfo>, ContactError> {
        self.check_circuit()?;
        debug!("Listing addressbooks from CardDAV");

        let paths = self
            .client
            .list_addressbooks()
            .await
            .map_err(Self::map_error)?;

        Ok(paths
            .into_iter()
            .enumerate()
            .map(|(i, path)| {
                let name = path
                    .rsplit('/')
                    .find(|s| !s.is_empty())
                    .unwrap_or("default")
                    .to_string();
                AddressbookInfo {
                    id: path,
                    name,
                    is_default: i == 0,
                }
            })
            .collect())
    }

    #[instrument(skip(self), fields(circuit = %self.circuit_state_desc()))]
    async fn list_contacts(
        &self,
        query: Option<String>,
    ) -> Result<Vec<ContactSummary>, ContactError> {
        self.check_circuit()?;
        debug!(query = ?query, "Listing contacts from CardDAV");

        let addressbook = self.get_default_addressbook().await?;

        let contacts = self
            .client
            .get_contacts(&addressbook)
            .await
            .map_err(Self::map_error)?;

        let summaries: Vec<ContactSummary> = query.as_deref().map_or_else(
            || contacts.iter().map(Self::to_summary).collect(),
            |q| {
                contacts
                    .iter()
                    .filter(|c| c.matches_query(q))
                    .map(Self::to_summary)
                    .collect()
            },
        );

        debug!(count = summaries.len(), "Listed contacts");
        Ok(summaries)
    }

    #[instrument(skip(self), fields(circuit = %self.circuit_state_desc()))]
    async fn get_contact(&self, contact_id: &str) -> Result<ContactDetail, ContactError> {
        self.check_circuit()?;
        debug!(contact_id, "Getting contact from CardDAV");

        let addressbook = self.get_default_addressbook().await?;

        let contact = self
            .client
            .get_contact(&addressbook, contact_id)
            .await
            .map_err(Self::map_error)?;

        Ok(Self::to_detail(&contact))
    }

    #[instrument(skip(self, contact), fields(circuit = %self.circuit_state_desc()))]
    async fn create_contact(&self, contact: &NewContact) -> Result<String, ContactError> {
        self.check_circuit()?;
        debug!(name = %contact.name, "Creating contact in CardDAV");

        let addressbook = self.get_default_addressbook().await?;
        let carddav_contact = Self::from_new_contact(contact);

        self.client
            .create_contact(&addressbook, &carddav_contact)
            .await
            .map_err(Self::map_error)
    }

    #[instrument(skip(self, update), fields(circuit = %self.circuit_state_desc()))]
    async fn update_contact(
        &self,
        contact_id: &str,
        update: &ContactUpdate,
    ) -> Result<(), ContactError> {
        self.check_circuit()?;
        debug!(contact_id, "Updating contact in CardDAV");

        let addressbook = self.get_default_addressbook().await?;

        // Fetch → modify → PUT (read-modify-write)
        let mut existing = self
            .client
            .get_contact(&addressbook, contact_id)
            .await
            .map_err(Self::map_error)?;

        // Apply updates
        if let Some(ref name) = update.name {
            existing.display_name = Some(name.clone());
            let parts: Vec<&str> = name.splitn(2, ' ').collect();
            if parts.len() == 2 {
                existing.first_name = Some(parts[0].to_string());
                existing.last_name = Some(parts[1].to_string());
            }
        }
        if let Some(ref email) = update.email {
            existing.emails = vec![ContactEmail {
                type_label: None,
                value: email.clone(),
            }];
        }
        if let Some(ref phone) = update.phone {
            existing.phones = vec![ContactPhone {
                type_label: None,
                value: phone.clone(),
            }];
        }
        if let Some(ref org) = update.organization {
            existing.organization = Some(org.clone());
        }
        if let Some(ref notes) = update.notes {
            existing.notes = Some(notes.clone());
        }

        self.client
            .update_contact(&addressbook, &existing)
            .await
            .map_err(Self::map_error)
    }

    #[instrument(skip(self), fields(circuit = %self.circuit_state_desc()))]
    async fn delete_contact(&self, contact_id: &str) -> Result<(), ContactError> {
        self.check_circuit()?;
        debug!(contact_id, "Deleting contact from CardDAV");

        let addressbook = self.get_default_addressbook().await?;

        self.client
            .delete_contact(&addressbook, contact_id)
            .await
            .map_err(Self::map_error)
    }

    #[instrument(skip(self), fields(circuit = %self.circuit_state_desc()))]
    async fn search_contacts(&self, query: &str) -> Result<Vec<ContactSummary>, ContactError> {
        self.check_circuit()?;
        debug!(query = %query, "Searching contacts in CardDAV");

        let addressbook = self.get_default_addressbook().await?;

        let contacts = self
            .client
            .search_contacts(&addressbook, query)
            .await
            .map_err(Self::map_error)?;

        let summaries: Vec<ContactSummary> =
            contacts.iter().map(Self::to_summary).collect();
        debug!(count = summaries.len(), "Search complete");
        Ok(summaries)
    }

    async fn is_available(&self) -> bool {
        if self.is_circuit_open() {
            debug!("CardDAV contacts unavailable: circuit breaker open");
            return false;
        }
        self.client.list_addressbooks().await.is_ok()
    }

    #[instrument(skip(self), fields(circuit = %self.circuit_state_desc()))]
    async fn get_upcoming_birthdays(
        &self,
        days: u32,
    ) -> Result<Vec<ContactSummary>, ContactError> {
        self.check_circuit()?;
        debug!(days, "Getting upcoming birthdays from CardDAV");

        let addressbook = self.get_default_addressbook().await?;

        let contacts = self
            .client
            .get_contacts(&addressbook)
            .await
            .map_err(Self::map_error)?;

        let today = chrono::Local::now().date_naive();
        let end_date = today + chrono::Duration::days(i64::from(days));

        let birthday_contacts: Vec<ContactSummary> = contacts
            .iter()
            .filter(|c| {
                c.birthday.is_some_and(|bday| {
                    // Check if the birthday (month+day) falls within the range
                    let this_year = bday.with_year(today.year()).unwrap_or(bday);
                    (this_year >= today && this_year <= end_date)
                        || {
                            // Also check next year for year-boundary cases
                            let next_year =
                                bday.with_year(today.year() + 1).unwrap_or(bday);
                            next_year >= today && next_year <= end_date
                        }
                })
            })
            .map(Self::to_summary)
            .collect();

        debug!(count = birthday_contacts.len(), "Found upcoming birthdays");
        Ok(birthday_contacts)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    #[test]
    fn to_summary_basic() {
        let contact =
            CardDavContact::new("uid-1", "Alice Smith").with_email("alice@test.com", None);
        let summary = CardDavContactAdapter::to_summary(&contact);
        assert_eq!(summary.display_name, "Alice Smith");
        assert_eq!(summary.email, Some("alice@test.com".to_string()));
    }

    #[test]
    fn to_summary_with_phone_and_org() {
        let mut contact =
            CardDavContact::new("uid-2", "Bob").with_phone("+49 123", None);
        contact.organization = Some("Acme".to_string());
        let summary = CardDavContactAdapter::to_summary(&contact);
        assert_eq!(summary.phone, Some("+49 123".to_string()));
        assert_eq!(summary.organization, Some("Acme".to_string()));
    }

    #[test]
    fn to_summary_unnamed_fallback() {
        let contact = CardDavContact {
            id: "uid-3".to_string(),
            display_name: None,
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
            created: None,
            last_modified: None,
        };
        let summary = CardDavContactAdapter::to_summary(&contact);
        assert_eq!(summary.display_name, "Unnamed");
    }

    #[test]
    fn to_detail_full() {
        let mut contact = CardDavContact::new("uid-4", "Alice Smith")
            .with_email("alice@work.com", Some("work".to_string()))
            .with_email("alice@home.com", Some("home".to_string()))
            .with_phone("+49 111", None);
        contact.first_name = Some("Alice".to_string());
        contact.last_name = Some("Smith".to_string());
        contact.organization = Some("Acme Corp".to_string());
        contact.title = Some("Engineer".to_string());
        contact.birthday = Some(NaiveDate::from_ymd_opt(1990, 5, 15).unwrap());
        contact.notes = Some("VIP contact".to_string());
        contact.categories = vec!["Work".to_string(), "VIP".to_string()];

        let detail = CardDavContactAdapter::to_detail(&contact);
        assert_eq!(detail.display_name, "Alice Smith");
        assert_eq!(detail.first_name, Some("Alice".to_string()));
        assert_eq!(detail.last_name, Some("Smith".to_string()));
        assert_eq!(detail.emails, vec!["alice@work.com", "alice@home.com"]);
        assert_eq!(detail.phones, vec!["+49 111"]);
        assert_eq!(detail.organization, Some("Acme Corp".to_string()));
        assert_eq!(detail.title, Some("Engineer".to_string()));
        assert_eq!(
            detail.birthday,
            Some(NaiveDate::from_ymd_opt(1990, 5, 15).unwrap())
        );
        assert_eq!(detail.notes, Some("VIP contact".to_string()));
        assert_eq!(detail.categories, vec!["Work", "VIP"]);
    }

    #[test]
    fn from_new_contact_basic() {
        let new = NewContact::new("Alice Smith").with_email("alice@test.com");
        let contact = CardDavContactAdapter::from_new_contact(&new);
        assert_eq!(contact.display_name, Some("Alice Smith".to_string()));
        assert_eq!(contact.first_name, Some("Alice".to_string()));
        assert_eq!(contact.last_name, Some("Smith".to_string()));
        assert_eq!(contact.primary_email(), Some("alice@test.com"));
    }

    #[test]
    fn from_new_contact_full() {
        let new = NewContact::new("Bob Jones")
            .with_email("bob@test.com")
            .with_phone("+1 555")
            .with_organization("TestCorp")
            .with_birthday(NaiveDate::from_ymd_opt(1985, 3, 20).unwrap())
            .with_notes("Friend");
        let contact = CardDavContactAdapter::from_new_contact(&new);
        assert_eq!(contact.organization, Some("TestCorp".to_string()));
        assert_eq!(
            contact.birthday,
            Some(NaiveDate::from_ymd_opt(1985, 3, 20).unwrap())
        );
        assert_eq!(contact.notes, Some("Friend".to_string()));
        assert_eq!(contact.primary_phone(), Some("+1 555"));
    }

    #[test]
    fn from_new_contact_single_name() {
        let new = NewContact::new("Madonna");
        let contact = CardDavContactAdapter::from_new_contact(&new);
        assert_eq!(contact.display_name, Some("Madonna".to_string()));
        // Single name should not split into first+last
        assert!(contact.first_name.is_none());
        assert!(contact.last_name.is_none());
    }

    #[test]
    fn from_new_contact_generates_uid() {
        let new = NewContact::new("Test");
        let c1 = CardDavContactAdapter::from_new_contact(&new);
        let c2 = CardDavContactAdapter::from_new_contact(&new);
        // Each call should produce a unique UID
        assert_ne!(c1.id, c2.id);
        assert!(!c1.id.is_empty());
    }

    #[test]
    fn map_error_auth() {
        let err = CardDavContactAdapter::map_error(CardDavError::AuthenticationFailed);
        assert!(matches!(err, ContactError::AuthenticationFailed));
    }

    #[test]
    fn map_error_connection() {
        let err = CardDavContactAdapter::map_error(CardDavError::ConnectionFailed(
            "timeout".to_string(),
        ));
        assert!(matches!(err, ContactError::ServiceUnavailable));
    }

    #[test]
    fn map_error_timeout() {
        let err = CardDavContactAdapter::map_error(CardDavError::Timeout);
        assert!(matches!(err, ContactError::ServiceUnavailable));
    }

    #[test]
    fn map_error_addressbook_not_found() {
        let err = CardDavContactAdapter::map_error(CardDavError::AddressBookNotFound(
            "default".to_string(),
        ));
        assert!(matches!(err, ContactError::AddressbookNotFound(_)));
    }

    #[test]
    fn map_error_contact_not_found() {
        let err = CardDavContactAdapter::map_error(CardDavError::ContactNotFound(
            "c-123".to_string(),
        ));
        assert!(matches!(err, ContactError::ContactNotFound(_)));
    }

    #[test]
    fn map_error_parse() {
        let err =
            CardDavContactAdapter::map_error(CardDavError::ParseError("bad vcard".to_string()));
        assert!(matches!(err, ContactError::OperationFailed(_)));
    }

    #[test]
    fn map_error_invalid_data() {
        let err =
            CardDavContactAdapter::map_error(CardDavError::InvalidData("bad field".to_string()));
        assert!(matches!(err, ContactError::InvalidData(_)));
    }

    #[test]
    fn map_error_request_failed() {
        let err =
            CardDavContactAdapter::map_error(CardDavError::RequestFailed("500".to_string()));
        let ContactError::OperationFailed(msg) = err else {
            unreachable!()
        };
        assert!(msg.contains("Request failed"));
    }

    #[test]
    fn adapter_debug_format() {
        let config = CardDavConfig {
            server_url: "https://dav.example.com".to_string(),
            username: "user".to_string(),
            password: "secret".to_string(),
            addressbook_path: Some("/a/default".to_string()),
            verify_certs: true,
            timeout_secs: 30,
        };
        let adapter = CardDavContactAdapter::new(config).unwrap();
        let dbg = format!("{adapter:?}");
        assert!(dbg.contains("CardDavContactAdapter"));
        assert!(!dbg.contains("secret"));
    }

    #[test]
    fn adapter_with_circuit_breaker() {
        let config = CardDavConfig {
            server_url: "https://dav.example.com".to_string(),
            username: "user".to_string(),
            password: "secret".to_string(),
            addressbook_path: None,
            verify_certs: true,
            timeout_secs: 30,
        };
        let adapter = CardDavContactAdapter::new(config)
            .unwrap()
            .with_circuit_breaker();
        assert!(!adapter.is_circuit_open());
        assert_eq!(adapter.circuit_state_desc(), "closed");
    }
}
