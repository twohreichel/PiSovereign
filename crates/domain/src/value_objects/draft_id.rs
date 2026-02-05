//! Email draft identifier for tracking pending drafts

use std::fmt;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// A unique email draft identifier
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct DraftId(Uuid);

impl DraftId {
    /// Create a new random draft ID
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    /// Create a draft ID from an existing UUID
    pub const fn from_uuid(uuid: Uuid) -> Self {
        Self(uuid)
    }

    /// Parse a draft ID from a string
    ///
    /// # Errors
    /// Returns an error if the string is not a valid UUID
    pub fn parse(s: &str) -> Result<Self, uuid::Error> {
        Ok(Self(Uuid::parse_str(s)?))
    }

    /// Get the underlying UUID
    pub const fn as_uuid(&self) -> Uuid {
        self.0
    }
}

impl Default for DraftId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for DraftId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<Uuid> for DraftId {
    fn from(uuid: Uuid) -> Self {
        Self(uuid)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_draft_id_is_unique() {
        let id1 = DraftId::new();
        let id2 = DraftId::new();
        assert_ne!(id1, id2);
    }

    #[test]
    fn draft_id_roundtrips_through_string() {
        let original = DraftId::new();
        let parsed = DraftId::parse(&original.to_string()).unwrap();
        assert_eq!(original, parsed);
    }

    #[test]
    fn from_uuid() {
        let uuid = Uuid::new_v4();
        let id = DraftId::from_uuid(uuid);
        assert_eq!(id.as_uuid(), uuid);
    }

    #[test]
    fn from_uuid_trait() {
        let uuid = Uuid::new_v4();
        let id: DraftId = uuid.into();
        assert_eq!(id.as_uuid(), uuid);
    }

    #[test]
    fn default_creates_new_id() {
        let id1 = DraftId::default();
        let id2 = DraftId::default();
        assert_ne!(id1, id2);
    }

    #[test]
    fn display_shows_uuid() {
        let uuid = Uuid::new_v4();
        let id = DraftId::from_uuid(uuid);
        assert_eq!(id.to_string(), uuid.to_string());
    }

    #[test]
    fn parse_invalid_uuid_fails() {
        let result = DraftId::parse("not-a-uuid");
        assert!(result.is_err());
    }

    #[test]
    fn draft_id_can_be_hashed() {
        use std::collections::HashSet;
        let mut set = HashSet::new();
        let id = DraftId::new();
        set.insert(id);
        assert!(set.contains(&id));
    }

    #[test]
    fn draft_id_serialization() {
        let id = DraftId::new();
        let json = serde_json::to_string(&id).unwrap();
        let parsed: DraftId = serde_json::from_str(&json).unwrap();
        assert_eq!(id, parsed);
    }

    #[test]
    fn draft_id_debug() {
        let id = DraftId::new();
        let debug = format!("{id:?}");
        assert!(debug.contains("DraftId"));
    }
}
