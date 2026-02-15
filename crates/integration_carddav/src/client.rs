//! CardDAV client
//!
//! Connects to CardDAV servers for contact management.
//! Supports standard CardDAV protocol with PROPFIND, REPORT, PUT, DELETE.
//! Uses vCard 3.0 (RFC 2426) for contact representation.

use async_trait::async_trait;
use chrono::{NaiveDate, Utc};
use quick_xml::{Reader, events::Event};
use reqwest::{Client, StatusCode};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tracing::{debug, instrument, warn};
use uuid::Uuid;

use crate::contact::{Contact, ContactAddress, ContactEmail, ContactPhone};

/// CardDAV client errors
#[derive(Debug, Error)]
pub enum CardDavError {
    #[error("Connection failed: {0}")]
    ConnectionFailed(String),

    #[error("Authentication failed")]
    AuthenticationFailed,

    #[error("Address book not found: {0}")]
    AddressBookNotFound(String),

    #[error("Contact not found: {0}")]
    ContactNotFound(String),

    #[error("Request failed: {0}")]
    RequestFailed(String),

    #[error("Parse error: {0}")]
    ParseError(String),

    #[error("Invalid data: {0}")]
    InvalidData(String),

    #[error("Request timed out")]
    Timeout,
}

/// CardDAV server configuration
#[derive(Clone, Serialize, Deserialize)]
pub struct CardDavConfig {
    /// Server URL (e.g., <https://dav.example.com/dav.php>)
    pub server_url: String,
    /// Username
    pub username: String,
    /// Password (excluded from serialization to prevent leaks)
    #[serde(skip_serializing, default)]
    pub password: String,
    /// Default address book path (auto-discovered if not set)
    pub addressbook_path: Option<String>,
    /// Verify TLS certificates (default: true)
    #[serde(default = "default_true")]
    pub verify_certs: bool,
    /// Connection timeout in seconds (default: 30)
    #[serde(default = "default_timeout")]
    pub timeout_secs: u64,
}

impl std::fmt::Debug for CardDavConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CardDavConfig")
            .field("server_url", &self.server_url)
            .field("username", &self.username)
            .field("password", &"[REDACTED]")
            .field("addressbook_path", &self.addressbook_path)
            .field("verify_certs", &self.verify_certs)
            .field("timeout_secs", &self.timeout_secs)
            .finish()
    }
}

const fn default_true() -> bool {
    true
}

const fn default_timeout() -> u64 {
    30
}

/// CardDAV client trait for contact operations
#[async_trait]
pub trait CardDavClient: Send + Sync {
    /// List available address books
    async fn list_addressbooks(&self) -> Result<Vec<String>, CardDavError>;

    /// Get all contacts from an address book
    async fn get_contacts(&self, addressbook: &str) -> Result<Vec<Contact>, CardDavError>;

    /// Get a single contact by ID
    async fn get_contact(
        &self,
        addressbook: &str,
        contact_id: &str,
    ) -> Result<Contact, CardDavError>;

    /// Create a new contact
    async fn create_contact(
        &self,
        addressbook: &str,
        contact: &Contact,
    ) -> Result<String, CardDavError>;

    /// Update an existing contact
    async fn update_contact(
        &self,
        addressbook: &str,
        contact: &Contact,
    ) -> Result<(), CardDavError>;

    /// Delete a contact
    async fn delete_contact(&self, addressbook: &str, contact_id: &str)
    -> Result<(), CardDavError>;

    /// Search contacts by query (client-side filtering)
    async fn search_contacts(
        &self,
        addressbook: &str,
        query: &str,
    ) -> Result<Vec<Contact>, CardDavError>;
}

/// HTTP-based CardDAV client implementation
#[derive(Debug)]
pub struct HttpCardDavClient {
    client: Client,
    config: CardDavConfig,
}

impl HttpCardDavClient {
    /// Create a new CardDAV client
    pub fn new(config: CardDavConfig) -> Result<Self, CardDavError> {
        let client = Client::builder()
            .timeout(std::time::Duration::from_secs(config.timeout_secs))
            .danger_accept_invalid_certs(!config.verify_certs)
            .build()
            .map_err(|e| CardDavError::ConnectionFailed(e.to_string()))?;

        Ok(Self { client, config })
    }

    /// Build a request with proper authentication
    fn build_request(&self, method: &str, url: &str) -> reqwest::RequestBuilder {
        let request = match method {
            "PROPFIND" => self.client.request(
                reqwest::Method::from_bytes(b"PROPFIND").unwrap_or(reqwest::Method::GET),
                url,
            ),
            "REPORT" => self.client.request(
                reqwest::Method::from_bytes(b"REPORT").unwrap_or(reqwest::Method::GET),
                url,
            ),
            _ => self.client.request(
                reqwest::Method::from_bytes(method.as_bytes()).unwrap_or(reqwest::Method::GET),
                url,
            ),
        };

        request
            .basic_auth(&self.config.username, Some(&self.config.password))
            .header("Content-Type", "application/xml; charset=utf-8")
    }

    /// Build the address book URL
    fn addressbook_url(&self, addressbook: &str) -> String {
        if addressbook.starts_with("http") {
            addressbook.to_string()
        } else {
            format!(
                "{}/{}",
                self.config.server_url.trim_end_matches('/'),
                addressbook.trim_start_matches('/')
            )
        }
    }

    /// Build a URL for a specific contact within an address book
    fn contact_url(&self, addressbook: &str, contact_id: &str) -> String {
        let base = self.addressbook_url(addressbook);
        let clean_id = contact_id.trim_end_matches(".vcf");
        format!("{}/{clean_id}.vcf", base.trim_end_matches('/'))
    }

