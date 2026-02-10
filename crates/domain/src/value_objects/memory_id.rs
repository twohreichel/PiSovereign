//! Memory identifier for tracking knowledge entries

use std::fmt;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// A unique memory/knowledge entry identifier
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct MemoryId(Uuid);

impl MemoryId {
    /// Create a new random memory ID
    #[must_use]
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    /// Create a memory ID from an existing UUID
    #[must_use]
    pub const fn from_uuid(uuid: Uuid) -> Self {
        Self(uuid)
    }

    /// Parse a memory ID from a string
    ///
    /// # Errors
    ///
    /// Returns an error if the string is not a valid UUID.
    pub fn parse(s: &str) -> Result<Self, uuid::Error> {
        Ok(Self(Uuid::parse_str(s)?))
    }

    /// Get the underlying UUID
    #[must_use]
    pub const fn as_uuid(&self) -> Uuid {
        self.0
    }
}

impl Default for MemoryId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for MemoryId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<Uuid> for MemoryId {
    fn from(uuid: Uuid) -> Self {
        Self(uuid)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_memory_id_is_unique() {
        let id1 = MemoryId::new();
        let id2 = MemoryId::new();
        assert_ne!(id1, id2);
    }

    #[test]
    fn memory_id_roundtrips_through_string() {
        let original = MemoryId::new();
        let parsed = MemoryId::parse(&original.to_string()).unwrap();
        assert_eq!(original, parsed);
    }

    #[test]
    fn from_uuid() {
        let uuid = Uuid::new_v4();
        let id = MemoryId::from_uuid(uuid);
        assert_eq!(id.as_uuid(), uuid);
    }

    #[test]
    fn from_uuid_trait() {
        let uuid = Uuid::new_v4();
        let id: MemoryId = uuid.into();
        assert_eq!(id.as_uuid(), uuid);
    }

    #[test]
    fn default_creates_new() {
        let id1 = MemoryId::default();
        let id2 = MemoryId::default();
        assert_ne!(id1, id2);
    }

    #[test]
    fn display_format() {
        let uuid = Uuid::parse_str("550e8400-e29b-41d4-a716-446655440000").unwrap();
        let id = MemoryId::from_uuid(uuid);
        assert_eq!(id.to_string(), "550e8400-e29b-41d4-a716-446655440000");
    }

    #[test]
    fn parse_invalid_returns_error() {
        let result = MemoryId::parse("not-a-uuid");
        assert!(result.is_err());
    }

    #[test]
    fn serialize_deserialize() {
        let original = MemoryId::new();
        let serialized = serde_json::to_string(&original).unwrap();
        let deserialized: MemoryId = serde_json::from_str(&serialized).unwrap();
        assert_eq!(original, deserialized);
    }

    #[test]
    fn hash_works() {
        use std::collections::HashSet;
        let mut set = HashSet::new();
        let id = MemoryId::new();
        set.insert(id);
        assert!(set.contains(&id));
    }
}
