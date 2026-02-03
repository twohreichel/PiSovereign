//! Conversation identifier for tracking chat sessions

use std::fmt;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// A unique conversation/session identifier
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ConversationId(Uuid);

impl ConversationId {
    /// Create a new random conversation ID
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    /// Create a conversation ID from an existing UUID
    pub const fn from_uuid(uuid: Uuid) -> Self {
        Self(uuid)
    }

    /// Parse a conversation ID from a string
    pub fn parse(s: &str) -> Result<Self, uuid::Error> {
        Ok(Self(Uuid::parse_str(s)?))
    }

    /// Get the underlying UUID
    pub const fn as_uuid(&self) -> Uuid {
        self.0
    }
}

impl Default for ConversationId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for ConversationId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<Uuid> for ConversationId {
    fn from(uuid: Uuid) -> Self {
        Self(uuid)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_conversation_id_is_unique() {
        let id1 = ConversationId::new();
        let id2 = ConversationId::new();
        assert_ne!(id1, id2);
    }

    #[test]
    fn conversation_id_roundtrips_through_string() {
        let original = ConversationId::new();
        let parsed = ConversationId::parse(&original.to_string()).unwrap();
        assert_eq!(original, parsed);
    }

    #[test]
    fn from_uuid() {
        let uuid = Uuid::new_v4();
        let id = ConversationId::from_uuid(uuid);
        assert_eq!(id.as_uuid(), uuid);
    }

    #[test]
    fn from_uuid_trait() {
        let uuid = Uuid::new_v4();
        let id: ConversationId = uuid.into();
        assert_eq!(id.as_uuid(), uuid);
    }

    #[test]
    fn default_creates_new() {
        let id1 = ConversationId::default();
        let id2 = ConversationId::default();
        assert_ne!(id1, id2);
    }

    #[test]
    fn display_format() {
        let uuid = Uuid::parse_str("550e8400-e29b-41d4-a716-446655440000").unwrap();
        let id = ConversationId::from_uuid(uuid);
        assert_eq!(id.to_string(), "550e8400-e29b-41d4-a716-446655440000");
    }

    #[test]
    fn parse_invalid_returns_error() {
        let result = ConversationId::parse("not-a-uuid");
        assert!(result.is_err());
    }

    #[test]
    fn serialization() {
        let id = ConversationId::new();
        let json = serde_json::to_string(&id).unwrap();
        let parsed: ConversationId = serde_json::from_str(&json).unwrap();
        assert_eq!(id, parsed);
    }

    #[test]
    fn hash_works() {
        use std::collections::HashSet;
        let id1 = ConversationId::new();
        let id2 = ConversationId::new();
        let mut set = HashSet::new();
        set.insert(id1);
        set.insert(id2);
        assert_eq!(set.len(), 2);
    }

    #[test]
    fn clone_and_copy() {
        let id = ConversationId::new();
        #[allow(clippy::clone_on_copy)]
        let cloned = id.clone();
        let copied = id;
        assert_eq!(id, cloned);
        assert_eq!(id, copied);
    }
}
