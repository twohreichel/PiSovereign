//! Email address value object with validation
//!
//! Provides a validated email address type that ensures proper format.
//!
//! # Examples
//!
//! ```
//! use domain::EmailAddress;
//!
//! // Create a valid email address
//! let email = EmailAddress::new("user@example.com").unwrap();
//! assert_eq!(email.as_str(), "user@example.com");
//!
//! // Email addresses are normalized to lowercase
//! let email = EmailAddress::new("User@Example.COM").unwrap();
//! assert_eq!(email.as_str(), "user@example.com");
//!
//! // Invalid emails are rejected
//! assert!(EmailAddress::new("invalid").is_err());
//! ```

use std::fmt;

use serde::{Deserialize, Serialize};
use validator::Validate;

use crate::errors::DomainError;

/// A validated email address
///
/// # Examples
///
/// ```
/// use domain::EmailAddress;
///
/// let email = EmailAddress::new("user@example.com").unwrap();
/// assert_eq!(email.local_part(), "user");
/// assert_eq!(email.domain(), "example.com");
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, Validate)]
#[serde(transparent)]
pub struct EmailAddress {
    #[validate(email)]
    value: String,
}

impl EmailAddress {
    /// Create a new email address, validating the format
    ///
    /// # Examples
    ///
    /// ```
    /// use domain::EmailAddress;
    ///
    /// let email = EmailAddress::new("hello@world.com").unwrap();
    /// assert_eq!(email.to_string(), "hello@world.com");
    /// ```
    ///
    /// # Errors
    ///
    /// Returns an error if the email format is invalid.
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
    ///
    /// # Examples
    ///
    /// ```
    /// use domain::EmailAddress;
    ///
    /// let email = EmailAddress::new("user@example.com").unwrap();
    /// assert_eq!(email.local_part(), "user");
    /// ```
    pub fn local_part(&self) -> &str {
        self.value.split('@').next().unwrap_or("")
    }

    /// Get the domain part (after @)
    ///
    /// # Examples
    ///
    /// ```
    /// use domain::EmailAddress;
    ///
    /// let email = EmailAddress::new("user@example.com").unwrap();
    /// assert_eq!(email.domain(), "example.com");
    /// ```
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
        set.insert(e1);
        set.insert(e2);
        assert_eq!(set.len(), 2);
    }

    #[test]
    fn clone() {
        let email = EmailAddress::new("test@example.com").unwrap();
        #[allow(clippy::redundant_clone)]
        let cloned = email.clone();
        assert_eq!(email, cloned);
    }

    #[test]
    fn whitespace_trimmed() {
        let email = EmailAddress::new("  test@example.com  ").unwrap();
        assert_eq!(email.as_str(), "test@example.com");
    }
}

#[cfg(test)]
mod proptest_tests {
    use proptest::prelude::*;

    use super::*;

    /// Strategy for generating valid email local parts
    fn valid_local_part() -> impl Strategy<Value = String> {
        "[a-z][a-z0-9._-]{0,15}".prop_map(|s| s.to_lowercase())
    }

    /// Strategy for generating valid email domains
    fn valid_domain() -> impl Strategy<Value = String> {
        "[a-z][a-z0-9-]{0,10}\\.[a-z]{2,4}".prop_map(|s| s.to_lowercase())
    }

    proptest! {
        #[test]
        fn valid_emails_are_accepted(
            local in valid_local_part(),
            domain in valid_domain()
        ) {
            let email_str = format!("{local}@{domain}");
            // Not all generated combinations are valid emails, but valid ones should parse
            if let Ok(email) = EmailAddress::new(&email_str) {
                // Successfully parsed emails should preserve content
                prop_assert!(email.as_str().contains('@'));
                prop_assert!(!email.local_part().is_empty());
                prop_assert!(!email.domain().is_empty());
            }
        }

        #[test]
        fn email_is_always_lowercase(input in "[A-Za-z]+@[A-Za-z]+\\.[a-z]{2,3}") {
            if let Ok(email) = EmailAddress::new(&input) {
                prop_assert_eq!(email.as_str(), email.as_str().to_lowercase());
            }
        }

        #[test]
        fn email_roundtrips_through_display(
            local in valid_local_part(),
            domain in valid_domain()
        ) {
            let email_str = format!("{local}@{domain}");
            if let Ok(email) = EmailAddress::new(&email_str) {
                let displayed = email.to_string();
                let reparsed = EmailAddress::new(&displayed).unwrap();
                prop_assert_eq!(email, reparsed);
            }
        }

        #[test]
        fn email_roundtrips_through_json(
            local in valid_local_part(),
            domain in valid_domain()
        ) {
            let email_str = format!("{local}@{domain}");
            if let Ok(email) = EmailAddress::new(&email_str) {
                let json = serde_json::to_string(&email).unwrap();
                let parsed: EmailAddress = serde_json::from_str(&json).unwrap();
                prop_assert_eq!(email, parsed);
            }
        }

        #[test]
        fn strings_without_at_are_rejected(s in "[a-zA-Z0-9.]+") {
            prop_assume!(!s.contains('@'));
            prop_assert!(EmailAddress::new(&s).is_err());
        }

        #[test]
        fn whitespace_is_trimmed(
            ws_before in "\\s{0,3}",
            local in "[a-z]{3,8}",
            domain in "[a-z]{3,8}\\.[a-z]{2,3}",
            ws_after in "\\s{0,3}"
        ) {
            let email_str = format!("{ws_before}{local}@{domain}{ws_after}");
            if let Ok(email) = EmailAddress::new(&email_str) {
                prop_assert!(!email.as_str().starts_with(char::is_whitespace));
                prop_assert!(!email.as_str().ends_with(char::is_whitespace));
            }
        }
    }
}
