//! Phone number value object with E.164 validation

use std::fmt;

use serde::{Deserialize, Serialize};

use crate::errors::DomainError;

/// A validated phone number in E.164 format (e.g., +491234567890)
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
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

    #[test]
    fn digits_returns_without_plus() {
        let phone = PhoneNumber::new("+491234567890").unwrap();
        assert_eq!(phone.digits(), "491234567890");
    }

    #[test]
    fn display_format() {
        let phone = PhoneNumber::new("+491234567890").unwrap();
        assert_eq!(phone.to_string(), "+491234567890");
    }

    #[test]
    fn try_from_string() {
        let phone: PhoneNumber = "+491234567890".to_string().try_into().unwrap();
        assert_eq!(phone.as_str(), "+491234567890");
    }

    #[test]
    fn try_from_str() {
        let phone: PhoneNumber = "+491234567890".try_into().unwrap();
        assert_eq!(phone.as_str(), "+491234567890");
    }

    #[test]
    fn number_with_dashes_and_parens_normalized() {
        let phone = PhoneNumber::new("+49-(123)-456-7890").unwrap();
        assert_eq!(phone.as_str(), "+491234567890");
    }

    #[test]
    fn too_long_number_is_rejected() {
        assert!(PhoneNumber::new("+12345678901234567890").is_err());
    }

    #[test]
    fn serialization() {
        let phone = PhoneNumber::new("+491234567890").unwrap();
        let json = serde_json::to_string(&phone).unwrap();
        let parsed: PhoneNumber = serde_json::from_str(&json).unwrap();
        assert_eq!(phone, parsed);
    }

    #[test]
    fn hash_works() {
        use std::collections::HashSet;
        let p1 = PhoneNumber::new("+491234567890").unwrap();
        let p2 = PhoneNumber::new("+491234567891").unwrap();
        let mut set = HashSet::new();
        set.insert(p1);
        set.insert(p2);
        assert_eq!(set.len(), 2);
    }

    #[test]
    fn clone() {
        let phone = PhoneNumber::new("+491234567890").unwrap();
        #[allow(clippy::redundant_clone)]
        let cloned = phone.clone();
        assert_eq!(phone, cloned);
    }
}

#[cfg(test)]
mod proptest_tests {
    use proptest::prelude::*;

    use super::*;

    proptest! {
        #[test]
        fn valid_e164_numbers_accepted(digits in "[0-9]{7,14}") {
            let phone_str = format!("+{digits}");
            let result = PhoneNumber::new(&phone_str);
            // Should succeed for valid length
            if digits.len() >= 7 && digits.len() <= 15 {
                prop_assert!(result.is_ok());
            }
        }

        #[test]
        fn phone_is_normalized(
            cc in "[0-9]{1,3}",
            area in "[0-9]{2,4}",
            number in "[0-9]{4,8}"
        ) {
            // With random separators
            let with_separators = format!("+{cc}-({area})-{number}");
            if let Ok(phone) = PhoneNumber::new(&with_separators) {
                // Should contain only + and digits
                prop_assert!(phone.as_str().starts_with('+'));
                prop_assert!(phone.as_str().chars().skip(1).all(|c| c.is_ascii_digit()));
            }
        }

        #[test]
        fn phone_roundtrips_through_display(digits in "[0-9]{7,12}") {
            let phone_str = format!("+{digits}");
            if let Ok(phone) = PhoneNumber::new(&phone_str) {
                let displayed = phone.to_string();
                let reparsed = PhoneNumber::new(&displayed).unwrap();
                prop_assert_eq!(phone, reparsed);
            }
        }

        #[test]
        fn phone_roundtrips_through_json(digits in "[0-9]{7,12}") {
            let phone_str = format!("+{digits}");
            if let Ok(phone) = PhoneNumber::new(&phone_str) {
                let json = serde_json::to_string(&phone).unwrap();
                let parsed: PhoneNumber = serde_json::from_str(&json).unwrap();
                prop_assert_eq!(phone, parsed);
            }
        }

        #[test]
        fn numbers_without_plus_rejected(digits in "[0-9]{7,14}") {
            let result = PhoneNumber::new(&digits);
            prop_assert!(result.is_err());
        }

        #[test]
        fn numbers_with_letters_rejected(
            digits in "[0-9]{3,6}",
            letters in "[a-zA-Z]{1,3}",
            more_digits in "[0-9]{3,6}"
        ) {
            let phone_str = format!("+{digits}{letters}{more_digits}");
            let result = PhoneNumber::new(&phone_str);
            prop_assert!(result.is_err());
        }

        #[test]
        fn german_number_detection(number in "[0-9]{8,11}") {
            let german_str = format!("+49{number}");
            if let Ok(phone) = PhoneNumber::new(&german_str) {
                prop_assert!(phone.is_german());
            }

            // Non-German numbers
            let us_str = format!("+1{number}");
            if let Ok(phone) = PhoneNumber::new(&us_str) {
                prop_assert!(!phone.is_german());
            }
        }
    }
}
