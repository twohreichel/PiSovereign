//! Tenant identifier value object
//!
//! # Examples
//!
//! ```
//! use domain::TenantId;
//!
//! // Create a new random tenant ID
//! let tenant_id = TenantId::new();
//! assert!(!tenant_id.to_string().is_empty());
//!
//! // Parse from string
//! let parsed = TenantId::parse("550e8400-e29b-41d4-a716-446655440000").unwrap();
//! assert_eq!(parsed.to_string(), "550e8400-e29b-41d4-a716-446655440000");
//!
//! // Use the default tenant for single-tenant deployments
//! let default = TenantId::default();
//! assert_eq!(default, TenantId::default());
//! ```

use std::fmt;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// A unique tenant identifier
///
/// Tenants are isolated organizational units within the system. Each tenant
/// has its own data, users, and configuration. In single-tenant deployments,
/// the [`TenantId::default()`] can be used.
///
/// # Examples
///
/// ```
/// use domain::TenantId;
///
/// let tenant_id = TenantId::new();
/// println!("Tenant ID: {}", tenant_id);
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TenantId(Uuid);

/// The default tenant UUID for single-tenant deployments
///
/// This UUID is deterministic: 00000000-0000-0000-0000-000000000001
const DEFAULT_TENANT_UUID: Uuid = Uuid::from_u128(1);

impl TenantId {
    /// Create a new random tenant ID
    ///
    /// # Examples
    ///
    /// ```
    /// use domain::TenantId;
    ///
    /// let id1 = TenantId::new();
    /// let id2 = TenantId::new();
    /// assert_ne!(id1, id2);
    /// ```
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    /// Create a tenant ID from an existing UUID
    ///
    /// # Examples
    ///
    /// ```
    /// use domain::TenantId;
    /// use uuid::Uuid;
    ///
    /// let uuid = Uuid::new_v4();
    /// let tenant_id = TenantId::from_uuid(uuid);
    /// assert_eq!(tenant_id.as_uuid(), uuid);
    /// ```
    pub const fn from_uuid(uuid: Uuid) -> Self {
        Self(uuid)
    }

    /// Parse a tenant ID from a string
    ///
    /// # Examples
    ///
    /// ```
    /// use domain::TenantId;
    ///
    /// let tenant_id = TenantId::parse("550e8400-e29b-41d4-a716-446655440000").unwrap();
    /// assert!(TenantId::parse("invalid").is_err());
    /// ```
    pub fn parse(s: &str) -> Result<Self, uuid::Error> {
        Ok(Self(Uuid::parse_str(s)?))
    }

    /// Get the underlying UUID
    pub const fn as_uuid(&self) -> Uuid {
        self.0
    }

    /// Check if this is the default tenant
    ///
    /// # Examples
    ///
    /// ```
    /// use domain::TenantId;
    ///
    /// assert!(TenantId::default().is_default());
    /// assert!(!TenantId::new().is_default());
    /// ```
    pub const fn is_default(&self) -> bool {
        self.0.as_u128() == DEFAULT_TENANT_UUID.as_u128()
    }
}

impl Default for TenantId {
    /// Returns the default tenant ID for single-tenant deployments
    ///
    /// The default tenant uses a deterministic UUID (00000000-0000-0000-0000-000000000001)
    /// to ensure consistency across application restarts.
    fn default() -> Self {
        Self(DEFAULT_TENANT_UUID)
    }
}

impl fmt::Display for TenantId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<Uuid> for TenantId {
    fn from(uuid: Uuid) -> Self {
        Self(uuid)
    }
}

impl From<TenantId> for Uuid {
    fn from(id: TenantId) -> Self {
        id.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_creates_unique_ids() {
        let id1 = TenantId::new();
        let id2 = TenantId::new();
        assert_ne!(id1, id2);
    }

    #[test]
    fn test_default_is_consistent() {
        let default1 = TenantId::default();
        let default2 = TenantId::default();
        assert_eq!(default1, default2);
    }

    #[test]
    fn test_default_is_not_random() {
        let default = TenantId::default();
        let random = TenantId::new();
        assert_ne!(default, random);
    }

    #[test]
    fn test_is_default() {
        assert!(TenantId::default().is_default());
        assert!(!TenantId::new().is_default());
    }

    #[test]
    fn test_parse_valid_uuid() {
        let uuid_str = "550e8400-e29b-41d4-a716-446655440000";
        let result = TenantId::parse(uuid_str);
        assert!(result.is_ok());
        assert_eq!(result.unwrap().to_string(), uuid_str);
    }

    #[test]
    fn test_parse_invalid_uuid() {
        assert!(TenantId::parse("not-a-uuid").is_err());
        assert!(TenantId::parse("").is_err());
    }

    #[test]
    fn test_from_uuid() {
        let uuid = Uuid::new_v4();
        let tenant_id = TenantId::from_uuid(uuid);
        assert_eq!(tenant_id.as_uuid(), uuid);
    }

    #[test]
    fn test_uuid_conversions() {
        let original_uuid = Uuid::new_v4();
        let tenant_id = TenantId::from(original_uuid);
        let converted_uuid: Uuid = tenant_id.into();
        assert_eq!(original_uuid, converted_uuid);
    }

    #[test]
    fn test_display() {
        let uuid_str = "550e8400-e29b-41d4-a716-446655440000";
        let tenant_id = TenantId::parse(uuid_str).unwrap();
        assert_eq!(format!("{tenant_id}"), uuid_str);
    }

    #[test]
    fn test_serialization() {
        let tenant_id = TenantId::parse("550e8400-e29b-41d4-a716-446655440000").unwrap();
        let json = serde_json::to_string(&tenant_id).unwrap();
        let deserialized: TenantId = serde_json::from_str(&json).unwrap();
        assert_eq!(tenant_id, deserialized);
    }
}