    /// Extract vCard data from CardDAV XML response
    fn extract_vcard_data_from_xml(xml_body: &str) -> Vec<String> {
        let mut reader = Reader::from_str(xml_body);
        reader.config_mut().trim_text(true);

        let mut vcard_list = Vec::new();
        let mut buf = Vec::new();
        let mut inside_address_data = false;
        let mut current_vcard = String::new();

        loop {
            match reader.read_event_into(&mut buf) {
                Ok(Event::Start(e)) => {
                    let name = e.name();
                    let name_ref = name.as_ref();
                    if name_ref == b"card:address-data"
                        || name_ref == b"C:address-data"
                        || name_ref == b"address-data"
                        || name_ref == b"D:address-data"
                    {
                        inside_address_data = true;
                        current_vcard.clear();
                    }
                },
                Ok(Event::Text(e)) => {
                    if inside_address_data {
                        if let Ok(text) = e.unescape() {
                            current_vcard.push_str(&text);
                        }
                    }
                },
                Ok(Event::CData(e)) => {
                    if inside_address_data {
                        if let Ok(text) = std::str::from_utf8(e.as_ref()) {
                            current_vcard.push_str(text);
                        }
                    }
                },
                Ok(Event::End(e)) => {
                    if inside_address_data {
                        let name = e.name();
                        let name_ref = name.as_ref();
                        if name_ref == b"card:address-data"
                            || name_ref == b"C:address-data"
                            || name_ref == b"address-data"
                            || name_ref == b"D:address-data"
                        {
                            inside_address_data = false;
                            if !current_vcard.trim().is_empty() {
                                vcard_list.push(current_vcard.clone());
                            }
                        }
                    }
                },
                Ok(Event::Eof) => break,
                Err(e) => {
                    debug!(error = ?e, "XML parsing error in CardDAV response");
                    break;
                },
                _ => {},
            }
            buf.clear();
        }

        vcard_list
    }

    /// Extract href paths from a multistatus response for addressbook discovery
    fn extract_addressbook_hrefs(xml_body: &str) -> Vec<String> {
        let mut reader = Reader::from_str(xml_body);
        reader.config_mut().trim_text(true);

        let mut hrefs = Vec::new();
        let mut buf = Vec::new();
        let mut inside_href = false;
        let mut inside_resourcetype = false;
        let mut is_addressbook = false;
        let mut current_href = String::new();
        // Track per-response context
        let mut response_hrefs: Vec<String> = Vec::new();

        loop {
            match reader.read_event_into(&mut buf) {
                Ok(Event::Start(e)) => {
                    let name = e.name();
                    let name_ref = name.as_ref();
                    if name_ref == b"D:response"
                        || name_ref == b"d:response"
                        || name_ref == b"response"
                    {
                        response_hrefs.clear();
                        is_addressbook = false;
                    } else if name_ref == b"D:href" || name_ref == b"d:href" || name_ref == b"href"
                    {
                        inside_href = true;
                        current_href.clear();
                    } else if name_ref == b"D:resourcetype"
                        || name_ref == b"d:resourcetype"
                        || name_ref == b"resourcetype"
                    {
                        inside_resourcetype = true;
                    }
                },
                Ok(Event::Empty(e)) => {
                    if inside_resourcetype {
                        let name = e.name();
                        let name_ref = name.as_ref();
                        if name_ref == b"card:addressbook"
                            || name_ref == b"C:addressbook"
                            || name_ref == b"CR:addressbook"
                            || name_ref == b"addressbook"
                        {
                            is_addressbook = true;
                        }
                    }
                },
                Ok(Event::Text(e)) => {
                    if inside_href {
                        if let Ok(text) = e.unescape() {
                            current_href.push_str(&text);
                        }
                    }
                },
                Ok(Event::End(e)) => {
                    let name = e.name();
                    let name_ref = name.as_ref();
                    if name_ref == b"D:href" || name_ref == b"d:href" || name_ref == b"href" {
                        inside_href = false;
                        if !current_href.is_empty() {
                            response_hrefs.push(current_href.clone());
                        }
                    } else if name_ref == b"D:resourcetype"
                        || name_ref == b"d:resourcetype"
                        || name_ref == b"resourcetype"
                    {
                        inside_resourcetype = false;
                    } else if is_addressbook
                        && (name_ref == b"D:response"
                            || name_ref == b"d:response"
                            || name_ref == b"response")
                    {
                        hrefs.append(&mut response_hrefs);
                    } else if name_ref == b"D:response"
                        || name_ref == b"d:response"
                        || name_ref == b"response"
                    {
                        response_hrefs.clear();
                    }
                },
                Ok(Event::Eof) => break,
                Err(e) => {
                    debug!(error = ?e, "XML parsing error during addressbook discovery");
                    break;
                },
                _ => {},
            }
            buf.clear();
        }

        hrefs
    }
}

