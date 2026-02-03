//! Phone number value object with E.164 validation

use serde::{Deserialize, Serialize};
use std::fmt;

use crate::errors::DomainError;

/// A validated phone number in E.164 format (e.g., +491234567890)
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PhoneNumber {
    value: String,
}

impl PhoneNumber {
    /// Create a new phone number, validating E.164 format
    ///
    /// E.164 format: +[country code][subscriber number]
    /// - Starts with +
    /// - Contains only digits after +
    /// - Length: 8-15 digits (including country code)
    pub fn new(number: impl Into<String>) -> Result<Self, DomainError> {
        let value = number.into().trim().replace([' ', '-', '(', ')'], "");

        if !value.starts_with('+') {
            return Err(DomainError::InvalidPhoneNumber(
                "Phone number must start with +".to_string(),
            ));
        }

        let digits = &value[1..];
        if !digits.chars().all(|c| c.is_ascii_digit()) {
            return Err(DomainError::InvalidPhoneNumber(
                "Phone number must contain only digits after +".to_string(),
            ));
        }

        if digits.len() < 7 || digits.len() > 15 {
            return Err(DomainError::InvalidPhoneNumber(
                "Phone number must have 7-15 digits".to_string(),
            ));
        }

        Ok(Self { value })
    }

    /// Get the phone number as a string slice (E.164 format)
    pub fn as_str(&self) -> &str {
        &self.value
    }

    /// Get digits only (without +)
    pub fn digits(&self) -> &str {
        &self.value[1..]
    }

    /// Check if this is a German number (+49)
    pub fn is_german(&self) -> bool {
        self.value.starts_with("+49")
    }
}

impl fmt::Display for PhoneNumber {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.value)
    }
}

impl TryFrom<String> for PhoneNumber {
    type Error = DomainError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl TryFrom<&str> for PhoneNumber {
    type Error = DomainError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_e164_number_is_accepted() {
        let phone = PhoneNumber::new("+491234567890").unwrap();
        assert_eq!(phone.as_str(), "+491234567890");
    }

    #[test]
    fn number_with_spaces_is_normalized() {
        let phone = PhoneNumber::new("+49 123 456 7890").unwrap();
        assert_eq!(phone.as_str(), "+491234567890");
    }

    #[test]
    fn number_without_plus_is_rejected() {
        assert!(PhoneNumber::new("491234567890").is_err());
    }

    #[test]
    fn number_with_letters_is_rejected() {
        assert!(PhoneNumber::new("+49123abc").is_err());
    }

    #[test]
    fn too_short_number_is_rejected() {
        assert!(PhoneNumber::new("+12345").is_err());
    }

    #[test]
    fn german_number_is_detected() {
        let phone = PhoneNumber::new("+491234567890").unwrap();
        assert!(phone.is_german());

        let us_phone = PhoneNumber::new("+11234567890").unwrap();
        assert!(!us_phone.is_german());
    }
}
