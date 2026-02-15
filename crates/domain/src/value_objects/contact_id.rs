//! Contact identifier value object
//!
//! # Examples
//!
//! ```
//! use domain::ContactId;
//!
//! // Create a new random contact ID
//! let contact_id = ContactId::new();
//! assert!(!contact_id.to_string().is_empty());
//!
//! // Parse from string
//! let parsed = ContactId::parse("550e8400-e29b-41d4-a716-446655440000").unwrap();
//! assert_eq!(parsed.to_string(), "550e8400-e29b-41d4-a716-446655440000");
//! ```

use std::fmt;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// A unique contact identifier
///
/// # Examples
///
/// ```
/// use domain::ContactId;
///
/// let contact_id = ContactId::new();
/// println!("Contact ID: {}", contact_id);
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ContactId(Uuid);

impl ContactId {
    /// Create a new random contact ID
    ///
    /// # Examples
    ///
    /// ```
    /// use domain::ContactId;
    ///
    /// let id1 = ContactId::new();
    /// let id2 = ContactId::new();
    /// assert_ne!(id1, id2);
    /// ```
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    /// Create a contact ID from an existing UUID
    pub const fn from_uuid(uuid: Uuid) -> Self {
        Self(uuid)
    }

    /// Parse a contact ID from a string
    ///
    /// # Examples
    ///
    /// ```
    /// use domain::ContactId;
    ///
    /// let contact_id = ContactId::parse("550e8400-e29b-41d4-a716-446655440000").unwrap();
    /// assert!(ContactId::parse("invalid").is_err());
    /// ```
    pub fn parse(s: &str) -> Result<Self, uuid::Error> {
        Ok(Self(Uuid::parse_str(s)?))
    }

    /// Get the underlying UUID
    pub const fn as_uuid(&self) -> Uuid {
        self.0
    }
}

impl Default for ContactId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for ContactId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<Uuid> for ContactId {
    fn from(uuid: Uuid) -> Self {
        Self(uuid)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_contact_id_is_unique() {
        let id1 = ContactId::new();
        let id2 = ContactId::new();
        assert_ne!(id1, id2);
    }

    #[test]
    fn contact_id_can_be_parsed() {
        let original = ContactId::new();
        let parsed = ContactId::parse(&original.to_string()).expect("parse");
        assert_eq!(original, parsed);
    }

    #[test]
    fn from_uuid() {
        let uuid = Uuid::new_v4();
        let id = ContactId::from_uuid(uuid);
        assert_eq!(id.as_uuid(), uuid);
    }

    #[test]
    fn from_uuid_trait() {
        let uuid = Uuid::new_v4();
        let id: ContactId = uuid.into();
        assert_eq!(id.as_uuid(), uuid);
    }

    #[test]
    fn default_creates_unique() {
        let id1 = ContactId::default();
        let id2 = ContactId::default();
        assert_ne!(id1, id2);
    }

    #[test]
    fn display_format() {
        let uuid = Uuid::parse_str("550e8400-e29b-41d4-a716-446655440000").expect("parse");
        let id = ContactId::from_uuid(uuid);
        assert_eq!(id.to_string(), "550e8400-e29b-41d4-a716-446655440000");
    }

    #[test]
    fn parse_invalid_returns_error() {
        let result = ContactId::parse("not-a-uuid");
        assert!(result.is_err());
    }

    #[test]
    fn serialization_roundtrip() {
        let id = ContactId::new();
        let json = serde_json::to_string(&id).expect("serialize");
        let parsed: ContactId = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(id, parsed);
    }

    #[test]
    fn hash_works() {
        use std::collections::HashSet;
        let id1 = ContactId::new();
        let id2 = ContactId::new();
        let mut set = HashSet::new();
        set.insert(id1);
        set.insert(id2);
        assert_eq!(set.len(), 2);
    }

    #[test]
    fn clone_and_copy() {
        let id = ContactId::new();
        #[allow(clippy::clone_on_copy)]
        let cloned = id.clone();
        let copied = id;
        assert_eq!(id, cloned);
        assert_eq!(id, copied);
    }
}