/// Parse a vCard 3.0 string into a Contact
pub fn parse_vcard(vcard_data: &str) -> Result<Contact, CardDavError> {
    let mut id = String::new();
    let mut first_name = None;
    let mut last_name = None;
    let mut display_name = None;
    let mut emails: Vec<ContactEmail> = Vec::new();
    let mut phones: Vec<ContactPhone> = Vec::new();
    let mut organization = None;
    let mut title = None;
    let mut addresses: Vec<ContactAddress> = Vec::new();
    let mut birthday = None;
    let mut notes = None;
    let mut photo_url = None;
    let mut categories: Vec<String> = Vec::new();
    let mut last_modified = None;

    for line in unfold_vcard_lines(vcard_data) {
        let line = line.trim();
        if line.is_empty()
            || line == "BEGIN:VCARD"
            || line == "END:VCARD"
            || line.starts_with("VERSION:")
        {
            continue;
        }

        // Split property name and value
        let Some((prop_with_params, value)) = line.split_once(':') else {
            continue;
        };

        // Extract property name and parameters
        let (prop_name, params) = match prop_with_params.split_once(';') {
            Some((name, params)) => (name.to_uppercase(), Some(params)),
            None => (prop_with_params.to_uppercase(), None),
        };

        match prop_name.as_str() {
            "UID" => id = value.to_string(),
            "FN" => display_name = Some(value.to_string()),
            "N" => {
                // N:Last;First;Middle;Prefix;Suffix
                let parts: Vec<&str> = value.split(';').collect();
                if let Some(ln) = parts.first() {
                    if !ln.is_empty() {
                        last_name = Some((*ln).to_string());
                    }
                }
                if let Some(fn_part) = parts.get(1) {
                    if !fn_part.is_empty() {
                        first_name = Some((*fn_part).to_string());
                    }
                }
            },
            "EMAIL" => {
                let type_label = extract_type_param(params);
                emails.push(ContactEmail {
                    type_label,
                    value: value.to_string(),
                });
            },
            "TEL" => {
                let type_label = extract_type_param(params);
                phones.push(ContactPhone {
                    type_label,
                    value: value.to_string(),
                });
            },
            "ORG" => {
                organization = Some(value.replace(';', ", "));
            },
            "TITLE" => {
                title = Some(value.to_string());
            },
            "ADR" => {
                // ADR:PO Box;Ext Addr;Street;City;State;Postal;Country
                let type_label = extract_type_param(params);
                let parts: Vec<&str> = value.split(';').collect();
                let street = parts
                    .get(2)
                    .filter(|s| !s.is_empty())
                    .map(|s| (*s).to_string());
                let city = parts
                    .get(3)
                    .filter(|s| !s.is_empty())
                    .map(|s| (*s).to_string());
                let state = parts
                    .get(4)
                    .filter(|s| !s.is_empty())
                    .map(|s| (*s).to_string());
                let postal_code = parts
                    .get(5)
                    .filter(|s| !s.is_empty())
                    .map(|s| (*s).to_string());
                let country = parts
                    .get(6)
                    .filter(|s| !s.is_empty())
                    .map(|s| (*s).to_string());

                addresses.push(ContactAddress {
                    type_label,
                    street,
                    city,
                    state,
                    postal_code,
                    country,
                });
            },
            "BDAY" => {
                birthday = parse_vcard_date(value);
            },
            "NOTE" => {
                notes = Some(value.to_string());
            },
            "PHOTO" => {
                // Only handle URL-based photos
                let is_uri_param = params.is_some_and(|p| {
                    let upper = p.to_uppercase();
                    upper.contains("VALUE=URI") || upper.contains("VALUE=URL")
                });
                if is_uri_param || value.starts_with("http") {
                    photo_url = Some(value.to_string());
                }
            },
            "CATEGORIES" => {
                categories.extend(value.split(',').map(|s| s.trim().to_string()));
            },
            "REV" => {
                last_modified = parse_vcard_datetime(value);
            },
            _ => {},
        }
    }

    if id.is_empty() {
        id = Uuid::new_v4().to_string();
    }

    Ok(Contact {
        id,
        first_name,
        last_name,
        display_name,
        emails,
        phones,
        organization,
        title,
        addresses,
        birthday,
        notes,
        photo_url,
        categories,
        created: None,
        last_modified,
    })
}

/// Build a vCard 3.0 string from a Contact
pub fn build_vcard(contact: &Contact) -> String {
    let mut vcard = String::with_capacity(512);
    vcard.push_str("BEGIN:VCARD\r\n");
    vcard.push_str("VERSION:3.0\r\n");
    vcard.push_str("PRODID:-//PiSovereign//CardDAV Client//EN\r\n");

    // UID
    vcard.push_str(&format!("UID:{}\r\n", contact.id));

    // N (structured name)
    let last = contact.last_name.as_deref().unwrap_or("");
    let first = contact.first_name.as_deref().unwrap_or("");
    vcard.push_str(&format!("N:{last};{first};;;\r\n"));

    // FN (formatted/display name)
    let fn_value = contact.full_name();
    if fn_value.is_empty() {
        vcard.push_str("FN:Unknown\r\n");
    } else {
        vcard.push_str(&format!("FN:{fn_value}\r\n"));
    }

    // Emails
    for email in &contact.emails {
        if let Some(ref t) = email.type_label {
            vcard.push_str(&format!("EMAIL;TYPE={t}:{}\r\n", email.value));
        } else {
            vcard.push_str(&format!("EMAIL:{}\r\n", email.value));
        }
    }

    // Phones
    for phone in &contact.phones {
        if let Some(ref t) = phone.type_label {
            vcard.push_str(&format!("TEL;TYPE={t}:{}\r\n", phone.value));
        } else {
            vcard.push_str(&format!("TEL:{}\r\n", phone.value));
        }
    }

    // Organization
    if let Some(ref org) = contact.organization {
        vcard.push_str(&format!("ORG:{org}\r\n"));
    }

    // Title
    if let Some(ref title) = contact.title {
        vcard.push_str(&format!("TITLE:{title}\r\n"));
    }

    // Addresses
    for addr in &contact.addresses {
        let type_part = addr
            .type_label
            .as_ref()
            .map_or(String::new(), |t| format!(";TYPE={t}"));
        let street = addr.street.as_deref().unwrap_or("");
        let city = addr.city.as_deref().unwrap_or("");
        let state = addr.state.as_deref().unwrap_or("");
        let postal = addr.postal_code.as_deref().unwrap_or("");
        let country = addr.country.as_deref().unwrap_or("");
        vcard.push_str(&format!(
            "ADR{type_part}:;;{street};{city};{state};{postal};{country}\r\n"
        ));
    }

    // Birthday
    if let Some(bday) = contact.birthday {
        vcard.push_str(&format!("BDAY:{}\r\n", bday.format("%Y-%m-%d")));
    }

    // Notes
    if let Some(ref notes) = contact.notes {
        vcard.push_str(&format!("NOTE:{notes}\r\n"));
    }

    // Photo URL
    if let Some(ref url) = contact.photo_url {
        vcard.push_str(&format!("PHOTO;VALUE=URI:{url}\r\n"));
    }

    // Categories
    if !contact.categories.is_empty() {
        vcard.push_str(&format!("CATEGORIES:{}\r\n", contact.categories.join(",")));
    }

    // Revision (last modified)
    let rev = contact
        .last_modified
        .unwrap_or_else(Utc::now)
        .format("%Y%m%dT%H%M%SZ");
    vcard.push_str(&format!("REV:{rev}\r\n"));

    vcard.push_str("END:VCARD\r\n");
    vcard
}

