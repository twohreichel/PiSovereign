//! Approval request identifier for tracking pending approvals

use std::fmt;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// A unique approval request identifier
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ApprovalId(Uuid);

impl ApprovalId {
    /// Create a new random approval ID
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    /// Create an approval ID from an existing UUID
    pub const fn from_uuid(uuid: Uuid) -> Self {
        Self(uuid)
    }

    /// Parse an approval ID from a string
    pub fn parse(s: &str) -> Result<Self, uuid::Error> {
        Ok(Self(Uuid::parse_str(s)?))
    }

    /// Get the underlying UUID
    pub const fn as_uuid(&self) -> Uuid {
        self.0
    }
}

impl Default for ApprovalId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for ApprovalId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<Uuid> for ApprovalId {
    fn from(uuid: Uuid) -> Self {
        Self(uuid)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_approval_id_is_unique() {
        let id1 = ApprovalId::new();
        let id2 = ApprovalId::new();
        assert_ne!(id1, id2);
    }

    #[test]
    fn approval_id_roundtrips_through_string() {
        let original = ApprovalId::new();
        let parsed = ApprovalId::parse(&original.to_string()).unwrap();
        assert_eq!(original, parsed);
    }

    #[test]
    fn from_uuid() {
        let uuid = Uuid::new_v4();
        let id = ApprovalId::from_uuid(uuid);
        assert_eq!(id.as_uuid(), uuid);
    }

    #[test]
    fn from_uuid_trait() {
        let uuid = Uuid::new_v4();
        let id: ApprovalId = uuid.into();
        assert_eq!(id.as_uuid(), uuid);
    }

    #[test]
    fn parse_invalid_returns_error() {
        let result = ApprovalId::parse("not-a-uuid");
        assert!(result.is_err());
    }

    #[test]
    fn default_creates_new() {
        let id = ApprovalId::default();
        // Just ensure it doesn't panic
        assert!(!id.to_string().is_empty());
    }

    #[test]
    fn display_format() {
        let uuid = Uuid::parse_str("550e8400-e29b-41d4-a716-446655440000").unwrap();
        let id = ApprovalId::from_uuid(uuid);
        assert_eq!(id.to_string(), "550e8400-e29b-41d4-a716-446655440000");
    }

    #[test]
    fn clone_and_copy() {
        let id1 = ApprovalId::new();
        let id2 = id1;
        #[allow(clippy::clone_on_copy)]
        let id3 = id1.clone();
        assert_eq!(id1, id2);
        assert_eq!(id1, id3);
    }

    #[test]
    fn hash_works() {
        use std::collections::HashSet;

        let id1 = ApprovalId::new();
        let id2 = ApprovalId::new();
        let mut set = HashSet::new();
        set.insert(id1);
        set.insert(id2);
        assert_eq!(set.len(), 2);
    }

    #[test]
    fn serialization() {
        let id = ApprovalId::new();
        let json = serde_json::to_string(&id).unwrap();
        let deserialized: ApprovalId = serde_json::from_str(&json).unwrap();
        assert_eq!(id, deserialized);
    }
}
