//! Email address value object with validation

use std::fmt;

use serde::{Deserialize, Serialize};
use validator::Validate;

use crate::errors::DomainError;

/// A validated email address
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, Validate)]
pub struct EmailAddress {
    #[validate(email)]
    value: String,
}

impl EmailAddress {
    /// Create a new email address, validating the format
    pub fn new(email: impl Into<String>) -> Result<Self, DomainError> {
        let value = email.into().trim().to_lowercase();

        let candidate = Self { value };
        candidate
            .validate()
            .map_err(|e| DomainError::InvalidEmailAddress(e.to_string()))?;

        Ok(candidate)
    }

    /// Get the email address as a string slice
    pub fn as_str(&self) -> &str {
        &self.value
    }

    /// Get the local part (before @)
    pub fn local_part(&self) -> &str {
        self.value.split('@').next().unwrap_or("")
    }

    /// Get the domain part (after @)
    pub fn domain(&self) -> &str {
        self.value.split('@').nth(1).unwrap_or("")
    }
}

impl fmt::Display for EmailAddress {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.value)
    }
}

impl TryFrom<String> for EmailAddress {
    type Error = DomainError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl TryFrom<&str> for EmailAddress {
    type Error = DomainError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_email_is_accepted() {
        let email = EmailAddress::new("user@example.com").unwrap();
        assert_eq!(email.as_str(), "user@example.com");
    }

    #[test]
    fn email_is_normalized_to_lowercase() {
        let email = EmailAddress::new("User@Example.COM").unwrap();
        assert_eq!(email.as_str(), "user@example.com");
    }

    #[test]
    fn email_parts_are_extracted() {
        let email = EmailAddress::new("andreas@proton.me").unwrap();
        assert_eq!(email.local_part(), "andreas");
        assert_eq!(email.domain(), "proton.me");
    }

    #[test]
    fn invalid_email_is_rejected() {
        assert!(EmailAddress::new("not-an-email").is_err());
        assert!(EmailAddress::new("@nodomain.com").is_err());
        assert!(EmailAddress::new("noat.com").is_err());
    }

    #[test]
    fn display_format() {
        let email = EmailAddress::new("test@example.com").unwrap();
        assert_eq!(email.to_string(), "test@example.com");
    }

    #[test]
    fn try_from_string() {
        let email: EmailAddress = "test@example.com".to_string().try_into().unwrap();
        assert_eq!(email.as_str(), "test@example.com");
    }

    #[test]
    fn try_from_str() {
        let email: EmailAddress = "test@example.com".try_into().unwrap();
        assert_eq!(email.as_str(), "test@example.com");
    }

    #[test]
    fn serialization() {
        let email = EmailAddress::new("test@example.com").unwrap();
        let json = serde_json::to_string(&email).unwrap();
        let parsed: EmailAddress = serde_json::from_str(&json).unwrap();
        assert_eq!(email, parsed);
    }

    #[test]
    fn hash_works() {
        use std::collections::HashSet;
        let e1 = EmailAddress::new("a@b.com").unwrap();
        let e2 = EmailAddress::new("c@d.com").unwrap();
        let mut set = HashSet::new();
        set.insert(e1.clone());
        set.insert(e2);
        assert_eq!(set.len(), 2);
    }

    #[test]
    fn clone() {
        let email = EmailAddress::new("test@example.com").unwrap();
        let cloned = email.clone();
        assert_eq!(email, cloned);
    }

    #[test]
    fn whitespace_trimmed() {
        let email = EmailAddress::new("  test@example.com  ").unwrap();
        assert_eq!(email.as_str(), "test@example.com");
    }
}