/// Unfold continuation lines in vCard data (RFC 2425 line folding)
fn unfold_vcard_lines(data: &str) -> Vec<String> {
    let mut lines = Vec::new();
    let mut current = String::new();

    for line in data.lines() {
        if line.starts_with(' ') || line.starts_with('\t') {
            // Continuation line — append content after the single folding whitespace
            current.push_str(&line[1..]);
        } else {
            if !current.is_empty() {
                lines.push(current.clone());
            }
            current = line.to_string();
        }
    }
    if !current.is_empty() {
        lines.push(current);
    }

    lines
}

/// Extract TYPE parameter from vCard property parameters
fn extract_type_param(params: Option<&str>) -> Option<String> {
    let params = params?;
    for param in params.split(';') {
        let param_upper = param.to_uppercase();
        if let Some(value) = param_upper.strip_prefix("TYPE=") {
            return Some(value.to_string().to_lowercase());
        }
        // Some vCards use bare type values (e.g., ";WORK" instead of ";TYPE=WORK")
        let bare = param.trim().to_uppercase();
        if matches!(
            bare.as_str(),
            "HOME" | "WORK" | "CELL" | "FAX" | "PAGER" | "VOICE" | "PREF" | "INTERNET"
        ) {
            return Some(bare.to_lowercase());
        }
    }
    None
}

/// Parse a vCard date string (BDAY)
fn parse_vcard_date(value: &str) -> Option<NaiveDate> {
    // Try formats: YYYY-MM-DD, YYYYMMDD
    NaiveDate::parse_from_str(value, "%Y-%m-%d")
        .or_else(|_| NaiveDate::parse_from_str(value, "%Y%m%d"))
        .ok()
}

/// Parse a vCard datetime string (REV)
fn parse_vcard_datetime(value: &str) -> Option<chrono::DateTime<Utc>> {
    // Try formats: YYYYMMDDTHHMMSSZ, YYYY-MM-DDTHH:MM:SSZ
    // chrono's parse_from_str requires matching input/format exactly
    chrono::NaiveDateTime::parse_from_str(value, "%Y%m%dT%H%M%SZ")
        .or_else(|_| chrono::NaiveDateTime::parse_from_str(value, "%Y-%m-%dT%H:%M:%SZ"))
        .ok()
        .map(|dt| dt.and_utc())
}

#[async_trait]
impl CardDavClient for HttpCardDavClient {
    #[instrument(skip(self), fields(server = %self.config.server_url))]
    async fn list_addressbooks(&self) -> Result<Vec<String>, CardDavError> {
        let url = format!(
            "{}/addressbooks/{}/",
            self.config.server_url.trim_end_matches('/'),
            self.config.username
        );

        let propfind_body = r#"<?xml version="1.0" encoding="utf-8"?>
<D:propfind xmlns:D="DAV:" xmlns:card="urn:ietf:params:xml:ns:carddav">
  <D:prop>
    <D:resourcetype/>
    <D:displayname/>
  </D:prop>
</D:propfind>"#;

        let response = self
            .build_request("PROPFIND", &url)
            .header("Depth", "1")
            .body(propfind_body.to_string())
            .send()
            .await
            .map_err(|e| {
                if e.is_timeout() {
                    return CardDavError::Timeout;
                }
                CardDavError::ConnectionFailed(e.to_string())
            })?;

        match response.status() {
            StatusCode::UNAUTHORIZED => return Err(CardDavError::AuthenticationFailed),
            status if status.is_server_error() => {
                return Err(CardDavError::RequestFailed(format!(
                    "Server error: {status}"
                )));
            },
            _ => {},
        }

        let body = response
            .text()
            .await
            .map_err(|e| CardDavError::RequestFailed(e.to_string()))?;

        debug!(
            body_length = body.len(),
            "Received addressbook list response"
        );

        let hrefs = Self::extract_addressbook_hrefs(&body);

        if hrefs.is_empty() {
            warn!("No address books found via PROPFIND");
        }

        Ok(hrefs)
    }

    #[instrument(skip(self), fields(addressbook = %addressbook))]
    async fn get_contacts(&self, addressbook: &str) -> Result<Vec<Contact>, CardDavError> {
        let url = self.addressbook_url(addressbook);

        let report_body = r#"<?xml version="1.0" encoding="utf-8"?>
<card:addressbook-query xmlns:D="DAV:" xmlns:card="urn:ietf:params:xml:ns:carddav">
  <D:prop>
    <card:address-data/>
  </D:prop>
</card:addressbook-query>"#;

        let response = self
            .build_request("REPORT", &url)
            .header("Depth", "1")
            .body(report_body.to_string())
            .send()
            .await
            .map_err(|e| {
                if e.is_timeout() {
                    return CardDavError::Timeout;
                }
                CardDavError::ConnectionFailed(e.to_string())
            })?;

        match response.status() {
            StatusCode::UNAUTHORIZED => return Err(CardDavError::AuthenticationFailed),
            StatusCode::NOT_FOUND => {
                return Err(CardDavError::AddressBookNotFound(addressbook.to_string()));
            },
            status if status.is_server_error() => {
                return Err(CardDavError::RequestFailed(format!(
                    "Server error: {status}"
                )));
            },
            _ => {},
        }

        let body = response
            .text()
            .await
            .map_err(|e| CardDavError::RequestFailed(e.to_string()))?;

        debug!(body_length = body.len(), "Received contacts response");

        let vcard_strings = Self::extract_vcard_data_from_xml(&body);
        let mut contacts = Vec::new();

        for vcard_str in &vcard_strings {
            match parse_vcard(vcard_str) {
                Ok(contact) => contacts.push(contact),
                Err(e) => {
                    warn!(error = %e, "Failed to parse vCard, skipping");
                },
            }
        }

        Ok(contacts)
    }

