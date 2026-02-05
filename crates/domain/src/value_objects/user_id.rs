//! User identifier value object
//!
//! # Examples
//!
//! ```
//! use domain::UserId;
//!
//! // Create a new random user ID
//! let user_id = UserId::new();
//! assert!(!user_id.to_string().is_empty());
//!
//! // Parse from string
//! let parsed = UserId::parse("550e8400-e29b-41d4-a716-446655440000").unwrap();
//! assert_eq!(parsed.to_string(), "550e8400-e29b-41d4-a716-446655440000");
//! ```

use std::fmt;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// A unique user identifier
///
/// # Examples
///
/// ```
/// use domain::UserId;
///
/// let user_id = UserId::new();
/// println!("User ID: {}", user_id);
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct UserId(Uuid);

impl UserId {
    /// Create a new random user ID
    ///
    /// # Examples
    ///
    /// ```
    /// use domain::UserId;
    ///
    /// let id1 = UserId::new();
    /// let id2 = UserId::new();
    /// assert_ne!(id1, id2);
    /// ```
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    /// Create a user ID from an existing UUID
    ///
    /// # Examples
    ///
    /// ```
    /// use domain::UserId;
    /// use uuid::Uuid;
    ///
    /// let uuid = Uuid::new_v4();
    /// let user_id = UserId::from_uuid(uuid);
    /// assert_eq!(user_id.as_uuid(), uuid);
    /// ```
    pub const fn from_uuid(uuid: Uuid) -> Self {
        Self(uuid)
    }

    /// Parse a user ID from a string
    ///
    /// # Examples
    ///
    /// ```
    /// use domain::UserId;
    ///
    /// let user_id = UserId::parse("550e8400-e29b-41d4-a716-446655440000").unwrap();
    /// assert!(UserId::parse("invalid").is_err());
    /// ```
    pub fn parse(s: &str) -> Result<Self, uuid::Error> {
        Ok(Self(Uuid::parse_str(s)?))
    }

    /// Get the underlying UUID
    pub const fn as_uuid(&self) -> Uuid {
        self.0
    }
}

impl Default for UserId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for UserId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<Uuid> for UserId {
    fn from(uuid: Uuid) -> Self {
        Self(uuid)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_user_id_is_unique() {
        let id1 = UserId::new();
        let id2 = UserId::new();
        assert_ne!(id1, id2);
    }

    #[test]
    fn user_id_can_be_parsed() {
        let original = UserId::new();
        let parsed = UserId::parse(&original.to_string()).unwrap();
        assert_eq!(original, parsed);
    }

    #[test]
    fn from_uuid() {
        let uuid = Uuid::new_v4();
        let id = UserId::from_uuid(uuid);
        assert_eq!(id.as_uuid(), uuid);
    }

    #[test]
    fn from_uuid_trait() {
        let uuid = Uuid::new_v4();
        let id: UserId = uuid.into();
        assert_eq!(id.as_uuid(), uuid);
    }

    #[test]
    fn default_creates_new() {
        let id1 = UserId::default();
        let id2 = UserId::default();
        assert_ne!(id1, id2);
    }

    #[test]
    fn display_format() {
        let uuid = Uuid::parse_str("550e8400-e29b-41d4-a716-446655440000").unwrap();
        let id = UserId::from_uuid(uuid);
        assert_eq!(id.to_string(), "550e8400-e29b-41d4-a716-446655440000");
    }

    #[test]
    fn parse_invalid_returns_error() {
        let result = UserId::parse("not-a-uuid");
        assert!(result.is_err());
    }

    #[test]
    fn serialization() {
        let id = UserId::new();
        let json = serde_json::to_string(&id).unwrap();
        let parsed: UserId = serde_json::from_str(&json).unwrap();
        assert_eq!(id, parsed);
    }

    #[test]
    fn hash_works() {
        use std::collections::HashSet;
        let id1 = UserId::new();
        let id2 = UserId::new();
        let mut set = HashSet::new();
        set.insert(id1);
        set.insert(id2);
        assert_eq!(set.len(), 2);
    }

    #[test]
    fn clone_and_copy() {
        let id = UserId::new();
        #[allow(clippy::clone_on_copy)]
        let cloned = id.clone();
        let copied = id;
        assert_eq!(id, cloned);
        assert_eq!(id, copied);
    }
}
