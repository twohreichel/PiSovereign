//! Conversation identifier for tracking chat sessions

use serde::{Deserialize, Serialize};
use std::fmt;
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
    pub fn from_uuid(uuid: Uuid) -> Self {
        Self(uuid)
    }

    /// Parse a conversation ID from a string
    pub fn parse(s: &str) -> Result<Self, uuid::Error> {
        Ok(Self(Uuid::parse_str(s)?))
    }

    /// Get the underlying UUID
    pub fn as_uuid(&self) -> Uuid {
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
}