    #[instrument(skip(self), fields(addressbook = %addressbook, contact_id = %contact_id))]
    async fn get_contact(
        &self,
        addressbook: &str,
        contact_id: &str,
    ) -> Result<Contact, CardDavError> {
        let url = self.contact_url(addressbook, contact_id);

        let response = self
            .build_request("GET", &url)
            .header("Accept", "text/vcard")
            .send()
            .await
            .map_err(|e| {
                if e.is_timeout() {
                    return CardDavError::Timeout;
                }
                CardDavError::ConnectionFailed(e.to_string())
            })?;

        match response.status() {
            StatusCode::UNAUTHORIZED => return Err(CardDavError::AuthenticationFailed),
            StatusCode::NOT_FOUND => {
                return Err(CardDavError::ContactNotFound(contact_id.to_string()));
            },
            status if status.is_server_error() => {
                return Err(CardDavError::RequestFailed(format!(
                    "Server error: {status}"
                )));
            },
            _ => {},
        }

        let body = response
            .text()
            .await
            .map_err(|e| CardDavError::RequestFailed(e.to_string()))?;

        parse_vcard(&body)
    }

    #[instrument(skip(self, contact), fields(addressbook = %addressbook, contact_id = %contact.id))]
    async fn create_contact(
        &self,
        addressbook: &str,
        contact: &Contact,
    ) -> Result<String, CardDavError> {
        let url = self.contact_url(addressbook, &contact.id);
        let vcard_body = build_vcard(contact);

        let response = self
            .client
            .put(&url)
            .basic_auth(&self.config.username, Some(&self.config.password))
            .header("Content-Type", "text/vcard; charset=utf-8")
            .header("If-None-Match", "*")
            .body(vcard_body)
            .send()
            .await
            .map_err(|e| {
                if e.is_timeout() {
                    return CardDavError::Timeout;
                }
                CardDavError::ConnectionFailed(e.to_string())
            })?;

        match response.status() {
            StatusCode::CREATED | StatusCode::NO_CONTENT => Ok(contact.id.clone()),
            StatusCode::UNAUTHORIZED => Err(CardDavError::AuthenticationFailed),
            StatusCode::NOT_FOUND => {
                Err(CardDavError::AddressBookNotFound(addressbook.to_string()))
            },
            status => Err(CardDavError::RequestFailed(format!(
                "Create contact failed: {status}"
            ))),
        }
    }

    #[instrument(skip(self, contact), fields(addressbook = %addressbook, contact_id = %contact.id))]
    async fn update_contact(
        &self,
        addressbook: &str,
        contact: &Contact,
    ) -> Result<(), CardDavError> {
        let url = self.contact_url(addressbook, &contact.id);
        let vcard_body = build_vcard(contact);

        let response = self
            .client
            .put(&url)
            .basic_auth(&self.config.username, Some(&self.config.password))
            .header("Content-Type", "text/vcard; charset=utf-8")
            .body(vcard_body)
            .send()
            .await
            .map_err(|e| {
                if e.is_timeout() {
                    return CardDavError::Timeout;
                }
                CardDavError::ConnectionFailed(e.to_string())
            })?;

        match response.status() {
            StatusCode::CREATED | StatusCode::NO_CONTENT | StatusCode::OK => Ok(()),
            StatusCode::UNAUTHORIZED => Err(CardDavError::AuthenticationFailed),
            StatusCode::NOT_FOUND => Err(CardDavError::ContactNotFound(contact.id.clone())),
            status => Err(CardDavError::RequestFailed(format!(
                "Update contact failed: {status}"
            ))),
        }
    }

    #[instrument(skip(self), fields(addressbook = %addressbook, contact_id = %contact_id))]
    async fn delete_contact(
        &self,
        addressbook: &str,
        contact_id: &str,
    ) -> Result<(), CardDavError> {
        let url = self.contact_url(addressbook, contact_id);

        let response = self
            .client
            .delete(&url)
            .basic_auth(&self.config.username, Some(&self.config.password))
            .send()
            .await
            .map_err(|e| {
                if e.is_timeout() {
                    return CardDavError::Timeout;
                }
                CardDavError::ConnectionFailed(e.to_string())
            })?;

        match response.status() {
            StatusCode::NO_CONTENT | StatusCode::OK => Ok(()),
            StatusCode::UNAUTHORIZED => Err(CardDavError::AuthenticationFailed),
            StatusCode::NOT_FOUND => Err(CardDavError::ContactNotFound(contact_id.to_string())),
            status => Err(CardDavError::RequestFailed(format!(
                "Delete contact failed: {status}"
            ))),
        }
    }

    #[instrument(skip(self), fields(addressbook = %addressbook, query = %query))]
    async fn search_contacts(
        &self,
        addressbook: &str,
        query: &str,
    ) -> Result<Vec<Contact>, CardDavError> {
        // Client-side filtering since Baïkal doesn't reliably support server-side search
        let all_contacts = self.get_contacts(addressbook).await?;
        let results = all_contacts
            .into_iter()
            .filter(|c| c.matches_query(query))
            .collect();
        Ok(results)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // === Config Tests ===

    #[test]
    fn config_debug_redacts_password() {
        let config = CardDavConfig {
            server_url: "https://dav.example.com".to_string(),
            username: "user".to_string(),
            password: "secret123".to_string(),
            addressbook_path: None,
            verify_certs: true,
            timeout_secs: 30,
        };
        let debug = format!("{config:?}");
        assert!(debug.contains("[REDACTED]"));
        assert!(!debug.contains("secret123"));
    }

    #[test]
    fn config_serialization_excludes_password() {
        let config = CardDavConfig {
            server_url: "https://dav.example.com".to_string(),
            username: "user".to_string(),
            password: "secret".to_string(),
            addressbook_path: None,
            verify_certs: true,
            timeout_secs: 30,
        };
        let json = serde_json::to_string(&config).expect("serialize");
        assert!(!json.contains("secret"));
        assert!(json.contains("dav.example.com"));
    }

    #[test]
    fn config_deserialization_defaults() {
        let json = r#"{"server_url":"https://dav.example.com","username":"user"}"#;
        let config: CardDavConfig = serde_json::from_str(json).expect("deserialize");
        assert!(config.verify_certs);
        assert_eq!(config.timeout_secs, 30);
        assert!(config.password.is_empty());
    }

    // === Error Tests ===

    #[test]
    fn error_display() {
        let err = CardDavError::ConnectionFailed("timeout".to_string());
        assert_eq!(err.to_string(), "Connection failed: timeout");

        let err = CardDavError::AuthenticationFailed;
        assert_eq!(err.to_string(), "Authentication failed");

        let err = CardDavError::ContactNotFound("uid-1".to_string());
        assert_eq!(err.to_string(), "Contact not found: uid-1");

        let err = CardDavError::AddressBookNotFound("default".to_string());
        assert_eq!(err.to_string(), "Address book not found: default");

        let err = CardDavError::Timeout;
        assert_eq!(err.to_string(), "Request timed out");

        let err = CardDavError::InvalidData("bad input".to_string());
        assert_eq!(err.to_string(), "Invalid data: bad input");

        let err = CardDavError::ParseError("parse failed".to_string());
        assert_eq!(err.to_string(), "Parse error: parse failed");
    }

    // === URL Building Tests ===

    #[test]
    fn addressbook_url_relative_path() {
        let config = CardDavConfig {
            server_url: "https://dav.example.com/dav.php".to_string(),
            username: "user".to_string(),
            password: String::new(),
            addressbook_path: None,
            verify_certs: true,
            timeout_secs: 30,
        };
        let client = HttpCardDavClient::new(config).expect("client");
        let url = client.addressbook_url("/addressbooks/user/default");
        assert_eq!(
            url,
            "https://dav.example.com/dav.php/addressbooks/user/default"
        );
    }

    #[test]
    fn addressbook_url_absolute_url() {
        let config = CardDavConfig {
            server_url: "https://dav.example.com".to_string(),
            username: "user".to_string(),
            password: String::new(),
            addressbook_path: None,
            verify_certs: true,
            timeout_secs: 30,
        };
        let client = HttpCardDavClient::new(config).expect("client");
        let url = client.addressbook_url("https://other.com/addressbooks/x");
        assert_eq!(url, "https://other.com/addressbooks/x");
    }

    #[test]
    fn contact_url_builds_correctly() {
        let config = CardDavConfig {
            server_url: "https://dav.example.com/dav.php".to_string(),
            username: "user".to_string(),
            password: String::new(),
            addressbook_path: None,
            verify_certs: true,
            timeout_secs: 30,
        };
        let client = HttpCardDavClient::new(config).expect("client");
        let url = client.contact_url("/addressbooks/user/default", "uid-123");
        assert_eq!(
            url,
            "https://dav.example.com/dav.php/addressbooks/user/default/uid-123.vcf"
        );
    }

    #[test]
    fn contact_url_strips_existing_vcf_extension() {
        let config = CardDavConfig {
            server_url: "https://dav.example.com".to_string(),
            username: "user".to_string(),
            password: String::new(),
            addressbook_path: None,
            verify_certs: true,
            timeout_secs: 30,
        };
        let client = HttpCardDavClient::new(config).expect("client");
        let url = client.contact_url("/books/default", "uid-123.vcf");
        assert_eq!(url, "https://dav.example.com/books/default/uid-123.vcf");
    }

    // === vCard Parsing Tests ===

    #[test]
    fn parse_vcard_basic() {
        let vcard = "BEGIN:VCARD\r\nVERSION:3.0\r\nUID:uid-1\r\nFN:Max Mustermann\r\nN:Mustermann;Max;;;\r\nEMAIL;TYPE=work:max@example.com\r\nTEL;TYPE=cell:+49123456\r\nEND:VCARD\r\n";
        let contact = parse_vcard(vcard).expect("parse");
        assert_eq!(contact.id, "uid-1");
        assert_eq!(contact.display_name.as_deref(), Some("Max Mustermann"));
        assert_eq!(contact.first_name.as_deref(), Some("Max"));
        assert_eq!(contact.last_name.as_deref(), Some("Mustermann"));
        assert_eq!(contact.emails.len(), 1);
        assert_eq!(contact.emails[0].value, "max@example.com");
        assert_eq!(contact.emails[0].type_label.as_deref(), Some("work"));
        assert_eq!(contact.phones.len(), 1);
        assert_eq!(contact.phones[0].value, "+49123456");
    }

    #[test]
    fn parse_vcard_extended_fields() {
        let vcard = "BEGIN:VCARD\r\nVERSION:3.0\r\nUID:uid-2\r\nFN:Erika Muster\r\nN:Muster;Erika;;;\r\nORG:ACME Corp\r\nTITLE:CEO\r\nADR;TYPE=home:;;123 Main St;Springfield;IL;62701;USA\r\nBDAY:1990-05-15\r\nNOTE:A note\r\nPHOTO;VALUE=URI:https://example.com/photo.jpg\r\nCATEGORIES:friends,family\r\nREV:20240101T120000Z\r\nEND:VCARD\r\n";
        let contact = parse_vcard(vcard).expect("parse");
        assert_eq!(contact.organization.as_deref(), Some("ACME Corp"));
        assert_eq!(contact.title.as_deref(), Some("CEO"));
        assert_eq!(contact.addresses.len(), 1);
        assert_eq!(contact.addresses[0].street.as_deref(), Some("123 Main St"));
        assert_eq!(contact.addresses[0].city.as_deref(), Some("Springfield"));
        assert_eq!(contact.addresses[0].country.as_deref(), Some("USA"));
        assert_eq!(contact.birthday, NaiveDate::from_ymd_opt(1990, 5, 15));
        assert_eq!(contact.notes.as_deref(), Some("A note"));
        assert_eq!(
            contact.photo_url.as_deref(),
            Some("https://example.com/photo.jpg")
        );
        assert_eq!(contact.categories, vec!["friends", "family"]);
        assert!(contact.last_modified.is_some());
    }

    #[test]
    fn parse_vcard_minimal() {
        let vcard = "BEGIN:VCARD\r\nVERSION:3.0\r\nFN:Just A Name\r\nEND:VCARD\r\n";
        let contact = parse_vcard(vcard).expect("parse");
        assert_eq!(contact.display_name.as_deref(), Some("Just A Name"));
        assert!(!contact.id.is_empty()); // Auto-generated UUID
        assert!(contact.emails.is_empty());
        assert!(contact.phones.is_empty());
    }

    #[test]
    fn parse_vcard_multiple_emails_phones() {
        let vcard = "BEGIN:VCARD\r\nVERSION:3.0\r\nUID:multi\r\nFN:Multi\r\nEMAIL;TYPE=work:work@example.com\r\nEMAIL;TYPE=home:home@example.com\r\nTEL;TYPE=cell:+49111\r\nTEL;TYPE=work:+49222\r\nEND:VCARD\r\n";
        let contact = parse_vcard(vcard).expect("parse");
        assert_eq!(contact.emails.len(), 2);
        assert_eq!(contact.phones.len(), 2);
    }

    #[test]
    fn parse_vcard_birthday_compact_format() {
        let vcard =
            "BEGIN:VCARD\r\nVERSION:3.0\r\nUID:bday\r\nFN:Bday\r\nBDAY:19900515\r\nEND:VCARD\r\n";
        let contact = parse_vcard(vcard).expect("parse");
        assert_eq!(contact.birthday, NaiveDate::from_ymd_opt(1990, 5, 15));
    }

    #[test]
    fn parse_vcard_line_folding() {
        let vcard = "BEGIN:VCARD\r\nVERSION:3.0\r\nUID:fold-test\r\nFN:A Very Long\r\n  Display Name\r\nEND:VCARD\r\n";
        let contact = parse_vcard(vcard).expect("parse");
        assert_eq!(
            contact.display_name.as_deref(),
            Some("A Very Long Display Name")
        );
    }

    #[test]
    fn parse_vcard_org_with_semicolons() {
        let vcard = "BEGIN:VCARD\r\nVERSION:3.0\r\nUID:org\r\nFN:Test\r\nORG:ACME;Division;Team\r\nEND:VCARD\r\n";
        let contact = parse_vcard(vcard).expect("parse");
        assert_eq!(
            contact.organization.as_deref(),
            Some("ACME, Division, Team")
        );
    }

    // === vCard Building Tests ===

    #[test]
    fn build_vcard_basic() {
        let contact = Contact::new("uid-1", "Max Mustermann")
            .with_first_name("Max")
            .with_last_name("Mustermann");
        let vcard = build_vcard(&contact);
        assert!(vcard.contains("BEGIN:VCARD"));
        assert!(vcard.contains("VERSION:3.0"));
        assert!(vcard.contains("UID:uid-1"));
        assert!(vcard.contains("FN:Max Mustermann"));
        assert!(vcard.contains("N:Mustermann;Max;;;"));
        assert!(vcard.contains("END:VCARD"));
        assert!(vcard.contains("PRODID:-//PiSovereign//CardDAV Client//EN"));
    }

    #[test]
    fn build_vcard_all_fields() {
        let contact = Contact::new("uid-full", "Erika Muster")
            .with_first_name("Erika")
            .with_last_name("Muster")
            .with_email("erika@example.com", Some("work".to_string()))
            .with_phone("+49123", Some("cell".to_string()))
            .with_organization("ACME")
            .with_title("CEO")
            .with_address(
                ContactAddress::new(Some("home".to_string()))
                    .with_street("123 Main St")
                    .with_city("Berlin")
                    .with_country("Germany"),
            )
            .with_birthday(NaiveDate::from_ymd_opt(1990, 5, 15).expect("valid date"))
            .with_notes("Important")
            .with_photo_url("https://example.com/photo.jpg")
            .with_category("friends");

        let vcard = build_vcard(&contact);
        assert!(vcard.contains("EMAIL;TYPE=work:erika@example.com"));
        assert!(vcard.contains("TEL;TYPE=cell:+49123"));
        assert!(vcard.contains("ORG:ACME"));
        assert!(vcard.contains("TITLE:CEO"));
        assert!(vcard.contains("ADR;TYPE=home:;;123 Main St;Berlin;;"));
        assert!(vcard.contains("BDAY:1990-05-15"));
        assert!(vcard.contains("NOTE:Important"));
        assert!(vcard.contains("PHOTO;VALUE=URI:https://example.com/photo.jpg"));
        assert!(vcard.contains("CATEGORIES:friends"));
    }

    #[test]
    fn build_vcard_empty_display_name_uses_unknown() {
        let mut contact = Contact::new("uid-1", "");
        contact.display_name = Some(String::new());
        contact.first_name = None;
        contact.last_name = None;
        let vcard = build_vcard(&contact);
        assert!(vcard.contains("FN:Unknown"));
    }

    #[test]
    fn build_and_parse_roundtrip() {
        let original = Contact::new("uid-rt", "Max Mustermann")
            .with_first_name("Max")
            .with_last_name("Mustermann")
            .with_email("max@example.com", Some("work".to_string()))
            .with_phone("+49123", None)
            .with_organization("ACME")
            .with_birthday(NaiveDate::from_ymd_opt(1990, 1, 15).expect("valid date"))
            .with_notes("Test note")
            .with_category("work");

        let vcard = build_vcard(&original);
        let parsed = parse_vcard(&vcard).expect("parse roundtrip");

        assert_eq!(parsed.id, original.id);
        assert_eq!(parsed.display_name, original.display_name);
        assert_eq!(parsed.first_name, original.first_name);
        assert_eq!(parsed.last_name, original.last_name);
        assert_eq!(parsed.emails.len(), original.emails.len());
        assert_eq!(parsed.emails[0].value, original.emails[0].value);
        assert_eq!(parsed.phones.len(), original.phones.len());
        assert_eq!(parsed.organization, original.organization);
        assert_eq!(parsed.birthday, original.birthday);
        assert_eq!(parsed.notes, original.notes);
        assert_eq!(parsed.categories, original.categories);
    }

    // === XML Extraction Tests ===

    #[test]
    fn extract_vcard_from_xml_single() {
        let xml = r#"<?xml version="1.0" encoding="utf-8"?>
<D:multistatus xmlns:D="DAV:" xmlns:card="urn:ietf:params:xml:ns:carddav">
  <D:response>
    <D:propstat>
      <D:prop>
        <card:address-data>BEGIN:VCARD
VERSION:3.0
UID:uid-1
FN:Max
END:VCARD</card:address-data>
      </D:prop>
    </D:propstat>
  </D:response>
</D:multistatus>"#;
        let vcards = HttpCardDavClient::extract_vcard_data_from_xml(xml);
        assert_eq!(vcards.len(), 1);
        assert!(vcards[0].contains("BEGIN:VCARD"));
        assert!(vcards[0].contains("UID:uid-1"));
    }

    #[test]
    fn extract_vcard_from_xml_multiple() {
        let xml = r#"<?xml version="1.0" encoding="utf-8"?>
<D:multistatus xmlns:D="DAV:" xmlns:card="urn:ietf:params:xml:ns:carddav">
  <D:response>
    <D:propstat>
      <D:prop>
        <card:address-data>BEGIN:VCARD
UID:uid-1
FN:Max
END:VCARD</card:address-data>
      </D:prop>
    </D:propstat>
  </D:response>
  <D:response>
    <D:propstat>
      <D:prop>
        <card:address-data>BEGIN:VCARD
UID:uid-2
FN:Erika
END:VCARD</card:address-data>
      </D:prop>
    </D:propstat>
  </D:response>
</D:multistatus>"#;
        let vcards = HttpCardDavClient::extract_vcard_data_from_xml(xml);
        assert_eq!(vcards.len(), 2);
    }

    #[test]
    fn extract_vcard_from_xml_empty() {
        let xml = r#"<?xml version="1.0" encoding="utf-8"?>
<D:multistatus xmlns:D="DAV:">
</D:multistatus>"#;
        let vcards = HttpCardDavClient::extract_vcard_data_from_xml(xml);
        assert!(vcards.is_empty());
    }

    // === Addressbook Discovery Tests ===

    #[test]
    fn extract_addressbook_hrefs_from_propfind() {
        let xml = r#"<?xml version="1.0" encoding="utf-8"?>
<D:multistatus xmlns:D="DAV:" xmlns:card="urn:ietf:params:xml:ns:carddav">
  <D:response>
    <D:href>/dav.php/addressbooks/user/default/</D:href>
    <D:propstat>
      <D:prop>
        <D:resourcetype>
          <D:collection/>
          <card:addressbook/>
        </D:resourcetype>
      </D:prop>
    </D:propstat>
  </D:response>
  <D:response>
    <D:href>/dav.php/addressbooks/user/</D:href>
    <D:propstat>
      <D:prop>
        <D:resourcetype>
          <D:collection/>
        </D:resourcetype>
      </D:prop>
    </D:propstat>
  </D:response>
</D:multistatus>"#;
        let hrefs = HttpCardDavClient::extract_addressbook_hrefs(xml);
        assert_eq!(hrefs.len(), 1);
        assert_eq!(hrefs[0], "/dav.php/addressbooks/user/default/");
    }

    #[test]
    fn extract_addressbook_hrefs_empty() {
        let xml = r#"<D:multistatus xmlns:D="DAV:"></D:multistatus>"#;
        let hrefs = HttpCardDavClient::extract_addressbook_hrefs(xml);
        assert!(hrefs.is_empty());
    }

    // === Helper Function Tests ===

    #[test]
    fn unfold_lines_works() {
        let data = "PROP:value1\r\n continued\r\nPROP2:value2";
        let lines = unfold_vcard_lines(data);
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0], "PROP:value1continued");
        assert_eq!(lines[1], "PROP2:value2");
    }

    #[test]
    fn extract_type_param_works() {
        assert_eq!(
            extract_type_param(Some("TYPE=work")),
            Some("work".to_string())
        );
        assert_eq!(
            extract_type_param(Some("TYPE=HOME")),
            Some("home".to_string())
        );
        assert_eq!(extract_type_param(Some("WORK")), Some("work".to_string()));
        assert_eq!(extract_type_param(Some("X-CUSTOM=foo")), None);
        assert_eq!(extract_type_param(None), None);
    }

    #[test]
    fn parse_vcard_date_formats() {
        assert_eq!(
            parse_vcard_date("1990-05-15"),
            NaiveDate::from_ymd_opt(1990, 5, 15)
        );
        assert_eq!(
            parse_vcard_date("19900515"),
            NaiveDate::from_ymd_opt(1990, 5, 15)
        );
        assert_eq!(parse_vcard_date("invalid"), None);
    }

    #[test]
    fn parse_vcard_datetime_formats() {
        assert!(parse_vcard_datetime("20240101T120000Z").is_some());
        assert!(parse_vcard_datetime("2024-01-01T12:00:00Z").is_some());
        assert!(parse_vcard_datetime("invalid").is_none());
    }

    // === Client Construction Tests ===

    #[test]
    fn client_construction() {
        let config = CardDavConfig {
            server_url: "https://dav.example.com".to_string(),
            username: "user".to_string(),
            password: "pass".to_string(),
            addressbook_path: Some("/addressbooks/user/default".to_string()),
            verify_certs: true,
            timeout_secs: 30,
        };
        let client = HttpCardDavClient::new(config);
        assert!(client.is_ok());
    }
}
